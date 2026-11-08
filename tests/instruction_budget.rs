//! Keeps `CLAUDE.md` and `AGENTS.md` inside their character budgets.

use std::fs;
use std::path::Path;

const CLAUDE_MD_BUDGET: usize = 2_500;
const SECTION_BUDGET: usize = 700;
const AGENTS_MD_BUDGET: usize = 200;

fn read(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn chars(text: &str) -> usize {
    text.chars().count()
}

/// Splits a Markdown document into its `##` sections, each with its heading line.
fn sections(text: &str) -> Vec<(&str, &str)> {
    let mut starts: Vec<usize> = text
        .match_indices("\n## ")
        .map(|(index, _)| index + 1)
        .collect();
    if text.starts_with("## ") {
        starts.insert(0, 0);
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = starts.get(n + 1).copied().unwrap_or(text.len());
            let section = &text[start..end];
            let heading = section.lines().next().unwrap_or_default();
            (heading, section)
        })
        .collect()
}

#[test]
fn claude_md_is_within_budget() {
    let text = read("CLAUDE.md");
    let count = chars(&text);
    assert!(
        count <= CLAUDE_MD_BUDGET,
        "CLAUDE.md has {count} characters; the budget is {CLAUDE_MD_BUDGET}"
    );
}

#[test]
fn each_claude_md_section_is_within_budget() {
    let text = read("CLAUDE.md");
    let sections = sections(&text);
    assert!(!sections.is_empty(), "CLAUDE.md has no ## sections");
    for (heading, section) in sections {
        let count = chars(section);
        assert!(
            count <= SECTION_BUDGET,
            "CLAUDE.md section '{heading}' has {count} characters; the budget is {SECTION_BUDGET}"
        );
    }
}

#[test]
fn agents_md_is_within_budget() {
    let text = read("AGENTS.md");
    let count = chars(&text);
    assert!(
        count <= AGENTS_MD_BUDGET,
        "AGENTS.md has {count} characters; the budget is {AGENTS_MD_BUDGET}"
    );
}
