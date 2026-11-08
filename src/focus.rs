//! Takes the foreground for an overlay, as OverlayLockLite did, so typed keys reach it even when a
//! launcher such as a Stream Deck kept the focus.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SW_SHOW, SetForegroundWindow,
    ShowWindow,
};

/// Brings `hwnd` to the foreground, borrowing the foreground thread's input state for the call,
/// and returns `true` if one of `ours` is the foreground window afterwards; otherwise, `false`.
pub fn force_foreground(hwnd: HWND, ours: &[HWND]) -> bool {
    // SAFETY: plain queries with no preconditions; a null foreground window gives thread 0.
    let (foreground_thread, own_thread) = unsafe {
        (
            GetWindowThreadProcessId(GetForegroundWindow(), None),
            GetCurrentThreadId(),
        )
    };
    let attach = foreground_thread != 0 && foreground_thread != own_thread;
    // SAFETY: `hwnd` is an overlay of this thread. The two threads are attached only for the
    // duration of these calls and detached at once.
    unsafe {
        if attach {
            let _ = AttachThreadInput(foreground_thread, own_thread, true);
        }
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if attach {
            let _ = AttachThreadInput(foreground_thread, own_thread, false);
        }
    }
    is_ours_foreground(ours)
}

/// Returns `true` if one of `ours` is the foreground window; otherwise, `false`.
pub fn is_ours_foreground(ours: &[HWND]) -> bool {
    // SAFETY: plain query with no preconditions.
    let foreground = unsafe { GetForegroundWindow() };
    !foreground.is_invalid() && ours.contains(&foreground)
}
