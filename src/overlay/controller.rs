//! The message-only controller window `OverlayLock.Controller`: it owns the timers, receives the
//! unlock request and drives the teardown.

use std::rc::Rc;
use std::time::Duration;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetForegroundWindow, HWND_MESSAGE, KillTimer, RegisterClassExW, SetTimer,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_TIMER, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};
use windows::core::{PCWSTR, w};

use super::{WM_APP_DISPLAY_CHANGED, WM_APP_UNLOCK};
use crate::app::App;
use crate::secure_desktop::{self, Action, Signal, WM_APP_DESKTOP_SWITCH};

/// The controller window class.
const CLASS: PCWSTR = w!("OverlayLock.Controller");

/// The timer that retries the foreground sequence (OL10).
const TIMER_FOCUS: usize = 1;

/// The one-shot `--timeout` timer (OL14).
const TIMER_TIMEOUT: usize = 2;

/// The timer that raises the overlays again after a display change.
const TIMER_RAISE: usize = 3;

/// The one-shot timer that waits for a late session message after a secure desktop visit.
const TIMER_SAS_SETTLE: usize = 4;

/// The focus retry and raise interval in milliseconds, as OverlayLockLite used for focus.
const FOCUS_INTERVAL_MS: u32 = 250;

/// The state of the controller window.
struct Controller {
    app: Rc<App>,
}

/// Registers the controller class.
pub fn register(instance: HINSTANCE) -> windows::core::Result<()> {
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).expect("small struct"),
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: CLASS,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and its strings are static.
    if unsafe { RegisterClassExW(&raw const class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

/// Creates the message-only controller window.
pub fn create(app: &Rc<App>) -> windows::core::Result<HWND> {
    let state = Box::into_raw(Box::new(Controller {
        app: Rc::clone(app),
    }));
    // SAFETY: the class is registered; `state` is a `Box<Controller>` pointer that the window
    // procedure takes over at `WM_NCCREATE` (see `overlay::dispatch`). On failure the state is
    // not freed, for the reason given in `window::create`.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS,
            w!("OverlayLock"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(app.instance()),
            Some(state.cast_const().cast()),
        )
    }
}

/// Starts the repeating focus timer.
pub fn start_focus_timer(controller: HWND) -> windows::core::Result<()> {
    set_timer(controller, TIMER_FOCUS, FOCUS_INTERVAL_MS)
}

/// Starts the one-shot timeout timer.
pub fn start_timeout(controller: HWND, timeout: Duration) -> windows::core::Result<()> {
    // The command line limits the timeout to 7 days, 604,800,000 ms, which fits in a u32.
    let milliseconds = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    set_timer(controller, TIMER_TIMEOUT, milliseconds)
}

fn set_timer(controller: HWND, id: usize, milliseconds: u32) -> windows::core::Result<()> {
    // SAFETY: the controller is a window of this thread; no timer callback, so `WM_TIMER` is used.
    if unsafe { SetTimer(Some(controller), id, milliseconds, None) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

fn kill_timer(controller: HWND, id: usize) {
    // SAFETY: the controller is a window of this thread; killing a timer that is gone is harmless.
    let _ = unsafe { KillTimer(Some(controller), id) };
}

extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: this is the window procedure of the controller class, whose only window is created
    // by `create` with a `Box<Controller>` as `lpParam`.
    unsafe {
        super::dispatch(hwnd, message, wparam, lparam, |state: &Controller| {
            handle(state, hwnd, message, wparam, lparam)
        })
    }
}

/// Handles one message, or returns `None` for the default handling.
fn handle(
    state: &Controller,
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> Option<LRESULT> {
    match message {
        WM_TIMER if wparam.0 == TIMER_FOCUS => {
            if !state.app.focus_tick() {
                kill_timer(hwnd, TIMER_FOCUS);
            }
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_TIMEOUT => {
            kill_timer(hwnd, TIMER_TIMEOUT);
            state.app.unlock();
            Some(LRESULT(0))
        }
        WM_APP_UNLOCK => {
            state.app.teardown();
            Some(LRESULT(0))
        }
        WM_APP_DISPLAY_CHANGED => {
            if state.app.after_display_change() {
                let _ = set_timer(hwnd, TIMER_RAISE, FOCUS_INTERVAL_MS);
            }
            Some(LRESULT(0))
        }
        WM_APP_DESKTOP_SWITCH => {
            let signal = if wparam.0 != 0 {
                Signal::DesktopReturned
            } else {
                Signal::DesktopLeft {
                    elevation_prompt: lparam.0 != 0,
                }
            };
            desktop_signal(state, hwnd, signal);
            Some(LRESULT(0))
        }
        WM_WTSSESSION_CHANGE => {
            match u32::try_from(wparam.0) {
                Ok(WTS_SESSION_LOCK) => desktop_signal(state, hwnd, Signal::SessionLock),
                Ok(WTS_SESSION_UNLOCK) => desktop_signal(state, hwnd, Signal::SessionUnlock),
                _ => {}
            }
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_SAS_SETTLE => {
            kill_timer(hwnd, TIMER_SAS_SETTLE);
            desktop_signal(state, hwnd, Signal::Settled);
            Some(LRESULT(0))
        }
        WM_TIMER if wparam.0 == TIMER_RAISE => {
            if !state.app.raise_tick() {
                kill_timer(hwnd, TIMER_RAISE);
            }
            Some(LRESULT(0))
        }
        _ => None,
    }
}

/// Passes a desktop or session signal to the app and carries out what it asks for.
fn desktop_signal(state: &Controller, hwnd: HWND, signal: Signal) {
    match state.app.desktop_signal(signal) {
        Action::None => {}
        Action::StartSettleTimer => {
            let _ = set_timer(hwnd, TIMER_SAS_SETTLE, secure_desktop::SETTLE_MS);
        }
        Action::Yield => {
            kill_timer(hwnd, TIMER_RAISE);
            // SAFETY: plain query with no preconditions.
            let foreground = unsafe { GetForegroundWindow() };
            state
                .app
                .yield_lock((!foreground.is_invalid()).then_some(foreground));
        }
    }
}
