//! Keeps the README's copy of the help text identical to the program's own `--help` output.

use std::process::Command;

#[test]
fn readme_contains_the_exact_help_text() {
    let output = Command::new(env!("CARGO_BIN_EXE_overlay-lock"))
        .arg("--help")
        .output()
        .expect("the executable starts");
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).expect("the help text is UTF-8");
    let readme_path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
    let readme = std::fs::read_to_string(readme_path).expect("README.md is readable");
    let block = format!("```text\n{}\n```", help.trim_end());
    assert!(
        readme.replace("\r\n", "\n").contains(&block),
        "README.md must contain the exact output of overlay-lock --help in a text block"
    );
}
