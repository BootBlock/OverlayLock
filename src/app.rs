//! The state of one lock: the options, the password matcher, the overlay windows and the
//! controller window, and the single path that unlocks.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, PostMessageW, PostQuitMessage, SW_SHOW, SW_SHOWNOACTIVATE, SetForegroundWindow,
    ShowWindow,
};

use crate::cli::Options;
use crate::focus;
use crate::hooks::keyboard::KeyboardConfig;
use crate::hooks::{HookConfig, HookThread};
use crate::monitors::{self, Change, MonitorId, Monitors};
use crate::overlay::{
    Overlay, OverlaySet, WM_APP_DISPLAY_CHANGED, WM_APP_UNLOCK, controller, window,
};
use crate::password::PasswordMatcher;
use crate::render::TextRenderer;
use crate::secure_desktop::{Action, SasDetector, SecureDesktopWatch, Signal};

/// The most times the focus timer repeats the foreground sequence (8 × 250 ms = 2 s, OL10).
const MAX_FOCUS_ATTEMPTS: u32 = 8;

/// How many times the overlays are raised again after a display change (40 × 250 ms = 10 s).
///
/// Other topmost windows, such as desktop bars, set `HWND_TOPMOST` again when the displays change,
/// which puts them above the overlays. A bar that rebuilds its windows after a monitor is removed
/// did so more than 2 s after the change on the owner's machine, so the raise repeats for 10 s.
pub const RAISE_REPEATS: u32 = 40;

/// The state of one lock, shared by every window through an [`Rc`].
///
/// Every mutable part is a [`Cell`] or a [`RefCell`], and no borrow is held across a Win32 call
/// that can send a message (OL18).
pub struct App {
    options: Options,
    instance: HINSTANCE,
    /// The message renderer, made only when `--message` is given (OL8).
    text: Option<TextRenderer>,
    matcher: RefCell<Option<PasswordMatcher>>,
    overlays: OverlaySet,
    /// The monitor under the cursor at start. With `--monitors cursor`, the covered monitor.
    kept: RefCell<Option<MonitorId>>,
    controller: Cell<Option<HWND>>,
    unlocking: Cell<bool>,
    display_change_pending: Cell<bool>,
    /// `true` once an overlay was foreground: the app never takes the focus again (OL10).
    focus_settled: Cell<bool>,
    focus_attempts: Cell<u32>,
    /// How many raises are left after the last display change.
    raises_left: Cell<u32>,
    /// The input hook thread, while `--block-keys` or `--block-mouse` hooks are installed.
    hooks: RefCell<Option<HookThread>>,
    torn_down: Cell<bool>,
    /// The desktop and session watch, while hooks run or `--yield-on-ctrl-alt-del` waits.
    watch: RefCell<Option<SecureDesktopWatch>>,
    detector: RefCell<SasDetector>,
    /// `true` once the lock stepped back after Ctrl+Alt+Del. It lasts until unlock (OL15).
    yielded: Cell<bool>,
}

impl App {
    /// Initialises a new instance of the [`App`] type for the given options.
    pub fn new(options: Options) -> windows::core::Result<Self> {
        // SAFETY: `None` asks for the handle of the executable itself, which needs no release.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
        let matcher = PasswordMatcher::new(options.password.clone());
        let text = options
            .message
            .as_ref()
            .map(|style| TextRenderer::new(style, options.background))
            .transpose()?;
        Ok(Self {
            options,
            instance,
            text,
            matcher: RefCell::new(Some(matcher)),
            overlays: OverlaySet::default(),
            kept: RefCell::new(None),
            controller: Cell::new(None),
            unlocking: Cell::new(false),
            display_change_pending: Cell::new(false),
            focus_settled: Cell::new(false),
            focus_attempts: Cell::new(0),
            raises_left: Cell::new(0),
            hooks: RefCell::new(None),
            torn_down: Cell::new(false),
            watch: RefCell::new(None),
            detector: RefCell::new(SasDetector::default()),
            yielded: Cell::new(false),
        })
    }

    /// Gets the message renderer, if `--message` was given.
    pub fn text_renderer(&self) -> Option<&TextRenderer> {
        self.text.as_ref()
    }

    /// Gets the module instance that owns the window classes.
    pub fn instance(&self) -> HINSTANCE {
        self.instance
    }

    /// Gets the overlay windows.
    pub fn overlays(&self) -> &OverlaySet {
        &self.overlays
    }

    /// Registers the window classes, makes the controller and covers the monitors.
    pub fn start(self: &Rc<Self>) -> windows::core::Result<()> {
        // SAFETY: plain GDI call; the brush is the overlay class background for the process life.
        let brush = unsafe { CreateSolidBrush(self.options.background.to_colorref()) };
        window::register(self.instance, brush)?;
        controller::register(self.instance)?;
        let controller = controller::create(self)?;
        self.controller.set(Some(controller));
        // The hooks go in before any overlay shows: a key typed as soon as the screen goes dark
        // must reach the matcher, which `--block-keys` moves to the hook thread.
        self.start_hooks(controller)?;
        *self.kept.borrow_mut() = Some(monitors::monitor_at_cursor()?);
        self.reconcile(true)?;
        if let Some(timeout) = self.options.timeout {
            controller::start_timeout(controller, timeout)?;
        }
        if !self.take_focus() {
            controller::start_focus_timer(controller)?;
        }
        let hooked = self.hooks.borrow().is_some();
        if hooked || self.options.yield_on_ctrl_alt_del {
            *self.watch.borrow_mut() = Some(SecureDesktopWatch::start(controller)?);
        }
        Ok(())
    }

    /// Handles one desktop or session signal and returns what the controller must do.
    ///
    /// The hooks pass input on while the session is locked, so the lock screen on the same
    /// desktop works, and reload their key state on each return. With `--yield-on-ctrl-alt-del`,
    /// the signal also goes to the Ctrl+Alt+Del detector.
    pub fn desktop_signal(&self, signal: Signal) -> Action {
        if self.watch.borrow().is_none() {
            return Action::None;
        }
        if let Some(hooks) = self.hooks.borrow().as_ref() {
            match signal {
                Signal::SessionLock => hooks.set_paused(true),
                Signal::SessionUnlock => {
                    hooks.set_paused(false);
                    hooks.resync();
                }
                Signal::DesktopSwitch { ours: true } => hooks.resync(),
                Signal::DesktopSwitch { ours: false } | Signal::Settled => {}
            }
        }
        if !self.options.yield_on_ctrl_alt_del {
            return Action::None;
        }
        self.detector.borrow_mut().signal(signal)
    }

    /// Steps back after Ctrl+Alt+Del (OL15): removes the hooks, takes the overlays out of the
    /// topmost band and stops watching. The overlays stay, and the password (now through
    /// `WM_CHAR` on a focused overlay) and the timeout still unlock. It lasts until unlock.
    pub fn yield_lock(&self, foreground: Option<HWND>) {
        if self.yielded.replace(true) {
            return;
        }
        let watch = self.watch.borrow_mut().take();
        drop(watch);
        let hooks = self.hooks.borrow_mut().take();
        if let Some(mut matcher) = hooks.and_then(HookThread::stop) {
            matcher.clear();
            *self.matcher.borrow_mut() = Some(matcher);
        }
        let ours = self.overlays.hwnds();
        let foreground = foreground.filter(|window| !ours.contains(window));
        for hwnd in ours {
            window::lower(hwnd, foreground);
        }
    }

    /// Installs the input hooks the options ask for. With `--block-keys` the password matcher
    /// moves to the hook thread. With only `--block-mouse`, the password still arrives through
    /// `WM_CHAR` on the focused overlay; the focus step uses no input, so the hook never blocks it.
    fn start_hooks(&self, controller: HWND) -> windows::core::Result<()> {
        if !self.options.block_keys && !self.options.block_mouse {
            return Ok(());
        }
        let keyboard = if self.options.block_keys {
            let matcher = self.matcher.borrow_mut().take();
            matcher.map(|matcher| KeyboardConfig {
                matcher,
                alt_f4_unlocks: self.options.alt_f4_unlocks,
            })
        } else {
            None
        };
        let config = HookConfig {
            keyboard,
            mouse: self.options.block_mouse,
        };
        let hooks = HookThread::start(config, controller)?;
        *self.hooks.borrow_mut() = Some(hooks);
        Ok(())
    }

    /// Brings the cursor monitor's overlay to the foreground unless an overlay was foreground
    /// before. Returns `true` once an overlay has been foreground; otherwise, `false`.
    fn take_focus(&self) -> bool {
        if self.focus_settled.get() {
            return true;
        }
        let ours = self.overlays.hwnds();
        let settled = if focus::is_ours_foreground(&ours) {
            true
        } else {
            self.focus_target()
                .is_some_and(|target| focus::force_foreground(target, &ours))
        };
        self.focus_settled.set(settled);
        settled
    }

    /// Runs on each focus timer tick. Returns `true` to keep the timer; otherwise, `false`.
    pub fn focus_tick(&self) -> bool {
        if self.take_focus() {
            return false;
        }
        let attempts = self.focus_attempts.get() + 1;
        self.focus_attempts.set(attempts);
        attempts < MAX_FOCUS_ATTEMPTS
    }

    /// Reconciles the overlays after a display change and raises them above every other topmost
    /// window. Returns `true` if the caller should start the raise timer; otherwise, `false`.
    pub fn after_display_change(self: &Rc<Self>) -> bool {
        // A failure leaves every overlay that could be placed, and the lock stays in place, so
        // there is nothing better to do with the error here.
        let _ = self.reconcile(false);
        self.raise_overlays();
        let already_running = self.raises_left.get() > 0;
        self.raises_left.set(RAISE_REPEATS);
        !already_running
    }

    /// Runs on each raise timer tick. Returns `true` to keep the timer; otherwise, `false`.
    pub fn raise_tick(&self) -> bool {
        self.raise_overlays();
        let left = self.raises_left.get().saturating_sub(1);
        self.raises_left.set(left);
        left > 0
    }

    /// Puts every overlay at the top of the topmost band, without activating it.
    fn raise_overlays(&self) {
        if self.yielded.get() {
            return;
        }
        for hwnd in self.overlays.hwnds() {
            window::raise(hwnd);
        }
    }

    /// Ends the lock after the message loop has ended: destroys the controller.
    pub fn finish(&self) {
        if let Some(controller) = self.controller.take() {
            // SAFETY: the controller is a window of this thread that still exists.
            let _ = unsafe { DestroyWindow(controller) };
        }
    }

    /// Feeds one typed UTF-16 code unit to the password matcher, and unlocks on a match.
    pub fn push_unit(&self, unit: u16) {
        let matched = self
            .matcher
            .borrow_mut()
            .as_mut()
            .is_some_and(|matcher| matcher.push(unit));
        if matched {
            self.unlock();
        }
    }

    /// Handles Alt+F4 or a close request: unlocks if Alt+F4 unlocks, else does nothing.
    pub fn close_requested(&self) {
        if self.options.alt_f4_unlocks {
            self.unlock();
        }
    }

    /// Asks the controller to end the lock. Only the first request counts.
    ///
    /// The request is posted, so the teardown never runs inside the caller's message.
    pub fn unlock(&self) {
        if !self.unlocking.replace(true) && !self.post(WM_APP_UNLOCK, 0) {
            // The request did not reach the controller: let the next one try again.
            self.unlocking.set(false);
        }
    }

    /// Destroys every overlay and ends the message loop. Runs on the controller's message.
    pub fn teardown(&self) {
        if self.torn_down.replace(true) {
            return;
        }
        let watch = self.watch.borrow_mut().take();
        drop(watch);
        let hooks = self.hooks.borrow_mut().take();
        if let Some(hooks) = hooks {
            hooks.stop();
        }
        for hwnd in self.overlays.take_all() {
            // SAFETY: each overlay is a window of this thread that still exists.
            let _ = unsafe { DestroyWindow(hwnd) };
        }
        // SAFETY: plain call on the UI thread.
        unsafe { PostQuitMessage(0) };
    }

    /// Notes that the displays changed. Each overlay receives the broadcast, so only the first
    /// one asks the controller to reconcile.
    pub fn display_changed(&self) {
        if !self.display_change_pending.replace(true) && !self.post(WM_APP_DISPLAY_CHANGED, 0) {
            self.display_change_pending.set(false);
        }
    }

    /// Brings the overlay set in line with the monitors: one window per covered monitor, each
    /// sized to its monitor. `activate` shows the cursor monitor's overlay with activation.
    ///
    /// Every change is attempted even when one fails, so one failed window never leaves another
    /// monitor uncovered; the first failure is returned.
    pub fn reconcile(self: &Rc<Self>, activate: bool) -> windows::core::Result<()> {
        self.display_change_pending.set(false);
        let all = monitors::enumerate()?;
        let kept = self.kept.borrow().clone();
        let wanted = monitors::select(&all, self.options.monitors, kept.as_ref());
        if self.options.monitors == Monitors::Cursor {
            *self.kept.borrow_mut() = wanted.first().map(|monitor| monitor.id.clone());
        }
        let kept = self.kept.borrow().clone();
        let mut first_error = None;
        let mut focus_orphaned = false;
        for change in monitors::reconcile(&self.overlays.current(), &wanted) {
            match change {
                Change::Destroy(id) => {
                    if let Some(hwnd) = self.overlays.remove(&id) {
                        focus_orphaned |= !self.hand_over_focus(hwnd);
                        // SAFETY: the overlay is a window of this thread that still exists.
                        let _ = unsafe { DestroyWindow(hwnd) };
                    }
                }
                Change::Move { id, rect } => {
                    self.overlays.set_rect(&id, rect);
                    if let Some(hwnd) = self.overlays.hwnd_of(&id)
                        && let Err(error) = window::place(hwnd, rect)
                    {
                        first_error.get_or_insert(error);
                    }
                }
                Change::Create(monitor) => {
                    let hwnd = match window::create(self, monitor.rect, !self.yielded.get()) {
                        Ok(hwnd) => hwnd,
                        Err(error) => {
                            first_error.get_or_insert(error);
                            continue;
                        }
                    };
                    self.overlays.insert(Overlay {
                        id: monitor.id.clone(),
                        rect: monitor.rect,
                        hwnd,
                    });
                    let show = if activate && kept.as_ref() == Some(&monitor.id) {
                        SW_SHOW
                    } else {
                        SW_SHOWNOACTIVATE
                    };
                    // SAFETY: `hwnd` was just created on this thread.
                    let _ = unsafe { ShowWindow(hwnd, show) };
                }
            }
        }
        // The focused overlay went and no other overlay existed to take the focus over (with
        // `--monitors cursor`). The lock let it go itself, so it takes it back: the one exception
        // OL10 allows.
        if focus_orphaned
            && !self.yielded.get()
            && let Some(target) = self.focus_target()
        {
            focus::force_foreground(target, &self.overlays.hwnds());
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Moves the focus from an overlay that is about to be destroyed (its monitor was removed) to
    /// another overlay, so typed keys keep reaching the lock. It acts only while `leaving` holds
    /// the focus, so the lock never takes the focus from another app (OL10).
    ///
    /// Returns `false` if `leaving` held the focus and no other overlay exists to take it;
    /// otherwise, `true`.
    fn hand_over_focus(&self, leaving: HWND) -> bool {
        if !focus::is_ours_foreground(&[leaving]) {
            return true;
        }
        let Some(target) = self.focus_target() else {
            return false;
        };
        // SAFETY: both are overlays of this thread, and this process holds the foreground.
        let _ = unsafe { SetForegroundWindow(target) };
        true
    }

    /// Gets the overlay that should hold the focus: the one on the cursor's monitor, else the
    /// first.
    pub fn focus_target(&self) -> Option<HWND> {
        let kept = self.kept.borrow().clone();
        kept.and_then(|id| self.overlays.hwnd_of(&id))
            .or_else(|| self.overlays.hwnds().first().copied())
    }

    /// Posts a message to the controller. Returns `true` if it was queued; otherwise, `false`.
    fn post(&self, message: u32, wparam: usize) -> bool {
        self.controller.get().is_some_and(|controller| {
            // SAFETY: posting to a window of this thread with plain integer parameters.
            unsafe { PostMessageW(Some(controller), message, WPARAM(wparam), LPARAM(0)) }.is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::cli::{Command, parse};

    fn app(args: &[&str]) -> App {
        let Ok(Command::Run(options)) = parse(args.iter().map(std::ffi::OsString::from)) else {
            panic!("{args:?} is not a lock command line");
        };
        App::new(options).expect("the app builds")
    }

    #[test]
    fn no_text_renderer_without_message() {
        assert!(app(&["-p", "go"]).text_renderer().is_none());
    }

    #[test]
    fn text_renderer_with_message() {
        assert!(
            app(&["-p", "go", "--message", "Locked"])
                .text_renderer()
                .is_some()
        );
    }
}
