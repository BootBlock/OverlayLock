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
