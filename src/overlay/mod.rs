//! The overlay windows, one per covered monitor, and the message-only controller window.

pub mod controller;
pub mod window;

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, DefWindowProcW, GWLP_USERDATA, GetWindowLongPtrW, PostMessageW,
    SetWindowLongPtrW, WM_APP, WM_NCCREATE, WM_NCDESTROY,
};

use crate::monitors::{MonitorId, Rect};

/// Posted to the controller to unlock, by the UI thread, the hook thread (Alt+F4) or the match
/// thread (the password). Every cause (the password, Alt+F4, the timeout, the end of the session)
/// ends the lock the same way.
pub const WM_APP_UNLOCK: u32 = WM_APP + 1;

/// Posted to the controller by an overlay that received `WM_DISPLAYCHANGE`.
pub const WM_APP_DISPLAY_CHANGED: u32 = WM_APP + 2;

/// The controller window's handle, which the hook thread and the match thread hold so they can
/// ask it to unlock.
#[derive(Clone, Copy, Debug)]
pub struct ControllerHandle(HWND);

// SAFETY: a window handle is an opaque value that any thread may pass to `PostMessageW`.
unsafe impl Send for ControllerHandle {}

impl ControllerHandle {
    /// Initialises a new instance of the [`ControllerHandle`] type for the controller window.
    pub fn new(controller: HWND) -> Self {
        Self(controller)
    }

    /// Posts [`WM_APP_UNLOCK`] to the controller. Never blocks, so a hook may call it.
    pub fn post_unlock(self) {
        // SAFETY: posting plain integers to a window never blocks; a window that is gone makes
        // the call fail, which is harmless.
        let _ = unsafe { PostMessageW(Some(self.0), WM_APP_UNLOCK, WPARAM(0), LPARAM(0)) };
    }
}

/// One overlay window and the monitor it covers.
#[derive(Clone, Debug)]
pub struct Overlay {
    /// The monitor the window covers.
    pub id: MonitorId,
    /// The monitor rectangle the window is sized to.
    pub rect: Rect,
    /// The window.
    pub hwnd: HWND,
}

/// The overlay windows that exist now.
///
/// Every method borrows the list only for its own body and never across a Win32 call, because
/// creating, moving and destroying a window sends messages to the window procedure, which may
/// read the list again (OL18).
#[derive(Debug, Default)]
pub struct OverlaySet {
    overlays: RefCell<Vec<Overlay>>,
}

impl OverlaySet {
    /// Gets each overlay's monitor and rectangle, as [`crate::monitors::reconcile`] takes them.
    pub fn current(&self) -> Vec<(MonitorId, Rect)> {
        self.overlays
            .borrow()
            .iter()
            .map(|overlay| (overlay.id.clone(), overlay.rect))
            .collect()
    }

    /// Gets every overlay window.
    pub fn hwnds(&self) -> Vec<HWND> {
        self.overlays
            .borrow()
            .iter()
            .map(|overlay| overlay.hwnd)
            .collect()
    }

    /// Gets the window that covers a monitor.
    pub fn hwnd_of(&self, id: &MonitorId) -> Option<HWND> {
        self.overlays
            .borrow()
            .iter()
            .find(|overlay| &overlay.id == id)
            .map(|overlay| overlay.hwnd)
    }

    /// Gets the rectangle a window is meant to cover.
    pub fn rect_of(&self, hwnd: HWND) -> Option<Rect> {
        self.overlays
            .borrow()
            .iter()
            .find(|overlay| overlay.hwnd == hwnd)
            .map(|overlay| overlay.rect)
    }

    /// Records a new overlay.
    pub fn insert(&self, overlay: Overlay) {
        self.overlays.borrow_mut().push(overlay);
    }

    /// Records a monitor's new rectangle.
    pub fn set_rect(&self, id: &MonitorId, rect: Rect) {
        if let Some(overlay) = self
            .overlays
            .borrow_mut()
            .iter_mut()
            .find(|overlay| &overlay.id == id)
        {
            overlay.rect = rect;
        }
    }

    /// Forgets a monitor's overlay and returns its window.
    pub fn remove(&self, id: &MonitorId) -> Option<HWND> {
        let mut overlays = self.overlays.borrow_mut();
        let index = overlays.iter().position(|overlay| &overlay.id == id)?;
        Some(overlays.remove(index).hwnd)
    }

    /// Forgets every overlay and returns their windows.
    pub fn take_all(&self) -> Vec<HWND> {
        self.overlays
            .borrow_mut()
            .drain(..)
            .map(|overlay| overlay.hwnd)
            .collect()
    }
}

/// Runs a window procedure with the state the window was created with.
///
/// The state is a `Box<T>` whose raw pointer is passed to `CreateWindowExW` as `lpParam`. It is
/// stored in `GWLP_USERDATA` at `WM_NCCREATE` and dropped at `WM_NCDESTROY`. A message before
/// `WM_NCCREATE` or after `WM_NCDESTROY` goes to `DefWindowProcW`.
///
/// # Safety
///
/// Call only from a window procedure, with its own arguments, for a window whose `lpParam` was
/// `Box::<T>::into_raw` of the same `T`.
pub unsafe fn dispatch<T>(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    handle: impl FnOnce(&T) -> Option<LRESULT>,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: for `WM_NCCREATE`, `lParam` points to the `CREATESTRUCTW` of this creation,
        // whose `lpCreateParams` is the `Box<T>` pointer (the caller's contract).
        let state = unsafe { (*(lparam.0 as *const CREATESTRUCTW)).lpCreateParams };
        // The value is pointer-sized on every target: `isize` on 64-bit, and `i32` on 32-bit x86,
        // where the `windows` crate maps `SetWindowLongPtrW` to `SetWindowLongW`.
        // SAFETY: `hwnd` is the window being created.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as _) };
    }
    // SAFETY: `hwnd` is a valid window of this thread.
    let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut T;
    if state.is_null() {
        // SAFETY: plain default handling for a window with no state.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    if message == WM_NCDESTROY {
        // SAFETY: `hwnd` is still valid during `WM_NCDESTROY`; clearing the pointer first means no
        // later message can reach the state dropped below.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
        // SAFETY: the pointer came from `Box::into_raw` (the caller's contract) and is dropped once,
        // here, as the window's last message.
        drop(unsafe { Box::from_raw(state) });
        // SAFETY: plain default handling.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    // SAFETY: the state lives until `WM_NCDESTROY` above, which cannot arrive while this shared
    // borrow is in use: no window is destroyed from inside its own window procedure. Overlays are
    // destroyed from the controller's handlers, and the controller after the message loop ends.
    let result = handle(unsafe { &*state });
    // SAFETY: plain default handling for a message the handler left alone.
    result.unwrap_or_else(|| unsafe { DefWindowProcW(hwnd, message, wparam, lparam) })
}
