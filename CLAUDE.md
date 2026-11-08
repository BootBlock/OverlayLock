# OverlayLock

`overlay-lock` is a small Win32 application in Rust that covers the screen until a password is
typed. The locked design (decisions OL1-OL22) is in `plans/`, under the rewrite plan's
`00-overview.md`. Read it before you change options, hooks, monitors or rendering.

## Build and test

Run all four before you finish a change, from the repository root:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build` (debug while iterating; release only when a check names it, as it is fat LTO)

A running `overlay-lock.exe` locks the exe and fails the link. Every live run carries
`--timeout 60s` or less, so a fault cannot leave the desktop locked.

## Code rules

- `unsafe` only in the Win32 modules, and every `unsafe` block has a `// SAFETY:` comment.
- No new dependency without a reason stated in the change.
- Never hold a `RefCell` borrow across a Win32 call that can send a message.
- A test must be able to fail on the old code.
- British English in every string, comment and doc (`--color` stays as an alias only).

## Plans

Plans live in `plans/`. Filing rules are in `plans/_complete/README.md`.

## Instruction budget

`tests/instruction_budget.rs` enforces: this file at most 2,500 characters, each `##` section at
most 700, and `AGENTS.md` at most 200. Move detail to a note; never raise a budget without asking.
