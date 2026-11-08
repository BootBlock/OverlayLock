//! Starts the built executable as a lock and finds and drives its overlay windows.
//!
//! Every lock carries `--timeout`, and a [`Lock`] kills its process when dropped, so a failing
//! test never leaves the desktop covered. Output is always redirected, so a message box never
//! blocks a test.

#![allow(
    dead_code,
    reason = "each test file uses a different part of this module"
)]

pub mod input;

use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CHAR,
};
use windows::core::BOOL;

/// Only one lock runs at a time: the app allows one instance per session.
static SERIAL: Mutex<()> = Mutex::new(());

/// Takes the right to run a lock, even after another test panicked while holding it.
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running lock process, killed on drop if it is still running.
pub struct Lock {
    child: Child,
}

impl Lock {
    /// Starts the executable with `args`, its output redirected.
    pub fn start(args: &[&str]) -> Self {
        assert!(
            args.contains(&"--timeout"),
            "every test lock needs --timeout as a safety net"
        );
        let child = Command::new(env!("CARGO_BIN_EXE_overlay-lock"))
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the executable starts");
        Self { child }
    }

    /// Gets the process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Waits up to 5 s for a visible overlay window of this process.
    pub fn overlay(&self) -> HWND {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(hwnd) = overlays_of(self.pid()).first() {
                return *hwnd;
            }
            assert!(
                Instant::now() < deadline,
                "no overlay window appeared within 5 s"
            );
            sleep(Duration::from_millis(10));
        }
    }

    /// Waits up to `limit` for the process to exit, and returns its exit code.
    pub fn exit_code_within(&mut self, limit: Duration) -> Option<i32> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().expect("the process can be polled") {
                return status.code();
            }
            if Instant::now() >= deadline {
                return None;
            }
            sleep(Duration::from_millis(10));
        }
    }

    /// Returns `true` if the process is still running; otherwise, `false`.
    pub fn is_running(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("the process can be polled")
            .is_none()
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Posts each UTF-16 unit of `text` to `hwnd` as `WM_CHAR`.
pub fn post_text(hwnd: HWND, text: &str) {
    for unit in text.encode_utf16() {
        post(hwnd, WM_CHAR, usize::from(unit));
    }
}

/// Posts one message with an integer `wParam`.
pub fn post(hwnd: HWND, message: u32, wparam: usize) {
    // SAFETY: posting plain integers to a window of another process is always memory-safe.
    unsafe { PostMessageW(Some(hwnd), message, WPARAM(wparam), LPARAM(0)) }
        .expect("the message is posted");
}

/// Gets the visible top-level windows of class `OverlayLock.Overlay` owned by `pid`.
pub fn overlays_of(pid: u32) -> Vec<HWND> {
    let mut search = (pid, Vec::new());
    // SAFETY: the callback runs only during this call and receives `search` through `LPARAM`.
    let _ = unsafe {
        EnumWindows(
            Some(collect),
            LPARAM(std::ptr::from_mut(&mut search) as isize),
        )
    };
    search.1
}

extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `overlays_of` passes a valid, exclusive pointer that outlives the enumeration.
    let (pid, found) = unsafe { &mut *(data.0 as *mut (u32, Vec<HWND>)) };
    let mut owner = 0u32;
    let mut class = [0u16; 64];
    // SAFETY: `hwnd` comes from the enumeration; the buffers are valid and writable.
    let (length, visible) = unsafe {
        GetWindowThreadProcessId(hwnd, Some(&raw mut owner));
        (GetClassNameW(hwnd, &mut class), IsWindowVisible(hwnd))
    };
    let length = usize::try_from(length).unwrap_or(0);
    if owner == *pid
        && visible.as_bool()
        && String::from_utf16_lossy(&class[..length]) == "OverlayLock.Overlay"
    {
        found.push(hwnd);
    }
    BOOL::from(true)
}
