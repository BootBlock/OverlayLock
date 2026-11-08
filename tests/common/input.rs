//! Synthesises keyboard and mouse input with `SendInput`, and owns a plain edit window that can
//! take the focus, for the hook tests.

use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CreateWindowExW, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetForegroundWindow, GetMessageW, GetWindowTextW, GetWindowThreadProcessId, MSG,
    PostThreadMessageW, SetForegroundWindow, TranslateMessage, WINDOW_EX_STYLE, WM_QUIT,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::core::w;

/// Presses and releases each virtual key in turn, as a keyboard would.
pub fn type_keys(keys: &[VIRTUAL_KEY]) {
    for &key in keys {
        send_key(key, true);
        send_key(key, false);
    }
}

/// Sends one key down or up, with its scan code, never as `KEYEVENTF_UNICODE`.
pub fn send_key(key: VIRTUAL_KEY, down: bool) {
    // SAFETY: plain query with no preconditions.
    let scan = unsafe { MapVirtualKeyW(u32::from(key.0), MAPVK_VK_TO_VSC) };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: u16::try_from(scan).unwrap_or(0),
                dwFlags: if down {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send(&input);
}

/// Types text as `KEYEVENTF_UNICODE` packets, as a Stream Deck text action does.
pub fn type_unicode(text: &str) {
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            send(&INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
}

/// Moves the mouse by a relative amount.
pub fn move_mouse(dx: i32, dy: i32) {
    send_mouse(dx, dy, 0, MOUSEEVENTF_MOVE);
}

/// Clicks the left and right buttons and turns the wheel one notch, where the cursor is.
pub fn click_and_scroll() {
    for flags in [
        MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_RIGHTDOWN,
        MOUSEEVENTF_RIGHTUP,
    ] {
        send_mouse(0, 0, 0, flags);
    }
    // One wheel notch towards the user; the data is the signed WHEEL_DELTA as two's complement.
    send_mouse(0, 0, (-120i32).cast_unsigned(), MOUSEEVENTF_WHEEL);
}

fn send_mouse(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) {
    send(&INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    });
}

/// Gets the cursor position.
pub fn cursor() -> POINT {
    let mut point = POINT::default();
    // SAFETY: `point` is writable.
    unsafe { GetCursorPos(&raw mut point) }.expect("the cursor position is readable");
    point
}

fn send(input: &INPUT) {
    let size = i32::try_from(size_of::<INPUT>()).expect("small struct");
    // SAFETY: one fully initialised `INPUT` with its size.
    let sent = unsafe { SendInput(std::slice::from_ref(input), size) };
    assert_eq!(sent, 1, "SendInput was blocked");
}

/// A plain edit window on its own thread, which the tests give the focus to.
pub struct EditWindow {
    hwnd: HWND,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

/// A window handle sent from the edit window's thread.
struct Handle(HWND);

// SAFETY: a window handle is an opaque value that any thread may use with these calls.
unsafe impl Send for Handle {}

impl EditWindow {
    /// Creates the window on a new thread that pumps its messages.
    pub fn open() -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            // SAFETY: a plain top-level edit control owned by this thread.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("EDIT"),
                    w!(""),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    100,
                    100,
                    400,
                    200,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .expect("the edit window is created");
            // SAFETY: plain query with no preconditions.
            let thread_id = unsafe { GetCurrentThreadId() };
            sender
                .send((Handle(hwnd), thread_id))
                .expect("the test is waiting");
            let mut message = MSG::default();
            // SAFETY: a plain message loop for the window above, then its destruction.
            unsafe {
                while GetMessageW(&raw mut message, None, 0, 0).0 > 0 {
                    let _ = TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                }
                let _ = DestroyWindow(hwnd);
            }
        });
        let (Handle(hwnd), thread_id) = receiver.recv().expect("the window thread reports");
        Self {
            hwnd,
            thread_id,
            thread: Some(thread),
        }
    }

    /// Makes this window the foreground window, retrying for up to 2 s. Returns `true` if it is
    /// the foreground window afterwards; otherwise, `false`.
    pub fn take_focus(&self) -> bool {
        (0..20).any(|_| {
            if self.try_take_focus() {
                return true;
            }
            thread::sleep(std::time::Duration::from_millis(100));
            false
        })
    }

    /// Makes this window the foreground window once, borrowing the foreground thread's input
    /// state as the app itself does.
    fn try_take_focus(&self) -> bool {
        // SAFETY: the two threads are attached only for these calls and detached at once.
        unsafe {
            // Attach the calling thread, which makes the `SetForegroundWindow` call below.
            let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let own = GetCurrentThreadId();
            let attach = foreground != 0 && foreground != own;
            if attach {
                let _ = AttachThreadInput(foreground, own, true);
            }
            let _ = BringWindowToTop(self.hwnd);
            let _ = SetForegroundWindow(self.hwnd);
            if attach {
                let _ = AttachThreadInput(foreground, own, false);
            }
            GetForegroundWindow() == self.hwnd
        }
    }

    /// Gets the text typed into the window.
    pub fn text(&self) -> String {
        let mut buffer = [0u16; 256];
        // SAFETY: the buffer is writable; the window's thread pumps `WM_GETTEXT`.
        let length = unsafe { GetWindowTextW(self.hwnd, &mut buffer) };
        String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or(0)])
    }
}

impl Drop for EditWindow {
    fn drop(&mut self) {
        // SAFETY: posts `WM_QUIT` to the window thread, whose queue exists.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
