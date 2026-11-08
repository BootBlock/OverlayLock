# 02 Command line, colours, durations and reporting

> **Status: done 2026-11-08.** The owner ran `--help` from PowerShell: the help printed. Differences: the named colours
> were parsed from the W3C TR page's table (148, hex cross-checked against the decimal column);
> `report` writes to an inherited console handle with `WriteConsoleW` (UTF-16), not UTF-8
> `WriteFile`; `windows` features `Win32_Security` and `Win32_System_IO` added (`CreateFileW`,
> `WriteFile`); `Colour::to_unit_rgb` added for 07.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 and 04 done (`monitors::Monitors`). Locked: OL4, OL5, OL16 (exit code 3).

## Touch points

| File | Change |
|---|---|
| `src/cli.rs` | New. `Options`, `Monitors`, `MessageStyle`, `UsageError`, `Command`, `parse`, help text. |
| `src/colour.rs` | New. `Colour { r: u8, g: u8, b: u8 }`, `FromStr`, the CSS named-colour table. |
| `src/duration.rs` | New. `parse_duration(&str) -> Result<Duration, DurationError>`. |
| `src/report.rs` | New. `report(Severity, &str)` with the OL4 sink order, `ExitStatus` codes. |
| `src/main.rs` | `mod` lines. `main` parses, reports usage errors or help/version, and returns the code. |
| `Cargo.toml` | `windows` features: `Win32_System_Console`, `Win32_Storage_FileSystem` (for `WriteFile`/`GetFileType`). |

Conflict note: none. 02 runs alone in its block.

## Design

- `cli::parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError>` with
  `enum Command { Run(Options), Help, Version }`. It takes the iterator so tests never touch the real
  process arguments. Use `lexopt::Parser::from_args`.
- `Options` fields: `password: Vec<u16>` (UTF-16, non-empty), `monitors: monitors::Monitors`,
  `background: Colour`, `message: Option<MessageStyle>`, `block_keys: bool`, `block_mouse: bool`,
  `timeout: Option<Duration>`, `alt_f4_unlocks: bool`, `yield_on_ctrl_alt_del: bool`.
  `MessageStyle { text: String, colour: Colour, size_points: f32, font: Option<String> }`. A `None`
  font means the OL5 default chain, resolved in stage 07 where DirectWrite is available.
- The password comes from `OsString` through `encode_wide`, so a password that is not valid
  Unicode still round-trips exactly. The message text must be valid Unicode (usage error if not).
- `UsageError` names the option and the reason, for example `--text-size: 500 is outside 4 to 400`.
  Duplicate options are errors. `--text-colour`, `--text-size` and `--font` without `--message` are
  errors. `--color` and `--text-color` are aliases.
- Message escapes: `\n` gives U+000A, `\\` gives `\`, any other `\x` is a usage error. Decode once in
  `cli.rs`.
- `colour.rs`: accept `#RGB`, `#RRGGBB`, `RGB`, `RRGGBB` (hex is case-insensitive), then the 148 CSS
  Color 4 names as a `static` slice sorted by name, looked up with `binary_search_by` on the
  ASCII-lowercased input. Copy the table from the CSS Color 4 specification, not from memory, and
  test a sample of ten names against the specification values plus a test that the slice is sorted
  and has 148 entries. `Colour::to_colorref()` gives the GDI `COLORREF` (`0x00BBGGRR`).
- `duration.rs`: one or more `<number><unit>` groups, units `h`, `m`, `s`, in that order, each at
  most once, no spaces, whole numbers. Total must be 1 s to 7 days. `90s`, `30m`, `1h30m`, `2h` are
  valid. `30`, `1m1h`, `0s`, `1.5h` are not.
- `report.rs`: `enum ExitStatus { Unlocked = 0, Failure = 1, Usage = 2, AlreadyLocked = 3 }` with
  `impl From<ExitStatus> for ExitCode`. `report` writes help and version to stdout, errors to
  stderr. Sink order: if `GetStdHandle` gives a handle whose `GetFileType` is not
  `FILE_TYPE_UNKNOWN`, write UTF-8 with `WriteFile`. Otherwise `AttachConsole(ATTACH_PARENT_PROCESS)`
  and write with `WriteConsoleW` to `CONOUT$`. Otherwise `MessageBoxW` with title `OverlayLock`,
  `MB_ICONERROR` or `MB_ICONINFORMATION`. Usage errors end with "Run overlay-lock --help for the
  options."
- Help text: a one-line summary, the usage line, every OL5 option with its default, and the exit
  codes. It is one `const &str`, the only list of options in prose. The README (stage 13) copies it.

## Done when

- [x] Unit tests cover: every option, both aliases, each default, each usage error class (missing
      password, empty password, duplicate, unknown option, positional, out of range, message option
      without `--message`, bad escape), all valid and invalid durations above, hex in four forms,
      ten named colours, and the table invariants.
- [x] A process test in `tests/cli.rs` runs the built exe (`env!("CARGO_BIN_EXE_overlay-lock")`)
      with `--help` (exit 0, stdout has `--password`), with no arguments (exit 2, stderr names
      `--password`), and with `--version` (exit 0, stdout has the crate version).
- [x] Running `target\debug\overlay-lock.exe --help` from PowerShell prints the help in the console.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.
