//! The overlay window class `OverlayLock.Overlay` and its window procedure.

use std::cell::Cell;
use std::rc::Rc;

use windows::Win32::Foundation::D2DERR_RECREATE_TARGET;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HBRUSH, InvalidateRect, PAINTSTRUCT};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, HCURSOR, HWND_NOTOPMOST, HWND_TOPMOST, MA_ACTIVATE,
    RegisterClassExW, SC_CLOSE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetCursor,
    SetWindowPos, WINDOW_EX_STYLE, WM_CHAR, WM_CLOSE, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_ENDSESSION, WM_MOUSEACTIVATE, WM_PAINT, WM_QUERYENDSESSION, WM_SETCURSOR, WM_SIZE,
    WM_SYSCOMMAND, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::app::App;
use crate::monitors::Rect;
use crate::render::WindowText;

/// The overlay window class. Tests find the overlay by this name.
pub const CLASS: PCWSTR = w!("OverlayLock.Overlay");

/// The state of one overlay window.
struct OverlayWindow {
    app: Rc<App>,
    /// The Direct2D resources of the message, made on first paint. It is taken out of the cell
    /// while in use, so no borrow is held across a Direct2D call.
    text: Cell<Option<WindowText>>,
}

/// Registers the overlay class with a null cursor and the background brush.
pub fn register(instance: HINSTANCE, background: HBRUSH) -> windows::core::Result<()> {
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).expect("small struct"),
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        hCursor: HCURSOR::default(),
        hbrBackground: background,
        lpszClassName: CLASS,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and its strings are static.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

/// Creates a hidden overlay window that covers `rect`, topmost unless the lock has yielded.
pub fn create(app: &Rc<App>, rect: Rect, topmost: bool) -> windows::core::Result<HWND> {
    let state = Box::into_raw(Box::new(OverlayWindow {
        app: Rc::clone(app),
        text: Cell::new(None),
    }));
    let ex_style: WINDOW_EX_STYLE = if topmost {
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST
    } else {
        WS_EX_TOOLWINDOW
    };
    // SAFETY: the class is registered; `state` is a `Box<OverlayWindow>` pointer that the window
    // procedure takes over at `WM_NCCREATE` (see `overlay::dispatch`).
    let created = unsafe {
        CreateWindowExW(
            ex_style,
            CLASS,
            w!("OverlayLock"),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            None,
            None,
            Some(app.instance()),
            Some(state.cast_const().cast()),
        )
    };
    // On failure the state is not freed here: a creation that reached `WM_NCCREATE` has already
    // dropped it at `WM_NCDESTROY`, and freeing it again would be a double free. The failure ends
    // the process, so the rare leak is harmless.
    created
}

/// Puts an overlay at the top of the topmost band without moving or activating it.
pub fn raise(hwnd: HWND) {
    // SAFETY: `hwnd` is an overlay window of this thread.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// Takes an overlay out of the topmost band (OL15). If another app's window has the focus, such
/// as Task Manager started from the Ctrl+Alt+Del screen, the overlay goes just below it, so that
/// window stays visible; otherwise the overlay stays at the top of the normal band.
pub fn lower(hwnd: HWND, foreground: Option<HWND>) {
    // SAFETY: `hwnd` is an overlay of this thread; `foreground` is only an insertion point.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        if let Some(foreground) = foreground {
            let _ = SetWindowPos(
                hwnd,
                Some(foreground),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

/// Moves and sizes an overlay to its monitor's rectangle, keeping its z-order and activation.
pub fn place(hwnd: HWND, rect: Rect) -> windows::core::Result<()> {
    // SAFETY: `hwnd` is an overlay window of this thread.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: this is the window procedure of the overlay class, whose windows are all created by
    // `create` with a `Box<OverlayWindow>` as `lpParam`.
    unsafe {
        super::dispatch(hwnd, message, wparam, lparam, |state: &OverlayWindow| {
            handle(state, hwnd, message, wparam)
        })
    }
}

/// Handles one message, or returns `None` for the default handling.
fn handle(state: &OverlayWindow, hwnd: HWND, message: u32, wparam: WPARAM) -> Option<LRESULT> {
    let app = &state.app;
    match message {
        WM_SETCURSOR => {
            // SAFETY: plain call; a null cursor hides it over this window only (OL9).
            unsafe { SetCursor(None) };
            Some(LRESULT(1))
        }
        WM_PAINT => {
            paint(state, hwnd);
            Some(LRESULT(0))
        }
        WM_SIZE => {
            refit_text(state, hwnd);
            None
        }
        WM_CHAR => {
            // `wParam` holds one UTF-16 code unit; a surrogate pair arrives as two messages.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "WM_CHAR carries one UTF-16 unit"
            )]
            app.push_unit(wparam.0 as u16);
            Some(LRESULT(0))
        }
        WM_SYSCOMMAND if wparam.0 & 0xFFF0 == SC_CLOSE as usize => {
            app.close_requested();
            Some(LRESULT(0))
        }
        WM_CLOSE => {
            app.close_requested();
            Some(LRESULT(0))
        }
        WM_DISPLAYCHANGE => {
            app.display_changed();
            Some(LRESULT(0))
        }
        WM_DPICHANGED => {
            // Keep covering the monitor; ignore the suggested rectangle. The pixel size may stay
            // the same, so the text is refitted to the new DPI here as well as on `WM_SIZE`.
            if let Some(rect) = app.overlays().rect_of(hwnd) {
                let _ = place(hwnd, rect);
            }
            refit_text(state, hwnd);
            Some(LRESULT(0))
        }
        WM_QUERYENDSESSION => Some(LRESULT(1)),
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                app.unlock();
            }
            Some(LRESULT(0))
        }
        WM_MOUSEACTIVATE => Some(LRESULT(isize::try_from(MA_ACTIVATE).unwrap_or(1))),
        _ => None,
    }
}

/// Paints an overlay. The class brush has already erased the background; with a message, Direct2D
/// draws the background and the text over it.
fn paint(state: &OverlayWindow, hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: starts a paint on this window; matched by `EndPaint` below.
    unsafe { BeginPaint(hwnd, &raw mut paint) };
    let lost_device = state.app.text_renderer().is_some_and(|renderer| {
        let mut text = state.text.take();
        let result = renderer.paint(hwnd, &mut text);
        state.text.set(text);
        result.is_err_and(|error| error.code() == D2DERR_RECREATE_TARGET)
    });
    // SAFETY: ends the paint started above.
    let _ = unsafe { EndPaint(hwnd, &raw const paint) };
    if lost_device {
        // SAFETY: plain call on this window; the next `WM_PAINT` makes a new target.
        let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
    }
}

/// Fits the message to the window's size and DPI, and repaints it.
fn refit_text(state: &OverlayWindow, hwnd: HWND) {
    let Some(renderer) = state.app.text_renderer() else {
        return;
    };
    let mut text = state.text.take();
    if renderer.resize(hwnd, &mut text).is_err() {
        // Make everything again on the next paint.
        text = None;
    }
    state.text.set(text);
    // SAFETY: plain call on this window.
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}
