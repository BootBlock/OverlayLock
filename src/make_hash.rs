//! `--make-hash`: reads a password and prints the hash `--password-hash` takes, so no other
//! program is needed to make one.
//!
//! The password comes from the first source that exists:
//!
//! - A console on standard input: it is asked for twice, with echo off, through `CONIN$` and
//!   `CONOUT$`, so the hash alone goes to standard output and `> hash.txt` captures only the hash.
//! - Redirected standard input (a pipe or a file): it is read as UTF-8 text, and one line break at
//!   the end is dropped.
//! - Neither, as from a shortcut or the Run dialog: the prompts and the hash go to the console the
//!   process has, or to one it opens. A console it opens shows the hash until Enter is pressed, so
//!   it can be copied before the console closes with the process.

use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_UNKNOWN,
    GetFileType, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AllocConsole, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, GetStdHandle, ReadConsoleW, STD_INPUT_HANDLE,
    SetConsoleMode,
};
use windows::core::{PCWSTR, w};

use crate::password::hash::{MAX_LENGTH, PasswordHash};
use crate::report::{ExitStatus, Severity, is_console, report, write_console};

/// Why no hash was made.
#[derive(Debug, PartialEq, Eq)]
enum Failure {
    /// The person pressed Ctrl+C at a prompt.
    Cancelled,
    /// Anything else, with the text to show.
    Message(String),
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

/// Where the password comes from.
enum Source {
    /// A console on standard input.
    Console,
    /// A pipe, a file or another device on standard input.
    Redirected,
    /// No standard input at all.
    Nothing,
}

/// Reads a password, prints its hash and returns the exit status: 0 for a hash, 1 otherwise.
pub fn run() -> ExitStatus {
    let result = match standard_input() {
        Source::Console => Console::open()
            .and_then(|console| ask_twice(&console))
            .and_then(|password| hash_text(&password)),
        Source::Redirected => read_redirected()
            .map_err(Failure::from)
            .and_then(|password| hash_text(&password)),
        Source::Nothing => return run_without_standard_input(),
    };
    match result {
        Ok(text) => {
            report(Severity::Info, &text);
            ExitStatus::Unlocked
        }
        Err(Failure::Cancelled) => ExitStatus::Failure,
        Err(Failure::Message(message)) => {
            report(Severity::Error, &format!("overlay-lock: {message}"));
            ExitStatus::Failure
        }
    }
}

/// Asks on the console the process has, or opens one, and shows the hash or the failure there.
/// A console it opened closes with the process, so it waits for Enter first, and the text can be
/// read and copied.
fn run_without_standard_input() -> ExitStatus {
    // A console can be attached while standard input is closed; it is used as it is.
    let (console, opened) = if let Ok(console) = Console::open() {
        (console, false)
    } else {
        // SAFETY: plain call; this process has no console here, so a new one is made for it.
        if let Err(error) = unsafe { AllocConsole() } {
            report(
                Severity::Error,
                &format!(
                    "overlay-lock: could not open a console: {}",
                    error.message()
                ),
            );
            return ExitStatus::Failure;
        }
        match Console::open() {
            Ok(console) => (console, true),
            Err(failure) => {
                if let Failure::Message(message) = failure {
                    report(Severity::Error, &format!("overlay-lock: {message}"));
                }
                return ExitStatus::Failure;
            }
        }
    };
    let (status, text) = match ask_twice(&console).and_then(|password| hash_text(&password)) {
        Ok(hash) => (ExitStatus::Unlocked, format!("\n{hash}\n")),
        Err(Failure::Cancelled) => return ExitStatus::Failure,
        Err(Failure::Message(message)) => {
            (ExitStatus::Failure, format!("\noverlay-lock: {message}\n"))
        }
    };
    console.write(&text);
    if opened {
        console.write(match status {
            ExitStatus::Unlocked => "\nCopy the hash above, then press Enter to close.\n",
            _ => "\nPress Enter to close.\n",
        });
        let _ = console.read_line();
    }
    status
}

/// Tells where standard input comes from.
fn standard_input() -> Source {
    // SAFETY: `GetStdHandle` has no preconditions.
    let Ok(handle) = (unsafe { GetStdHandle(STD_INPUT_HANDLE) }) else {
        return Source::Nothing;
    };
    if handle.is_invalid() {
        return Source::Nothing;
    }
    if is_console(handle) {
        return Source::Console;
    }
    // SAFETY: `handle` is the process's own standard input handle.
    if unsafe { GetFileType(handle) } == FILE_TYPE_UNKNOWN {
        return Source::Nothing;
    }
    Source::Redirected
}

/// Asks for the password twice and returns it if both entries are the same.
fn ask_twice(console: &Console) -> Result<Vec<u16>, Failure> {
    console.write("Type the password to hash (it is not shown): ");
    let first = console.read_line();
    console.write("\n");
    let first = first?;
    check_length(&first)?;
    console.write("Type it again: ");
    let second = console.read_line();
    console.write("\n");
    if second? != first {
        return Err(Failure::Message(
            "the two passwords are not the same, so no hash was made".to_owned(),
        ));
    }
    Ok(first)
}

/// Reads all of redirected standard input as the password.
fn read_redirected() -> Result<Vec<u16>, String> {
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::stdin(), &mut bytes)
        .map_err(|error| format!("could not read standard input: {error}"))?;
    let password = password_from_bytes(&bytes)?;
    check_length(&password)?;
    Ok(password)
}

/// Decodes redirected input: UTF-8 text, without a byte order mark at the start or one line break
/// (`\n` or `\r\n`) at the end, which `echo` and a pipe add.
fn password_from_bytes(bytes: &[u8]) -> Result<Vec<u16>, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| "standard input is not UTF-8 text".to_owned())?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    Ok(text.encode_utf16().collect())
}

/// Checks that a password can be hashed: not empty, and at most [`MAX_LENGTH`] units.
fn check_length(password: &[u16]) -> Result<(), String> {
    if password.is_empty() {
        return Err("the password must not be empty".to_owned());
    }
    if password.len() > MAX_LENGTH {
        return Err(format!(
            "the password is {} UTF-16 code units long; a hashed password has at most {MAX_LENGTH}",
            password.len()
        ));
    }
    Ok(())
}

/// Hashes a password and gives the text `--password-hash` takes.
fn hash_text(password: &[u16]) -> Result<String, Failure> {
    PasswordHash::make(password)
        .map(|hash| hash.to_string())
        .map_err(|error| {
            Failure::Message(format!("could not hash the password: {}", error.message()))
        })
}

/// What one typed unit does to the line being read.
#[derive(Debug, PartialEq, Eq)]
enum Edit {
    /// Keep reading.
    More,
    /// Enter: the line is complete.
    Done,
    /// Ctrl+C: give up.
    Cancel,
}

/// Applies one unit read from the console with line editing off: Enter ends the line, Backspace
/// removes the last character (both halves of a surrogate pair), Ctrl+C cancels, a line feed is
/// skipped (it follows a carriage return in pasted text), and anything else is part of the line.
fn edit(line: &mut Vec<u16>, unit: u16) -> Edit {
    const BACKSPACE: u16 = 0x08;
    const LINE_FEED: u16 = 0x0A;
    const CARRIAGE_RETURN: u16 = 0x0D;
    const CTRL_C: u16 = 0x03;
    match unit {
        CARRIAGE_RETURN => Edit::Done,
        CTRL_C => Edit::Cancel,
        LINE_FEED => Edit::More,
        BACKSPACE => {
            let low = line.pop();
            if low.is_some_and(|unit| (0xDC00..=0xDFFF).contains(&unit))
                && line
                    .last()
                    .is_some_and(|unit| (0xD800..=0xDBFF).contains(unit))
            {
                line.pop();
            }
            Edit::More
        }
        _ => {
            line.push(unit);
            Edit::More
        }
    }
}

/// The console the prompts use, opened as `CONIN$` and `CONOUT$` so it works whatever standard
/// output and standard error point at.
struct Console {
    input: HANDLE,
    output: HANDLE,
}

impl Console {
    fn open() -> Result<Self, Failure> {
        let input = open_device(w!("CONIN$"))?;
        let output = match open_device(w!("CONOUT$")) {
            Ok(output) => output,
            Err(failure) => {
                // SAFETY: `input` was opened above and is closed once, here.
                let _ = unsafe { CloseHandle(input) };
                return Err(failure);
            }
        };
        Ok(Self { input, output })
    }

    fn write(&self, text: &str) {
        let _ = write_console(self.output, text);
    }

    /// Reads one line with echo and line editing off. The console's mode is restored before it
    /// returns.
    fn read_line(&self) -> Result<Vec<u16>, Failure> {
        let mut mode = CONSOLE_MODE::default();
        // SAFETY: `mode` is writable and `input` is an open console input handle.
        unsafe { GetConsoleMode(self.input, &raw mut mode) }
            .map_err(|error| Failure::Message(console_error(&error)))?;
        let raw = mode
            & !(ENABLE_ECHO_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_PROCESSED_INPUT
                | ENABLE_VIRTUAL_TERMINAL_INPUT);
        // SAFETY: as above; the mode is put back below on every path.
        unsafe { SetConsoleMode(self.input, raw) }
            .map_err(|error| Failure::Message(console_error(&error)))?;
        let line = self.read_raw_line();
        // SAFETY: restores the mode read above.
        let _ = unsafe { SetConsoleMode(self.input, mode) };
        line
    }

    /// Reads one unit at a time, so the units after Enter stay in the console's buffer for the
    /// next line: both entries pasted at once still answer both prompts.
    fn read_raw_line(&self) -> Result<Vec<u16>, Failure> {
        let mut line = Vec::new();
        loop {
            let mut unit = 0u16;
            let mut read = 0u32;
            // SAFETY: `unit` is writable for the one unit asked for, and `read` receives the count.
            unsafe { ReadConsoleW(self.input, (&raw mut unit).cast(), 1, &raw mut read, None) }
                .map_err(|error| Failure::Message(console_error(&error)))?;
            if read == 0 {
                return Err(Failure::Message("the console input ended".to_owned()));
            }
            match edit(&mut line, unit) {
                Edit::More => {}
                Edit::Done => return Ok(line),
                Edit::Cancel => return Err(Failure::Cancelled),
            }
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        // SAFETY: both handles were opened by `open` and are closed once, here.
        unsafe {
            let _ = CloseHandle(self.input);
            let _ = CloseHandle(self.output);
        }
    }
}

/// Opens a console device by its name.
fn open_device(name: PCWSTR) -> Result<HANDLE, Failure> {
    // SAFETY: `name` is a static wide string; no security attributes or template are passed.
    unsafe {
        CreateFileW(
            name,
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|error| Failure::Message(console_error(&error)))
}

fn console_error(error: &windows::core::Error) -> String {
    format!("could not use the console: {}", error.message())
}

#[cfg(test)]
mod tests {
    use super::{Edit, check_length, edit, password_from_bytes};
    use crate::password::hash::MAX_LENGTH;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    /// Feeds `typed` to [`edit`] and returns the line and the last edit.
    fn type_line(typed: &str) -> (Vec<u16>, Edit) {
        let mut line = Vec::new();
        let mut last = Edit::More;
        for unit in typed.encode_utf16() {
            last = edit(&mut line, unit);
            if last != Edit::More {
                break;
            }
        }
        (line, last)
    }

    #[test]
    fn enter_ends_the_line() {
        assert_eq!(type_line("go\rmore"), (utf16("go"), Edit::Done));
    }

    #[test]
    fn backspace_removes_the_last_character() {
        assert_eq!(type_line("gx\u{8}o\r"), (utf16("go"), Edit::Done));
        assert_eq!(type_line("\u{8}go\r"), (utf16("go"), Edit::Done));
    }

    #[test]
    fn backspace_removes_a_whole_surrogate_pair() {
        assert_eq!(type_line("g\u{1F512}\u{8}o\r"), (utf16("go"), Edit::Done));
    }

    #[test]
    fn ctrl_c_cancels() {
        assert_eq!(type_line("go\u{3}").1, Edit::Cancel);
    }

    #[test]
    fn a_line_feed_is_skipped() {
        assert_eq!(type_line("\ngo\r"), (utf16("go"), Edit::Done));
    }

    #[test]
    fn redirected_input_drops_one_line_break_and_a_byte_order_mark() {
        assert_eq!(password_from_bytes(b"go\r\n"), Ok(utf16("go")));
        assert_eq!(password_from_bytes(b"go\n"), Ok(utf16("go")));
        assert_eq!(password_from_bytes(b"go\n\n"), Ok(utf16("go\n")));
        assert_eq!(password_from_bytes(b"go "), Ok(utf16("go ")));
        assert_eq!(
            password_from_bytes("\u{FEFF}Purr 42\n".as_bytes()),
            Ok(utf16("Purr 42"))
        );
        assert_eq!(
            password_from_bytes("caf\u{E9}".as_bytes()),
            Ok(utf16("caf\u{E9}"))
        );
    }

    #[test]
    fn redirected_input_must_be_utf8() {
        assert!(password_from_bytes(&[0x67, 0xFF]).is_err());
    }

    #[test]
    fn length_limits() {
        assert!(check_length(&[]).is_err());
        assert!(check_length(&[1]).is_ok());
        assert!(check_length(&vec![1; MAX_LENGTH]).is_ok());
        assert!(check_length(&vec![1; MAX_LENGTH + 1]).is_err());
    }
}
