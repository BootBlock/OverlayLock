//! The low-level input hooks (OL11, OL12), installed on a dedicated thread with its own message
//! loop at the highest thread priority.

pub mod keyboard;
pub mod mouse;
pub mod translate;

use std::cell::Cell;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{E_FAIL, HINSTANCE, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, HHOOK, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_QUIT,
};

use keyboard::{KEYBOARD, KeyboardConfig, KeyboardState, keyboard_proc};
use mouse::mouse_proc;
use translate::KeyTranslator;

use crate::overlay::ControllerHandle;

/// Posted to the hook thread: `wParam` 1 passes input on (the session is locked), 0 swallows it.
const WM_APP_HOOK_PAUSE: u32 = WM_APP + 16;

/// Posted to the hook thread: reload the held keys after a visit to another desktop.
const WM_APP_HOOK_RESYNC: u32 = WM_APP + 17;

thread_local! {
    /// `true` while the session is locked: the hooks pass input on, so the lock screen, which
    /// runs on the same desktop, can be used to sign in.
    static PAUSED: Cell<bool> = const { Cell::new(false) };
}

/// Which hooks to install. A configuration with neither hook is a programming error: the app
/// does not start the hook thread then.
pub struct HookConfig {
    /// The keyboard hook's configuration, if `--block-keys` was given.
    pub keyboard: Option<KeyboardConfig>,
    /// `true` to install the mouse hook (`--block-mouse`); otherwise, `false`.
    pub mouse: bool,
}

/// The running hook thread.
pub struct HookThread {
    thread_id: u32,
    handle: JoinHandle<()>,
}

impl HookThread {
    /// Starts the hook thread and waits until its hooks are installed.
    ///
    /// A hook that cannot be installed is an error, never a silently unhooked lock (OL4).
    pub fn start(config: HookConfig, controller: ControllerHandle) -> windows::core::Result<Self> {
        let (report, installed) = mpsc::channel::<windows::core::Result<u32>>();
        let handle = thread::Builder::new()
            .name("overlay-lock-hooks".to_owned())
            .spawn(move || run(config, controller, &report))
            .map_err(|error| windows::core::Error::new(E_FAIL, error.to_string()))?;
        match installed.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, handle }),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                let _ = handle.join();
                Err(windows::core::Error::new(
                    E_FAIL,
                    "the input hook thread ended before it installed its hooks",
                ))
            }
        }
    }

    /// Passes input on (`true`) or swallows it again (`false`).
    pub fn set_paused(&self, paused: bool) {
        self.post(WM_APP_HOOK_PAUSE, usize::from(paused));
    }

    /// Reloads the held keys after a visit to another desktop.
    pub fn resync(&self) {
        self.post(WM_APP_HOOK_RESYNC, 0);
    }

    fn post(&self, message: u32, wparam: usize) {
        // SAFETY: posts plain integers to the hook thread, whose message queue exists.
        let _ = unsafe { PostThreadMessageW(self.thread_id, message, WPARAM(wparam), LPARAM(0)) };
    }

    /// Removes the hooks and ends the thread.
    pub fn stop(self) {
        // SAFETY: posts `WM_QUIT` to the hook thread, whose message queue exists (made in `run`
        // before it reported success).
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        let _ = self.handle.join();
    }
}

/// The hook thread: installs the hooks, reports, pumps messages until `WM_QUIT`, then unhooks.
fn run(
    config: HookConfig,
    controller: ControllerHandle,
    report: &mpsc::Sender<windows::core::Result<u32>>,
) {
    let mut message = MSG::default();
    // SAFETY: plain calls on this thread. `PeekMessageW` makes the thread's message queue, so a
    // `WM_QUIT` posted right after the report below cannot be lost.
    let thread_id = unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        let _ = PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE);
        GetCurrentThreadId()
    };
    let hooks = match install(config, controller) {
        Ok(hooks) => hooks,
        Err(error) => {
            KEYBOARD.take();
            let _ = report.send(Err(error));
            return;
        }
    };
    let _ = report.send(Ok(thread_id));
    // SAFETY: a plain message loop on this thread; the low-level hooks are called from it.
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
        match message.message {
            WM_APP_HOOK_PAUSE => PAUSED.set(message.wParam.0 != 0),
            WM_APP_HOOK_RESYNC => KEYBOARD.with_borrow_mut(|state| {
                if let Some(state) = state {
                    state.resync();
                }
            }),
            // SAFETY: dispatches a message this loop received.
            _ => unsafe {
                DispatchMessageW(&raw const message);
            },
        }
    }
    unhook(hooks);
    KEYBOARD.take();
}

/// Installs the configured hooks on this thread, the keyboard hook first. On failure, removes those
/// already installed and returns the first error.
fn install(config: HookConfig, controller: ControllerHandle) -> windows::core::Result<Vec<HHOOK>> {
    debug_assert!(
        config.keyboard.is_some() || config.mouse,
        "no hook to install"
    );
    // SAFETY: `None` asks for the handle of the executable itself, which needs no release.
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
    let mut hooks = Vec::new();
    if let Some(keyboard) = config.keyboard {
        // SAFETY: plain query; 0 is this thread, whose layout is read once, at install.
        let layout = unsafe { GetKeyboardLayout(0) };
        let mut translator = KeyTranslator::new(layout);
        translator.seed_from_system();
        KEYBOARD.set(Some(KeyboardState::new(keyboard, translator, controller)));
        // SAFETY: the procedure is a valid `HOOKPROC`; thread 0 makes the hook global.
        match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(instance), 0) } {
            Ok(hook) => hooks.push(hook),
            Err(error) => {
                unhook(hooks);
                return Err(error);
            }
        }
    }
    if config.mouse {
        // SAFETY: the procedure is a valid `HOOKPROC`; thread 0 makes the hook global.
        match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(instance), 0) } {
            Ok(hook) => hooks.push(hook),
            Err(error) => {
                unhook(hooks);
                return Err(error);
            }
        }
    }
    Ok(hooks)
}

fn unhook(hooks: Vec<HHOOK>) {
    for hook in hooks {
        // SAFETY: each hook was installed by `install` on this thread and is removed once.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
}
