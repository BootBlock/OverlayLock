//! Shows a message to the person who started the process, and defines the exit codes.
//!
//! The binary uses the console subsystem, so a shell waits for it and its text and exit code reach
//! that shell in order. Its manifest sets `consoleAllocationPolicy` to `detached` (Windows 11 24H2
//! and later), so Windows gives it no console when nothing else did: started from a shortcut or a
//! Stream Deck button, no console window appears. Older Windows ignores that setting and makes a
//! console for it; [`release_console_made_for_us`] closes that at once.
//!
//! A message goes to the first sink that exists: the standard handle (a console the process shares
//! with its shell, or a pipe or file a parent redirected), then a message box.

use std::process::ExitCode;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FILE_TYPE_CHAR, FILE_TYPE_UNKNOWN, GetFileType, WriteFile,
};
use windows::Win32::System::Console::{
    CONSOLE_MODE, FreeConsole, GetConsoleMode, GetConsoleProcessList, GetConsoleWindow,
    GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    WriteConsoleW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};
use windows::core::{HSTRING, w};

/// The process exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    /// The overlay was unlocked, or help, the version or a password hash was shown.
    Unlocked = 0,
    /// A runtime failure stopped the lock.
    Failure = 1,
    /// The command line is not valid.
    Usage = 2,
    /// Another instance is already locked.
    AlreadyLocked = 3,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        ExitCode::from(status as u8)
    }
}

/// What kind of message is reported, which sets its stream and its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Help, version or password hash text: standard output, information icon.
    Info,
    /// A usage error or a failure: standard error, error icon.
    Error,
}

/// Shows `text` on the first sink that exists (see the module documentation).
pub fn report(severity: Severity, text: &str) {
    let stream = match severity {
        Severity::Info => STD_OUTPUT_HANDLE,
        Severity::Error => STD_ERROR_HANDLE,
    };
    let mut line = String::with_capacity(text.len() + 2);
    line.push_str(text.trim_end_matches('\n'));
    line.push('\n');

    if write_std_handle(stream, &line) {
        return;
    }
    let style = match severity {
        Severity::Info => MB_ICONINFORMATION,
        Severity::Error => MB_ICONERROR,
    };
    // SAFETY: both strings are valid, NUL-terminated wide strings for the call.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(line.trim_end()),
            w!("OverlayLock"),
            MB_OK | style | MB_SETFOREGROUND,
        );
    }
}

/// Writes to an inherited standard handle: a pipe or file as UTF-8, a console as UTF-16.
fn write_std_handle(stream: STD_HANDLE, line: &str) -> bool {
    // SAFETY: `GetStdHandle` has no preconditions.
    let Ok(handle) = (unsafe { GetStdHandle(stream) }) else {
        return false;
    };
    if handle.is_invalid() {
        return false;
    }
    // SAFETY: `handle` is the process's own standard handle, valid for the process life.
    match unsafe { GetFileType(handle) } {
        FILE_TYPE_UNKNOWN => false,
        FILE_TYPE_CHAR if is_console(handle) => write_console(handle, line),
        _ => write_utf8(handle, line),
    }
}

/// Closes the console if Windows made it for this process alone, which happens only where the
/// `detached` console allocation policy is not supported (before Windows 11 24H2) and nothing
/// started the process from a console. A console shared with a shell is kept. Call it first in
/// `main`, so the window shows for as short a time as possible.
pub fn release_console_made_for_us() {
    // SAFETY: plain query with no preconditions.
    if unsafe { GetConsoleWindow() }.is_invalid() {
        return;
    }
    let mut processes = [0u32; 2];
    // SAFETY: the buffer is writable and its length is passed with it.
    let attached = unsafe { GetConsoleProcessList(&mut processes) };
    if attached != 1 {
        return;
    }
    // SAFETY: detaches this process from the console it alone uses, which closes it.
    let _ = unsafe { FreeConsole() };
    for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: clears a standard handle that pointed at the console just closed, so `report`
        // falls through to the message box.
        let _ = unsafe { SetStdHandle(stream, HANDLE::default()) };
    }
}

/// Returns `true` if `handle` is a console; otherwise, `false`.
pub fn is_console(handle: HANDLE) -> bool {
    let mut mode = CONSOLE_MODE::default();
    // SAFETY: `mode` is writable and `handle` is a valid handle.
    unsafe { GetConsoleMode(handle, &raw mut mode) }.is_ok()
}

/// Writes `line` to a console output handle as UTF-16. Returns `true` if it was written;
/// otherwise, `false`.
pub fn write_console(handle: HANDLE, line: &str) -> bool {
    let wide: Vec<u16> = line.encode_utf16().collect();
    // SAFETY: `handle` is a console output handle and `wide` outlives the call.
    unsafe { WriteConsoleW(handle, &wide, None, None) }.is_ok()
}

fn write_utf8(handle: HANDLE, line: &str) -> bool {
    let mut rest = line.as_bytes();
    while !rest.is_empty() {
        let mut written = 0u32;
        // SAFETY: `handle` is a valid, writable handle and `rest` outlives the call.
        if unsafe { WriteFile(handle, Some(rest), Some(&raw mut written), None) }.is_err()
            || written == 0
        {
            return false;
        }
        rest = &rest[written as usize..];
    }
    true
}
