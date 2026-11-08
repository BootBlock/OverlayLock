# 01 Scaffold

> **Status: done 2026-11-08.** The ARM64 build first failed: Visual Studio 18 had no MSVC ARM64
> build tools. The owner installed `Microsoft.VisualStudio.Component.VC.Tools.ARM64` (decisions
> d-f554e2cc, then d-790f62ca elevated), and the ARM64 release build then passed (`AA64`, no
> `VCRUNTIME140.dll`). Also added `clippy.toml` (`doc-valid-idents` for product names) and
> `winresource` with `default-features = false` (its `toml` feature is unused).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: none. Locked: OL1, OL2, OL3, OL4, OL22.

The folder holds only `plans/` today. It is not a git repository.

## Touch points

| File | Change |
|---|---|
| `Cargo.toml` | Package `overlay-lock` 0.1.0, edition 2024, `rust-version = "1.98"`, `license = "MIT"`, `repository = "https://github.com/BootBlock/OverlayLock"`, `description`, `authors = ["Joe Cox"]`. `[workspace] members = [".", "xtask"]`. Dependencies per OL2 with only the `windows` features each later stage needs (start with `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_System_LibraryLoader`). `[profile.release]` per OL3. `[lints.rust]` `unsafe_op_in_unsafe_fn = "deny"`, `missing_docs = "warn"`. `[lints.clippy]` `pedantic = { level = "warn", priority = -1 }`, `undocumented_unsafe_blocks = "warn"`. |
| `.cargo/config.toml` | `[target.x86_64-pc-windows-msvc]` and `[target.aarch64-pc-windows-msvc]` `rustflags = ["-C", "target-feature=+crt-static"]`. `[alias] xtask = "run --package xtask --"`. |
| `build.rs` | `winresource`: embed `assets/overlay-lock.manifest`, a version resource (ProductName `OverlayLock`, FileDescription `OverlayLock`, LegalCopyright `Copyright (c) 2026 Joe Cox`, versions from `CARGO_PKG_VERSION`). Stage 11 adds the icon. `cargo:rerun-if-changed` for each asset. |
| `assets/overlay-lock.manifest` | `asInvoker`, `uiAccess="false"`; `dpiAwareness` `PerMonitorV2` with `dpiAware` `true/pm` fallback; Common Controls v6 dependency; `supportedOS` Windows 10/11 GUID. |
| `src/main.rs` | `#![windows_subsystem = "windows"]`, crate doc comment, `fn main() -> ExitCode` returning `ExitCode::SUCCESS`. Later stages add modules. |
| `xtask/Cargo.toml`, `xtask/src/main.rs` | Empty command dispatcher (`cargo xtask <command>`) that reports unknown commands. Stage 11 adds `icon`. |
| `.gitignore`, `.gitattributes` | `/target`. `* text=auto eol=lf`, `*.ico binary`, `*.png binary`. |
| `LICENSE` | MIT text, `Copyright (c) 2026 Joe Cox`. |
| `rustfmt.toml` | `edition = "2024"`, `max_width = 100`. |
| `CLAUDE.md`, `AGENTS.md`, `tests/instruction_budget.rs` | See below. |

## Design

- Run `git init -b main` first. Do not commit (the seed forbids it).
- Pin `windows = "0.62"`, `lexopt = "0.3"`, `winresource = "0.1"` and check each version with
  `cargo search` before writing it. Run `cargo generate-lockfile`.
- The manifest is the only way to get PMv2 at process start. Do not call
  `SetProcessDpiAwarenessContext` in code as well.
- `CLAUDE.md` holds only rules for every change, as the owner's global instructions require:
  build and test commands (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`), "unsafe only in Win32 modules with SAFETY comments", "no new dependency without a
  reason in the change", "plans live in `plans/`", and the budget numbers. Budget: whole file 2,500
  characters, each `##` section 700, `AGENTS.md` 200. `AGENTS.md` is one line pointing to
  `CLAUDE.md`. `tests/instruction_budget.rs` reads both files and fails past each budget.

## Done when

- [x] `cargo build` and `cargo build --release` succeed for `x86_64-pc-windows-msvc`.
- [x] `cargo build --release --target aarch64-pc-windows-msvc` succeeds (the target is installed).
- [x] `dumpbin /dependents target\release\overlay-lock.exe` lists no `VCRUNTIME140.dll` (crt-static
      works). If `dumpbin` is not on `PATH`, find it under the Visual Studio install with `vswhere`.
- [x] The manifest is embedded: `mt.exe -inputresource:target\release\overlay-lock.exe;#1 -out:con`
      shows `PerMonitorV2`, or the resource is seen with another resource viewer.
- [x] `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean.
- [x] `cargo test` passes, and `tests/instruction_budget.rs` fails when you pad `CLAUDE.md` past
      2,500 characters (then remove the padding).
- [x] Status line flipped, stage table in `00-overview.md` updated.
