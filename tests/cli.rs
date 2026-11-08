//! Runs the built executable with help, version and usage-error command lines.

mod common;

use std::process::{Command, Output, Stdio};

use common::{make_hash, run_make_hash};

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

#[test]
fn make_hash_prints_one_line_in_the_documented_form() {
    let hash = make_hash("Purr 42");
    let fields: Vec<&str> = hash.split(':').collect();
    assert_eq!(fields.len(), 5, "{hash}");
    assert_eq!(fields[0], "pbkdf2-sha256");
    assert_eq!(fields[1], "5000");
    assert_eq!(fields[2], "7", "the length of Purr 42 in UTF-16 units");
    assert!(!hash.contains('\n'));
    assert_ne!(make_hash("Purr 42"), hash, "each hash has its own salt");
}

#[test]
fn make_hash_with_empty_input_fails_with_1() {
    let output = run_make_hash(b"\n");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must not be empty"), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn make_hash_with_null_input_fails_with_1_and_opens_no_console() {
    let output = Command::new(env!("CARGO_BIN_EXE_overlay-lock"))
        .arg("--make-hash")
        .stdin(Stdio::null())
        .output()
        .expect("the executable starts");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not be empty"));
}

#[test]
fn make_hash_with_another_option_is_a_usage_error() {
    let output = run(&["--make-hash", "-p", "go"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--make-hash"));
}

#[test]
fn a_malformed_password_hash_is_a_usage_error() {
    let output = run(&["--password-hash", "pbkdf2-sha256:5000:2:abc"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--password-hash: not a password hash")
    );
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
