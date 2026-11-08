//! Keeps the README's copy of the help text identical to the program's own `--help` output, apart
//! from the build date in the first line, which changes with every build.

use std::process::Command;

/// The text before the build date in the help's first line.
const DATE_MARK: &str = ", built on the ";

/// Splits help text at the build date: everything up to and including [`DATE_MARK`], the date,
/// and everything after the first line. Panics if the first line has no build date.
fn split_date(help: &str) -> (&str, &str, &str) {
    let (first, rest) = help
        .split_once('\n')
        .expect("the help has more than one line");
    let at = first
        .find(DATE_MARK)
        .expect("the first line has a build date")
        + DATE_MARK.len();
    (&first[..at], &first[at..], rest)
}

/// Returns `true` if `date` reads like `8th November 2026`; otherwise, `false`.
fn is_long_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split(' ').collect();
    matches!(parts.as_slice(), [day, month, year]
        if ["st", "nd", "rd", "th"].iter().any(|suffix| {
            day.strip_suffix(suffix).is_some_and(|n| n.parse::<u8>().is_ok_and(|n| (1..=31).contains(&n)))
        })
        && month.chars().next().is_some_and(char::is_uppercase)
        && year.len() == 4 && year.parse::<u16>().is_ok())
}

#[test]
fn readme_contains_the_exact_help_text() {
    let output = Command::new(env!("CARGO_BIN_EXE_overlay-lock"))
        .arg("--help")
        .output()
        .expect("the executable starts");
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8(output.stdout).expect("the help text is UTF-8");
    let help = help.replace("\r\n", "\n");
    let help = help.trim_end();

    let readme_path = concat!(env!("CARGO_MANIFEST_DIR"), "/README.md");
    let readme = std::fs::read_to_string(readme_path)
        .expect("README.md is readable")
        .replace("\r\n", "\n");
    let start = readme
        .find("```text\nOverlayLock ")
        .expect("README.md has the help text in a text block")
        + "```text\n".len();
    let block = &readme[start..];
    let block = &block[..block.find("\n```").expect("the text block is closed")];

    let (help_head, help_date, help_rest) = split_date(help);
    let (readme_head, readme_date, readme_rest) = split_date(block);
    assert!(is_long_date(help_date), "--help shows {help_date:?}");
    assert!(is_long_date(readme_date), "README.md shows {readme_date:?}");
    assert_eq!(
        readme_head, help_head,
        "the first line differs before the date"
    );
    assert_eq!(
        readme_rest, help_rest,
        "README.md must contain the exact output of overlay-lock --help in a text block"
    );
}
