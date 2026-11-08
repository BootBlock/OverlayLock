//! OverlayLock covers the screen with a borderless, topmost overlay and hides the cursor until the
//! password given on the command line, or the password whose hash was given, is typed.

mod app;
mod cli;
mod colour;
mod duration;
mod focus;
mod hooks;
mod instance;
mod make_hash;
mod monitors;
mod overlay;
mod password;
mod render;
mod report;
mod secure_desktop;

use std::process::ExitCode;
use std::rc::Rc;

use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, TranslateMessage,
};

use app::App;
use cli::{Command, Options};
use instance::InstanceGuard;
use report::{ExitStatus, Severity, report};

fn main() -> ExitCode {
    report::release_console_made_for_us();
    run().into()
}

fn run() -> ExitStatus {
    match cli::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => {
            report(Severity::Info, &cli::help());
            ExitStatus::Unlocked
        }
        Ok(Command::Version) => {
            report(
                Severity::Info,
                concat!("overlay-lock ", env!("CARGO_PKG_VERSION")),
            );
            ExitStatus::Unlocked
        }
        Ok(Command::MakeHash) => make_hash::run(),
        Ok(Command::Run(options)) => lock(options),
        Err(error) => {
            report(
                Severity::Error,
                &format!("overlay-lock: {error}\nRun overlay-lock --help for the options."),
            );
            ExitStatus::Usage
        }
    }
}

/// Covers the monitors until the lock ends, and returns how it ended.
fn lock(options: Options) -> ExitStatus {
    // The guard lives until this function returns, so the lock holds the mutex while it runs.
    let _guard = match InstanceGuard::acquire() {
        Ok(Some(guard)) => guard,
        Ok(None) => return ExitStatus::AlreadyLocked,
        Err(error) => return failure(&error),
    };
    let app = match App::new(options) {
        Ok(app) => Rc::new(app),
        Err(error) => return failure(&error),
    };
    if let Err(error) = app.start() {
        return failure(&error);
    }
    let status = message_loop();
    app.finish();
    status
}

/// Runs the message loop until `WM_QUIT`.
fn message_loop() -> ExitStatus {
    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is a valid, writable `MSG`.
        match unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 {
            0 => return ExitStatus::Unlocked,
            -1 => return failure(&windows::core::Error::from_thread()),
            _ => {
                // SAFETY: `message` was filled by `GetMessageW` above.
                unsafe {
                    let _ = TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                }
            }
        }
    }
}

/// Reports a runtime failure and returns its exit status.
fn failure(error: &windows::core::Error) -> ExitStatus {
    report(
        Severity::Error,
        &format!("overlay-lock: {}", error.message()),
    );
    ExitStatus::Failure
}
