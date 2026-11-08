//! Shows a message to the person who started the process, and defines the exit codes.
//!
//! The binary uses the GUI subsystem, so it has no console of its own. A message goes to the first
//! sink that exists: a standard handle the parent redirected or shared, then the parent's console,
//! then a message box.

use std::process::ExitCode;

use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_WRITE, FILE_TYPE_CHAR, FILE_TYPE_UNKNOWN,
    GetFileType, OPEN_EXISTING, WriteFile,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, CONSOLE_MODE, FreeConsole, GetConsoleMode, GetStdHandle,
    STD_ERROR_HANDLE, STD_HANDLE, STD_OUTPUT_HANDLE, WriteConsoleW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};
use windows::core::{HSTRING, w};

/// The process exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitStatus {
    /// The overlay was unlocked, or help or the version was shown.
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
    /// Help or version text: standard output, information icon.
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

    if write_std_handle(stream, &line) || write_parent_console(&line) {
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

/// Attaches to the parent's console, if it has one, and writes there.
fn write_parent_console(line: &str) -> bool {
    // SAFETY: `AttachConsole` has no preconditions; it fails when there is no parent console.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
        return false;
    }
    // SAFETY: the file name is a valid wide string and the arguments are valid for a console.
    let written = unsafe {
        CreateFileW(
            w!("CONOUT$"),
            GENERIC_WRITE.0,
            FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .is_ok_and(|console| {
        // The shell printed its prompt before this text arrives, so start on a new line.
        let ok = write_console(console, &format!("\n{line}"));
        // SAFETY: `console` came from `CreateFileW` above and is closed once.
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(console) };
        ok
    });
    // SAFETY: detaches the console attached above.
    let _ = unsafe { FreeConsole() };
    written
}

fn is_console(handle: HANDLE) -> bool {
    let mut mode = CONSOLE_MODE::default();
    // SAFETY: `mode` is writable and `handle` is a valid handle.
    unsafe { GetConsoleMode(handle, &raw mut mode) }.is_ok()
}

fn write_console(handle: HANDLE, line: &str) -> bool {
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
