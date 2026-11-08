//! Watches desktop switches and session locks. The hooks pass input on while the session is
//! locked and reload their key state on each return, and `--yield-on-ctrl-alt-del` detects the
//! return from the Ctrl+Alt+Del screen (OL15), told apart from a Win+L lock.
//!
//! Measured on Windows 11 (build 26100): each visit to the secure desktop (Ctrl+Alt+Del, the
//! lock screen's sign-in) gives one `EVENT_SYSTEM_DESKTOPSWITCH` away from the user's desktop and
//! one back. Win+L adds `WTS_SESSION_LOCK` 11 ms after its first switch away, and
//! `WTS_SESSION_UNLOCK` 31 ms before its last switch back. Ctrl+Alt+Del gives no session message.
//! So a visit is judged as a whole, after a short settle time once the user's desktop is back: a
//! visit with no lock or unlock message is a return from Ctrl+Alt+Del.
//!
//! A UAC prompt on the secure desktop also gives a visit with no session message, so it yields as
//! well. That errs towards unlocking, as OL17 does.

use std::cell::Cell;

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS,
    GetThreadDesktop, GetUserObjectInformationW, HDESK, OpenInputDesktop, UOI_NAME,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_SYSTEM_DESKTOPSWITCH, PostMessageW, WINEVENT_OUTOFCONTEXT, WM_APP,
};

/// Posted to the controller on each desktop switch. `wParam` is 1 if the input desktop is the
/// user's own desktop again; otherwise, 0.
pub const WM_APP_DESKTOP_SWITCH: u32 = WM_APP + 4;

/// How long after the return to wait for a late session message before deciding, in
/// milliseconds. Windows 11 (build 26100) sent the unlock 31 ms before the switch back.
pub const SETTLE_MS: u32 = 500;

/// One thing the detector is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// The input desktop changed. `ours` is `true` if it is the user's desktop again.
    DesktopSwitch {
        /// `true` if the input desktop is the user's desktop; otherwise, `false`.
        ours: bool,
    },
    /// `WTS_SESSION_LOCK` arrived.
    SessionLock,
    /// `WTS_SESSION_UNLOCK` arrived.
    SessionUnlock,
    /// The settle time after a return ran out.
    Settled,
}

/// What the caller must do after a signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing.
    None,
    /// Start (or restart) the settle timer.
    StartSettleTimer,
    /// The user came back from the Ctrl+Alt+Del screen: yield.
    Yield,
}

/// Where the input desktop is, as the detector sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// On the user's desktop, with no decision pending.
    #[default]
    Home,
    /// Away from the user's desktop (the secure desktop or the lock screen).
    Away,
    /// Back on the user's desktop; the settle timer runs before the decision.
    Settling,
}

/// The pure decision logic: no Win32 call, so it is unit-tested.
#[derive(Debug, Default)]
pub struct SasDetector {
    phase: Phase,
    /// `true` if a lock or unlock message arrived during the current visit or its settle time.
    session_changed: bool,
    /// `true` between `WTS_SESSION_LOCK` and `WTS_SESSION_UNLOCK`.
    locked: bool,
}

impl SasDetector {
    /// Applies one signal and returns what to do.
    pub fn signal(&mut self, signal: Signal) -> Action {
        match signal {
            Signal::DesktopSwitch { ours: false } => {
                // Leaving again during the settle time continues the same visit.
                if self.phase == Phase::Home {
                    self.session_changed = false;
                }
                self.phase = Phase::Away;
                Action::None
            }
            Signal::DesktopSwitch { ours: true } => {
                if self.phase != Phase::Away {
                    return Action::None;
                }
                self.phase = Phase::Settling;
                Action::StartSettleTimer
            }
            Signal::SessionLock => {
                self.session_changed = true;
                self.locked = true;
                Action::None
            }
            Signal::SessionUnlock => {
                self.session_changed = true;
                self.locked = false;
                Action::None
            }
            Signal::Settled => {
                if self.phase != Phase::Settling {
                    return Action::None;
                }
                self.phase = Phase::Home;
                let returned_from_ctrl_alt_del = !self.session_changed && !self.locked;
                self.session_changed = false;
                if returned_from_ctrl_alt_del {
                    Action::Yield
                } else {
                    Action::None
                }
            }
        }
    }
}

thread_local! {
    /// The controller that receives `WM_APP_DESKTOP_SWITCH`. The WinEvent callback has no user
    /// pointer; out-of-context callbacks run on the thread that set the hook, the UI thread.
    static CONTROLLER: Cell<Option<HWND>> = const { Cell::new(None) };
}

/// The desktop-switch hook and the session notification, removed on drop. It runs while input
/// hooks are installed or `--yield-on-ctrl-alt-del` waits for its signal.
pub struct SecureDesktopWatch {
    hook: HWINEVENTHOOK,
    controller: HWND,
}

impl SecureDesktopWatch {
    /// Starts watching. Desktop switches arrive as `WM_APP_DESKTOP_SWITCH` and session changes as
    /// `WM_WTSSESSION_CHANGE`, both on `controller`.
    pub fn start(controller: HWND) -> windows::core::Result<Self> {
        CONTROLLER.set(Some(controller));
        // SAFETY: `controller` is a window of this thread; it is unregistered in `drop`.
        unsafe { WTSRegisterSessionNotification(controller, NOTIFY_FOR_THIS_SESSION) }?;
        // SAFETY: an out-of-context hook on this thread with a valid callback; removed in `drop`.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_DESKTOPSWITCH,
                EVENT_SYSTEM_DESKTOPSWITCH,
                None,
                Some(desktop_switched),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if hook.is_invalid() {
            // SAFETY: undoes the registration above.
            let _ = unsafe { WTSUnRegisterSessionNotification(controller) };
            return Err(windows::core::Error::from_thread());
        }
        Ok(Self { hook, controller })
    }
}

impl Drop for SecureDesktopWatch {
    fn drop(&mut self) {
        // SAFETY: both were set up by `start` on this thread and are removed once.
        unsafe {
            let _ = UnhookWinEvent(self.hook);
            let _ = WTSUnRegisterSessionNotification(self.controller);
        }
        CONTROLLER.set(None);
    }
}

unsafe extern "system" fn desktop_switched(
    _: HWINEVENTHOOK,
    _: u32,
    _: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    if let Some(controller) = CONTROLLER.get() {
        let ours = usize::from(input_desktop_is_ours());
        // SAFETY: posting plain integers to a window of this thread.
        let _ = unsafe {
            PostMessageW(
                Some(controller),
                WM_APP_DESKTOP_SWITCH,
                WPARAM(ours),
                LPARAM(0),
            )
        };
    }
}

/// Returns `true` if the input desktop is the one this thread runs on; otherwise, `false`. The
/// secure desktop cannot be opened by the user (access denied), which also gives `false`.
fn input_desktop_is_ours() -> bool {
    // SAFETY: plain query; the thread desktop handle needs no release.
    let Ok(own) = (unsafe { GetThreadDesktop(GetCurrentThreadId()) }) else {
        return false;
    };
    // SAFETY: opens the input desktop for reading its name only; closed below.
    let Ok(input) = (unsafe {
        OpenInputDesktop(
            DESKTOP_CONTROL_FLAGS(0),
            false,
            DESKTOP_ACCESS_FLAGS(DESKTOP_READOBJECTS.0),
        )
    }) else {
        return false;
    };
    let same = desktop_name(input).is_some_and(|name| desktop_name(own) == Some(name));
    // SAFETY: `input` was opened above and is closed once.
    let _ = unsafe { CloseDesktop(input) };
    same
}

/// Gets a desktop's name, up to 64 UTF-16 units, without allocating.
fn desktop_name(desktop: HDESK) -> Option<([u16; 64], usize)> {
    let mut name = [0u16; 64];
    let size = u32::try_from(size_of_val(&name)).ok()?;
    // SAFETY: the buffer is writable and its size in bytes is given.
    unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(name.as_mut_ptr().cast()),
            size,
            None,
        )
    }
    .ok()?;
    let len = name
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(name.len());
    Some((name, len))
}

#[cfg(test)]
mod tests {
    use super::{Action, SasDetector, Signal};

    const AWAY: Signal = Signal::DesktopSwitch { ours: false };
    const BACK: Signal = Signal::DesktopSwitch { ours: true };

    fn run(signals: &[Signal]) -> Vec<Action> {
        let mut detector = SasDetector::default();
        signals
            .iter()
            .map(|&signal| detector.signal(signal))
            .collect()
    }

    fn yields(signals: &[Signal]) -> usize {
        run(signals).iter().filter(|&&a| a == Action::Yield).count()
    }

    #[test]
    fn ctrl_alt_del_then_cancel_yields() {
        assert_eq!(
            run(&[AWAY, BACK, Signal::Settled]),
            [Action::None, Action::StartSettleTimer, Action::Yield]
        );
    }

    /// The order Windows 11 (build 26100) gives for Win+L and sign-in.
    #[test]
    fn win_l_and_sign_in_never_yields() {
        let sequence = [
            AWAY,
            Signal::SessionLock,
            BACK,
            Signal::Settled,
            AWAY,
            Signal::SessionUnlock,
            BACK,
            Signal::Settled,
        ];
        assert_eq!(yields(&sequence), 0);
    }

    #[test]
    fn late_unlock_inside_the_settle_time_does_not_yield() {
        let sequence = [
            AWAY,
            Signal::SessionLock,
            BACK,
            Signal::Settled,
            AWAY,
            BACK,
            Signal::SessionUnlock,
            Signal::Settled,
        ];
        assert_eq!(yields(&sequence), 0);
    }

    #[test]
    fn ctrl_alt_del_after_a_lock_cycle_yields() {
        let sequence = [
            AWAY,
            Signal::SessionLock,
            BACK,
            Signal::Settled,
            AWAY,
            Signal::SessionUnlock,
            BACK,
            Signal::Settled,
            AWAY,
            BACK,
            Signal::Settled,
        ];
        assert_eq!(run(&sequence).last(), Some(&Action::Yield));
        assert_eq!(yields(&sequence), 1);
    }

    #[test]
    fn ctrl_alt_del_then_lock_from_its_screen_does_not_yield() {
        let sequence = [AWAY, Signal::SessionLock, BACK, Signal::Settled];
        assert_eq!(yields(&sequence), 0);
    }

    #[test]
    fn a_settle_with_no_visit_does_nothing() {
        assert_eq!(run(&[Signal::Settled]), [Action::None]);
        assert_eq!(run(&[BACK]), [Action::None]);
    }

    #[test]
    fn leaving_again_before_the_settle_time_waits_for_the_next_return() {
        let sequence = [AWAY, BACK, AWAY, Signal::Settled, BACK, Signal::Settled];
        assert_eq!(
            run(&sequence),
            [
                Action::None,
                Action::StartSettleTimer,
                Action::None,
                Action::None,
                Action::StartSettleTimer,
                Action::Yield,
            ]
        );
    }
}
