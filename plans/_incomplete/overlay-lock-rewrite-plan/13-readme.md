# 13 README

> **Status: done 2026-11-08.** `cargo xtask icon` now also writes `assets/overlay-lock-256.png`
> for the README (GitHub does not render ICO). `tests/readme.rs` compares the README with the
> built exe's real `--help` output (the help const lives in a binary crate, which a test cannot
> import). Commands run: `--help`, `cargo build --release` (both targets), `cargo xtask icon`, the
> hook test command, and every example lock with `--timeout 60s` added (the cat example's 30 m
> shortened to 60 s); the Stream Deck Open action example ran from the owner's Stream Deck.
> `cargo install --git ...` was not run: the repository does not exist on GitHub yet. A
> `taskkill` without `/f` was checked against the README claim (unlocks; ignored with
> `--ignore-alt-f4`). The claudisms scanner found nothing.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 10, 11, 12 done. Locked: OL5, OL22.

## Touch points

| File | Change |
|---|---|
| `README.md` | New. |
| `Cargo.toml` | `readme = "README.md"`, `keywords`, `categories`. |

## Design

Use the `documentation-voice` skill for the wording. Sections:

1. **Title, icon, one-line summary**, CI badge for `BootBlock/OverlayLock`.
2. **What it is, and what it is not.** It covers the screen until a password is typed. It is not a
   security tool: the password is in the command line, which any process of the same user can read,
   and Ctrl+Alt+Del and Win+L always work.
3. **Install.** Download a release zip, or `cargo install --git
   https://github.com/BootBlock/OverlayLock`. No runtime to install.
4. **Usage.** The `--help` text from `src/cli.rs`, copied exactly. Add examples: a Stream Deck
   "Open" action, a Ctrl+Alt+Del-safe lock for a child or a cat, a message screen.
5. **Behaviour.** Monitors, focus taking, the dead-key limit in `--block-keys` mode, Alt+F4,
   timeout, single instance and exit codes.
6. **Recovery.** What works when the password is forgotten, from the stage 10 record: `--timeout`,
   `--yield-on-ctrl-alt-del`, and sign-out from the Ctrl+Alt+Del screen.
7. **Build from source.** Toolchain, `cargo build --release`, `cargo xtask icon`.
8. **Licence.** MIT, link to `LICENSE`.

A test in `tests/readme.rs` asserts that the README contains the exact help text, so the two cannot
drift.

## Done when

- [x] `tests/readme.rs` passes, and fails when one option line in the README is edited.
- [x] Every command in the README was run once and behaves as written.
- [x] The `claudisms` skill scan of `README.md` finds nothing.
- [x] Status line flipped, stage table updated.
