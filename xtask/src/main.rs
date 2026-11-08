//! Development tasks for OverlayLock, run as `cargo xtask <command>`.

mod icon;

use std::process::ExitCode;

const USAGE: &str = "Usage: cargo xtask <command>\n\nCommands:\n  icon    render assets/overlay-lock.ico (see cargo xtask icon --help)";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        None | Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("icon") => icon::run(args),
        Some(other) => Err(format!("unknown command '{other}'\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}
