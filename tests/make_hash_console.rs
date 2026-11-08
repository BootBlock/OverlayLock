//! Drives `--make-hash` through a real console.
//!
//! A pseudoconsole (`ConPTY`) carries the console cases, with no window and no keyboard: the
//! prompts, echo off, Backspace, both entries typed at once, and a mismatch. The program runs
//! under `cmd.exe`, so it is not its console's only process, which it would close as a console
//! Windows made for it alone.
//!
//! The case with no standard input, where the program opens a console of its own, needs this test
//! to leave its own console and shows a console window for a moment, so it is `#[ignore]` and run
//! by hand: `cargo test --test make_hash_console -- --ignored`.

mod common;

use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use common::{Lock, post_text, serial};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AttachConsole, CONSOLE_SCREEN_BUFFER_INFO, COORD, ClosePseudoConsole, CreatePseudoConsole,
    FreeConsole, GetConsoleScreenBufferInfo, HPCON, INPUT_RECORD, INPUT_RECORD_0, KEY_EVENT,
    KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, ReadConsoleOutputCharacterW, WriteConsoleInputW,
};
use windows::Win32::System::Threading::{
    CreateProcessW, DETACHED_PROCESS, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    GetExitCodeProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::{PCWSTR, PWSTR, w};

/// A started process, its handles closed on drop.
struct Child(PROCESS_INFORMATION);

impl Child {
    /// Starts `command_line` with `flags` and `startup`, which must point at a `STARTUPINFOW` or
    /// a `STARTUPINFOEXW` that matches `flags`.
    fn start(command_line: &str, flags: PROCESS_CREATION_FLAGS, startup: &STARTUPINFOW) -> Self {
        let mut command: Vec<u16> = command_line.encode_utf16().chain([0]).collect();
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: `command` is a writable, NUL-terminated buffer; `startup` is valid for `flags`
        // (the caller's contract), and `info` is writable.
        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(command.as_mut_ptr())),
                None,
                None,
                false,
                flags,
                None,
                PCWSTR::null(),
                startup,
                &raw mut info,
            )
        }
        .expect("the process starts");
        Self(info)
    }

    /// Waits up to 10 s for the process to exit and returns its exit code. A process still
    /// running then is ended, so a failing test leaves nothing behind.
    fn exit_code(&self) -> u32 {
        // SAFETY: the process handle is valid for the life of `self`.
        let waited = unsafe { WaitForSingleObject(self.0.hProcess, 10_000) };
        if waited != WAIT_OBJECT_0 {
            // SAFETY: as above; ends the process this test started.
            let _ = unsafe { TerminateProcess(self.0.hProcess, 1) };
            panic!("the program did not end within 10 s");
        }
        let mut code = 0u32;
        // SAFETY: as above; `code` is writable.
        unsafe { GetExitCodeProcess(self.0.hProcess, &raw mut code) }.expect("the exit code");
        code
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        // SAFETY: both handles came from `CreateProcessW` and are closed once, here.
        unsafe {
            let _ = CloseHandle(self.0.hProcess);
            let _ = CloseHandle(self.0.hThread);
        }
    }
}

/// A pseudoconsole, closed on drop, which ends every process still attached to it.
struct PseudoConsole(HPCON);

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreatePseudoConsole` and is closed once, here.
        unsafe { ClosePseudoConsole(self.0) };
    }
}

fn program() -> &'static str {
    env!("CARGO_BIN_EXE_overlay-lock")
}

/// Runs `--make-hash` under `cmd.exe` in a pseudoconsole, sends `typed` as one write (as a paste
/// does), and returns the exit code and everything the console showed.
fn make_hash_in_pseudo_console(typed: &str) -> (u32, String) {
    let (input_read, mut input_write) = std::io::pipe().expect("a pipe");
    let (mut output_read, output_write) = std::io::pipe().expect("a pipe");
    // SAFETY: both handles are open pipe ends, which the pseudoconsole duplicates.
    let console = PseudoConsole(
        unsafe {
            CreatePseudoConsole(
                COORD { X: 120, Y: 40 },
                HANDLE(input_read.as_raw_handle()),
                HANDLE(output_write.as_raw_handle()),
                0,
            )
        }
        .expect("the pseudoconsole opens"),
    );
    drop((input_read, output_write));
    // The pseudoconsole blocks when its output is not read, so a thread reads it all.
    let reader = thread::spawn(move || {
        let mut shown = Vec::new();
        let _ = output_read.read_to_end(&mut shown);
        shown
    });

    let mut size = 0usize;
    // SAFETY: asks for the size of a one-attribute list; it fails with that size, as designed.
    let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &raw mut size) };
    let mut list_buffer = vec![0u8; size];
    let list = LPPROC_THREAD_ATTRIBUTE_LIST(list_buffer.as_mut_ptr().cast());
    // SAFETY: `list_buffer` has the size asked for above and outlives the list.
    unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &raw mut size) }
        .expect("the attribute list initialises");
    // SAFETY: the attribute value is the pseudoconsole handle itself, as the API takes it, and
    // the console outlives the process creation below.
    unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(console.0.0 as *const c_void),
            size_of::<HPCON>(),
            None,
            None,
        )
    }
    .expect("the pseudoconsole attribute is set");
    let startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: u32::try_from(size_of::<STARTUPINFOEXW>()).expect("small struct"),
            // No standard handles: the child takes the pseudoconsole's, not this process's.
            dwFlags: STARTF_USESTDHANDLES,
            ..Default::default()
        },
        lpAttributeList: list,
    };
    let child = Child::start(
        &format!("cmd.exe /d /s /c \"\"{}\" --make-hash\"", program()),
        EXTENDED_STARTUPINFO_PRESENT,
        &startup.StartupInfo,
    );
    // SAFETY: the list was initialised above and is deleted once, after its last use.
    unsafe { DeleteProcThreadAttributeList(list) };

    input_write
        .write_all(typed.as_bytes())
        .expect("the input is sent");
    let code = child.exit_code();
    drop(input_write);
    // Closing the pseudoconsole ends its output pipe, which ends the reader.
    drop(console);
    let shown = reader.join().expect("the reader ends");
    (code, String::from_utf8_lossy(&shown).into_owned())
}

/// Finds the hash in what a console showed: the text from `pbkdf2-sha256:` to the next character
/// that is not part of one.
fn hash_in(shown: &str) -> String {
    let start = shown
        .find("pbkdf2-sha256:")
        .unwrap_or_else(|| panic!("no hash was shown: {shown:?}"));
    shown[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '+' | '/'))
        .collect()
}

/// Checks that `hash` unlocks a lock when `password` is typed.
fn assert_unlocks(hash: &str, password: &str) {
    let _serial = serial();
    let mut lock = Lock::start(&[
        "--password-hash",
        hash,
        "--monitors",
        "cursor",
        "--timeout",
        "20s",
    ]);
    post_text(lock.overlay(), password);
    assert_eq!(lock.exit_code_within(Duration::from_secs(2)), Some(0));
}

/// The password is never shown, Backspace edits it, and both entries sent at once each answer one
/// prompt. `!` appears in no prompt and no hash, so any `!` shown is an echo.
#[test]
fn make_hash_asks_twice_on_the_console_without_echo() {
    // A terminal sends DEL (0x7F) for the Backspace key, which the console delivers as Backspace.
    let (code, shown) = make_hash_in_pseudo_console("go!x\u{7f}!\rgo!!\r");
    assert_eq!(code, 0, "{shown:?}");
    assert!(shown.contains("Type the password to hash (it is not shown):"));
    assert!(shown.contains("Type it again:"));
    assert!(!shown.contains('!'), "the password was echoed: {shown:?}");
    assert_unlocks(&hash_in(&shown), "go!!");
}

#[test]
fn make_hash_refuses_two_different_entries() {
    let (code, shown) = make_hash_in_pseudo_console("go\rgx\r");
    assert_eq!(code, 1, "{shown:?}");
    assert!(shown.contains("not the same"), "{shown:?}");
    assert!(!shown.contains("pbkdf2-sha256:"), "{shown:?}");
}

/// Opens a device of the attached console.
fn open(name: PCWSTR) -> HANDLE {
    // SAFETY: `name` is a static wide string; no security attributes or template.
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
    .expect("the console device opens")
}

/// Puts every unit of `text` into the attached console's input in one write.
fn type_into_console(text: &str) {
    let records: Vec<INPUT_RECORD> = text
        .encode_utf16()
        .map(|unit| INPUT_RECORD {
            EventType: u16::try_from(KEY_EVENT).expect("small value"),
            Event: INPUT_RECORD_0 {
                KeyEvent: KEY_EVENT_RECORD {
                    bKeyDown: true.into(),
                    wRepeatCount: 1,
                    uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unit },
                    ..Default::default()
                },
            },
        })
        .collect();
    let input = open(w!("CONIN$"));
    let mut written = 0u32;
    // SAFETY: `input` is the console input handle and `records` outlives the call.
    unsafe { WriteConsoleInputW(input, &records, &raw mut written) }.expect("the input is written");
    assert_eq!(written as usize, records.len());
    // SAFETY: opened above and closed once.
    let _ = unsafe { CloseHandle(input) };
}

/// Reads the attached console's screen buffer, one string per row that is not blank.
fn screen() -> Vec<String> {
    let output = open(w!("CONOUT$"));
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    // SAFETY: `output` is the console screen buffer and `info` is writable.
    unsafe { GetConsoleScreenBufferInfo(output, &raw mut info) }.expect("the buffer info reads");
    let width = usize::try_from(info.dwSize.X).expect("positive width");
    let height = usize::try_from(info.dwSize.Y).expect("positive height");
    let mut text = vec![0u16; width * height];
    let mut read = 0u32;
    // SAFETY: as above; `text` is writable for its length.
    unsafe { ReadConsoleOutputCharacterW(output, &mut text, COORD { X: 0, Y: 0 }, &raw mut read) }
        .expect("the buffer reads");
    // SAFETY: opened above and closed once.
    let _ = unsafe { CloseHandle(output) };
    text.chunks(width)
        .map(|row| String::from_utf16_lossy(row).trim_end().to_owned())
        .filter(|row| !row.is_empty())
        .collect()
}

/// With no console and no standard handles, as from a shortcut, the program opens a console,
/// asks there, and keeps the hash on screen until Enter.
#[test]
#[ignore = "leaves this process's console and shows a console window; run by hand"]
fn make_hash_without_standard_input_opens_a_console() {
    // SAFETY: leaves the console this test started in, so it can attach to the program's.
    let _ = unsafe { FreeConsole() };
    let startup = STARTUPINFOW {
        cb: u32::try_from(size_of::<STARTUPINFOW>()).expect("small struct"),
        dwFlags: STARTF_USESTDHANDLES,
        ..Default::default()
    };
    let child = Child::start(
        &format!("\"{}\" --make-hash", program()),
        DETACHED_PROCESS,
        &startup,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    // SAFETY: attaches to the child's console; it fails until the child has opened one.
    while unsafe { AttachConsole(child.0.dwProcessId) }.is_err() {
        assert!(
            Instant::now() < deadline,
            "the program opened no console within 10 s"
        );
        sleep(Duration::from_millis(20));
    }
    type_into_console("go\rgo\r\r");
    let code = child.exit_code();
    let rows = screen();
    // SAFETY: leaves the child's console, which then closes.
    let _ = unsafe { FreeConsole() };
    assert_eq!(code, 0, "{rows:#?}");
    assert!(
        rows.iter()
            .any(|row| row == "Copy the hash above, then press Enter to close."),
        "{rows:#?}"
    );
    assert_unlocks(&hash_in(&rows.join("\n")), "go");
}
