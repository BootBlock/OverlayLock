//! The `WH_MOUSE_LL` procedure. It swallows every mouse event, movement included (OL12), except
//! while the session is locked. It holds no state and reads nothing beyond the notification code.

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{CallNextHookEx, HC_ACTION};

/// The `WH_MOUSE_LL` procedure: swallows every mouse event.
pub extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if u32::try_from(code) == Ok(HC_ACTION) && !super::PAUSED.get() {
        return LRESULT(1);
    }
    // SAFETY: passes a notification that is not an action on unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
