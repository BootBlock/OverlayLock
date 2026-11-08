//! Runs the built executable with help, version and usage-error command lines.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_overlay-lock"))
        .args(args)
        .output()
        .expect("the executable starts")
}

#[test]
fn help_prints_to_stdout_and_exits_0() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("--password"));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn no_arguments_is_a_usage_error_naming_password() {
    let output = run(&[]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--password"), "stderr: {stderr}");
    assert!(
        stderr.contains("Run overlay-lock --help for the options."),
        "stderr: {stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn version_prints_the_crate_version() {
    let output = run(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains(env!("CARGO_PKG_VERSION")));
}

/// A console program makes a shell wait for it, so its text and exit code arrive in order; the
/// manifest's `detached` policy keeps Windows from giving it a console of its own (OL4).
#[test]
fn the_executable_is_a_console_program_with_a_detached_console_policy() {
    const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;
    let image = std::fs::read(env!("CARGO_BIN_EXE_overlay-lock")).expect("the executable reads");
    let pe = usize::try_from(u32::from_le_bytes(
        image[0x3C..0x40].try_into().expect("four bytes"),
    ))
    .expect("the PE offset fits");
    assert_eq!(&image[pe..pe + 4], b"PE\0\0", "no PE signature");
    // The optional header follows the 4-byte signature and the 20-byte file header; its
    // `Subsystem` field is at offset 68 in both PE32 and PE32+.
    let subsystem = pe + 24 + 68;
    let subsystem = u16::from_le_bytes(
        image[subsystem..subsystem + 2]
            .try_into()
            .expect("two bytes"),
    );
    assert_eq!(subsystem, IMAGE_SUBSYSTEM_WINDOWS_CUI);
    let manifest = String::from_utf8_lossy(&image);
    assert!(
        manifest.contains(
            "<consoleAllocationPolicy xmlns=\"http://schemas.microsoft.com/SMI/2024/WindowsSettings\">detached</consoleAllocationPolicy>"
        ),
        "the embedded manifest has no detached console allocation policy"
    );
}
