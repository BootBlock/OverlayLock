# OverlayLock Rust rewrite plan

> **Status: incomplete, 2026-11-08.** Every stage ran, and the app works live on every check run.
> Left open: the first remote CI run (needs the owner to create the GitHub repository and push),
> the live monitor-scale change (stage 07; Windows asks for a sign-out here), the Lite parity
> run (stage 14), and a live unplug with `--monitors cursor` for the OL10 amendment. A follow-up
> plan in `plans/_future/uac-yield-spike-plan/` covers UAC prompts that trigger the yield.
> Nothing is committed. Authored 2026-11-08. Lives at `plans/overlay-lock-rewrite-plan/` while
> it is written and run. Filing rules: `plans/_complete/README.md`.

## What this adds

A from-scratch Rust rewrite of OverlayLockLite (`P:\Source\DotNet\OverlayLockLite`, C# WinForms,
about 120 lines) as `overlay-lock`, a small, fast, dependency-light Win32 application. It keeps the
Lite behaviour (a black, borderless, topmost overlay with a hidden cursor that closes when the last
typed characters match a password) and adds what the owner chose on 2026-11-08: the password on the
command line, every monitor covered (or only the cursor's), a background colour and a centred
message, opt-in low-level keyboard and mouse hooks that swallow input system-wide, a timeout, an
optional yield to Task Manager after Ctrl+Alt+Del, an Alt+F4 switch, single-instance protection, a
new icon, CI, tagged release builds, an MIT licence and a README. The repository will be public at
`github.com/BootBlock/OverlayLock`. It is not a security product: the password is visible in the
process command line, and a config file comes in a later plan.

## Grounding a stage agent must not re-derive

- **What Lite does, exactly** (`OverlayLockLite/Main.cs`, `Main.Designer.cs`):
  - The form is `FormBorderStyle.None`, `TopMost = true`, `ShowInTaskbar = false`, `BackColor =
    Color.Black`, no icon, no control box (`Main.Designer.cs:42-53`).
  - `Main_Load` (`Main.cs:23-40`) sizes the form to `Screen.FromPoint(Cursor.Position).Bounds`
    (the full monitor, not the work area), maximises it, calls `Cursor.Hide()` and starts a
    one-shot 250 ms timer (`tmrEnsureFocus`, `Main.Designer.cs:36`).
  - The timer tick (`Main.cs:80-87`) disables itself, then calls `Activate`, `BringToFront` and
    `ForceForegroundWindow`: `AttachThreadInput(foregroundThread, ownThread, true)`,
    `BringWindowToTop`, `ShowWindow(SW_SHOW)`, detach (`Main.cs:90-110`). The comment says the
    window may lack focus when a Stream Deck launches it.
  - `Main_KeyPress` (`Main.cs:48-64`) keeps a queue of the last `UnlockPassword.Length` characters
    and closes when the queue equals the password. The match is case-sensitive. Every `KeyChar`
    counts, control characters included (Backspace is a character, not a delete).
  - The password is the constant `"go"` (`Main.cs:18`). There is no argument parsing.
  - In `DEBUG` builds the form is windowed and not topmost (`Main.cs:25-27`).
  - Alt+F4 closes the form, because WinForms accepts it. Nothing else closes it.
- **The older VB.NET OverlayLock** (`P:\Source\DotNet\OverlayLock`, `Forms/Overlay.vb`) matched the
  password case-insensitively with `EndsWith` on an unbounded string, and had gradient and image
  themes. It is reference only: nothing ports from it.
- **Low-level hooks.** `WH_KEYBOARD_LL` and `WH_MOUSE_LL` run on the thread that installed them, and
  that thread must pump messages. Windows removes a hook silently when its callback exceeds
  `LowLevelHooksTimeout` (about 1 s on Windows 10/11 when the value is absent from the registry),
  so a callback must never block or allocate. Returning a non-zero value swallows the event.
  Ctrl+Alt+Del (the secure attention sequence, SAS) and Win+L never reach a hook. A hook in a
  medium-integrity process may not see input sent to an elevated window (UIPI); stage 14 measures
  this for Task Manager instead of assuming it.
- **Key state in a hook.** A swallowed key never updates the system key state, so
  `GetKeyboardState` is useless inside a swallowing hook. The translator keeps its own 256-byte
  key state, seeded from `GetAsyncKeyState` and `GetKeyState(VK_CAPITAL)` when the hook installs,
  and passes it to `ToUnicodeEx` with flag `0x4` (do not change the kernel keyboard state; Windows
  10 1607 and later).
- **Per-monitor DPI v2.** With the PMv2 manifest entry, `EnumDisplayMonitors` and
  `GetMonitorInfoW` return physical pixels, and windows get `WM_DPICHANGED` when they cross a DPI
  boundary. The overlay sizes itself from `rcMonitor` and ignores the suggested rectangle.
- **GUI subsystem output.** A `windows_subsystem = "windows"` binary has no console. Its standard
  handles are valid only when a parent redirected them (a test harness pipe, `> file`).

## Locked decisions

- **OL1 Platform.** Windows 10 1809 or later, `x86_64-pc-windows-msvc` and
  `aarch64-pc-windows-msvc`. Rust edition 2024, `rust-version = "1.98"`, stable toolchain.
- **OL2 Dependencies.** Runtime: `windows` 0.62 (needed for the Direct2D and DirectWrite COM
  interfaces) and `lexopt` 0.3 (minimal, strict argument parser). Build: `winresource` 0.1. The icon
  tool `xtask` uses only `windows` (DirectWrite, Direct2D, WIC). Nothing else without a stated reason in the stage. No GUI
  framework, no async runtime, no logging crate.
- **OL3 Build profile.** `[profile.release]`: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`,
  `panic = "abort"`, `strip = true`, `debug = false`. `.cargo/config.toml` sets
  `-C target-feature=+crt-static` for both MSVC targets, so the exe needs no VC++ redistributable.
- **OL4 Reporting.** `#![windows_subsystem = "windows"]` in every profile. A message (usage error,
  `--help`, `--version`, runtime failure) goes to the first sink that exists: the inherited stderr
  or stdout handle (redirected by a parent), then the parent console through
  `AttachConsole(ATTACH_PARENT_PROCESS)`, then a `MessageBoxW`. Tests always redirect, so they never
  see a message box. Exit codes: `0` unlocked, `1` runtime failure, `2` usage error, `3` another
  instance is already locked.
- **OL5 Command line.** Exactly these options, parsed in `src/cli.rs`:

  | Option | Value | Default |
  |---|---|---|
  | `-p`, `--password <TEXT>` | required, non-empty | none: usage error |
  | `--monitors <all\|cursor>` | which monitors to cover | `all` |
  | `--colour <COLOUR>` (alias `--color`) | background | `black` |
  | `--message <TEXT>` | centred text; `\n` is a line break, `\\` a backslash | none |
  | `--text-colour <COLOUR>` (alias `--text-color`) | message colour | `#CCCCCC` |
  | `--text-size <POINTS>` | 4 to 400, decimals allowed | `28` |
  | `--font <FAMILY>` | DirectWrite family name | `Segoe UI Variable Display`, else `Segoe UI` |
  | `--block-keys` | install the keyboard hook | off |
  | `--block-mouse` | install the mouse hook | off |
  | `--timeout <DURATION>` | unlock after it, e.g. `90s`, `30m`, `1h30m` | none |
  | `--ignore-alt-f4` | Alt+F4 does not unlock | off |
  | `--yield-on-ctrl-alt-del` | OL15 | off |
  | `-h`, `--help` / `-V`, `--version` | print and exit 0 | |

  A colour is `#RGB` or `#RRGGBB` (the `#` optional, case-insensitive) or a CSS Color 4 named colour
  (all 148, case-insensitive). Repeating an option, an unknown option, a positional argument or an
  out-of-range value is a usage error that names the option. Message options without `--message`
  are a usage error.
- **OL6 Password match.** The password is stored as UTF-16 code units. A fixed ring of the last N
  code units (N = password length, allocated once) is compared against the password after each
  unit. Case-sensitive. Every character counts, as in Lite. No allocation per key. One type,
  `PasswordMatcher`, serves both the window path and the hook path.
- **OL7 Overlay windows.** One `WS_POPUP` window per covered monitor, `WS_EX_TOPMOST |
  WS_EX_TOOLWINDOW` (not in the taskbar or Alt+Tab), sized to `rcMonitor`. Monitors are identified
  by `MONITORINFOEXW.szDevice`. On `WM_DISPLAYCHANGE` the set is reconciled: windows move and resize
  to their monitor's new rectangle, new monitors get a window, removed monitors lose theirs. With
  `--monitors cursor` the monitor under the cursor at start is kept, and the primary monitor
  replaces it if it disappears.
- **OL8 Painting.** Without `--message`, painting is GDI only: the window class brush is a solid
  brush of the background colour and nothing else draws. Direct2D and DirectWrite are never
  initialised. With `--message`, one shared `ID2D1Factory` and `IDWriteFactory` are created at
  start, and each window owns an `ID2D1HwndRenderTarget` and an `IDWriteTextLayout`, made on first
  paint, set to the window's DPI, resized on `WM_SIZE`, rebuilt on `WM_DPICHANGED` and on
  `D2DERR_RECREATE_TARGET`. Text is centred, wrapped at 80 % of the monitor width, size in points
  converted to DIPs (`pt × 96 / 72`). Painting happens only on `WM_PAINT`: no timer, no animation.
- **OL9 Cursor.** The window class cursor is null and `WM_SETCURSOR` sets a null cursor, so the
  cursor is hidden only over the overlays and returns when they go. No global `ShowCursor` counter.
- **OL10 Focus.** At show, the overlay on the cursor's monitor takes the foreground with Lite's
  `AttachThreadInput` sequence plus `SetForegroundWindow`. A 250 ms timer checks whether an overlay
  is foreground and repeats the sequence, at most 8 times (2 s). After an overlay was once
  foreground, the app never takes the focus again, so a later Task Manager window stays usable.
  *Amended by the owner on 2026-11-08:* when a reconcile destroys the focused overlay (its monitor
  was removed), the focus moves to another overlay before the destroy; if none exists (with
  `--monitors cursor`), the replacement overlay takes the focus back. Nothing else may take it.
- **OL11 Keyboard hook.** `--block-keys` installs `WH_KEYBOARD_LL` on a dedicated hook thread with
  its own message loop at `THREAD_PRIORITY_HIGHEST`. It swallows every key event. Key-downs pass
  through its own key state and `ToUnicodeEx` (flag `0x4`, layout of the hook thread at install)
  into the hook thread's `PasswordMatcher`. A dead key gives no character in hook mode (documented).
  Injected input (`LLKHF_INJECTED`, e.g. a Stream Deck text action) counts like physical input. On a
  match the hook thread posts an unlock message to the controller window (OL18).
- **OL12 Mouse hook.** `--block-mouse` installs `WH_MOUSE_LL` on the same hook thread and swallows
  every mouse event, movement included. Each flag works alone or with the other.
- **OL13 Alt+F4.** Alt+F4 unlocks in both modes unless `--ignore-alt-f4` is given. In focus mode the
  overlay handles `WM_SYSCOMMAND SC_CLOSE` and `WM_CLOSE`. In hook mode the keyboard hook detects
  F4 down with Alt down in its own key state and posts the unlock message. A `WM_CLOSE` from any
  other source (a polite `taskkill`) follows the same switch. `WM_QUERYENDSESSION` always returns `TRUE`, so shutdown and sign-out are never blocked.
- **OL14 Timeout.** `--timeout` sets a one-shot timer on the controller window. It unlocks with the
  same path as a password match.
- **OL15 Yield on Ctrl+Alt+Del.** With `--yield-on-ctrl-alt-del`, when the user returns from the
  Ctrl+Alt+Del screen, the overlays drop `WS_EX_TOPMOST` and the hooks are removed, so Task Manager
  started from that screen gets input. The overlays stay, the password and the timeout still unlock,
  and the yield lasts until unlock. A Win+L lock and unlock must not trigger it. Stage 10 proves the
  detection signal first (`SetWinEventHook(EVENT_SYSTEM_DESKTOPSWITCH)` with
  `WTSRegisterSessionNotification` to filter out lock, else `OpenInputDesktop` polling). If the two
  cannot be told apart, stop and say so.
- **OL16 Single instance.** A named mutex `Local\OverlayLock-6F1D2C4B-9A57-4E3B-8C0E-2B7D5A1F9E63`
  is taken after parsing. If it already exists, the process exits with code `3` and shows nothing.
- **OL17 Fail open.** `panic = "abort"`. A crash ends the process, and Windows then removes the hooks
  and the windows: a fault unlocks. Nothing restarts the app.
- **OL18 Structure.** A message-only controller window (`HWND_MESSAGE`) owns the timers, receives
  the unlock message from the hook thread and drives exit. One module per responsibility (stage 01
  lists them). `unsafe` stays in the Win32 modules, each block with a `// SAFETY:` comment.
  Window state is reached through `GWLP_USERDATA`. Never hold a `RefCell` borrow across a Win32
  call that can send a message (`DestroyWindow`, `SetWindowPos`, `SetForegroundWindow`), because
  the window procedure re-enters.
- **OL19 Same behaviour in every build.** Unlike Lite, debug builds behave as release builds. For a
  safe development run, use `--timeout 20s --monitors cursor`.
- **OL20 Tests.** Pure logic (CLI, colours, durations, escapes, matcher, monitor reconcile) has unit
  tests. `tests/` holds process tests that spawn the built exe with redirected output and
  `--timeout 20s`, find the overlay by class name `OverlayLock.Overlay` and the child's process ID,
  and post `WM_CHAR` for each password unit. Hook tests use `SendInput` and are `#[ignore]`, run in
  stage 14. A test that cannot fail on the old code is a defect.
- **OL21 Icon.** The SpectraWrite style (`P:\Source\DotNet\SpectraWrite\scripts\generate-spectrawrite-icon.ps1`):
  one Segoe Fluent Icons glyph (Segoe MDL2 Assets as fallback) on a transparent background, its
  outline scaled to 80 % of the frame and centred, filled with SpectraWrite's diagonal eight-stop
  rainbow gradient. `cargo xtask icon` renders it to `assets/overlay-lock.ico` with PNG frames at
  16, 20, 24, 32, 40, 48, 64, 128 and 256 px. The owner picks the glyph.
- **OL22 Repository.** MIT licence, `Copyright (c) 2026 Joe Cox`, repository
  `github.com/BootBlock/OverlayLock`, default branch `main`. `Cargo.lock` is committed. CI runs fmt,
  clippy with warnings denied and tests on Windows. A `v*` tag builds x64 and ARM64 zips with
  SHA-256 sums into a GitHub release. No push, tag or release is made by this plan.

## Stage map

| № | File | Gist | Status |
|---|---|---|---|
| 01 | `01-scaffold.md` | Cargo workspace, profile, manifest, version resource, git, LICENSE, CLAUDE.md | done 2026-11-08 |
| 02 | `02-cli.md` | `cli`, `colour`, `duration` modules, `report` sinks, exit codes | done 2026-11-08 |
| 03 | `03-password-matcher.md` | `PasswordMatcher` ring and tests | done 2026-11-08 |
| 04 | `04-monitors.md` | Monitor enumeration, selection, reconcile plan | done 2026-11-08 |
| 05 | `05-overlay-windows.md` | Window class, overlay set, controller window, focus-mode unlock, Alt+F4 | done 2026-11-08 (+ raise after display change) |
| 06 | `06-focus-instance-timeout.md` | Foreground forcing, single instance, timeout | done 2026-11-08 |
| 07 | `07-message-rendering.md` | Direct2D and DirectWrite message | done 2026-11-08 (live DPI-change check outstanding) |
| 08 | `08-keyboard-hook.md` | Hook thread, key state, `ToUnicodeEx`, Alt+F4 in hook mode | done 2026-11-08 (+ `VK_PACKET`) |
| 09 | `09-mouse-hook.md` | `WH_MOUSE_LL` | done 2026-11-08 (one live failure not reproduced) |
| 10 | `10-yield-on-ctrl-alt-del.md` | SAS detection spike and yield | done 2026-11-08 (+ hooks pass input on while locked) |
| 11 | `11-icon.md` | SpectraWrite-style glyph icon, owner pick, `xtask icon`, embed | done 2026-11-08 (U+EE3F LockscreenDesktop) |
| 12 | `12-ci-release.md` | GitHub Actions CI and tagged release | done 2026-11-08 (first remote run outstanding) |
| 13 | `13-readme.md` | README with usage, options, recovery, limits | done 2026-11-08 |
| 14 | `14-verification.md` | Fresh build, size and start-time measures, live checks, review | done 2026-11-08 (Lite parity outstanding) |

All stages land in `P:\Source\Rust\OverlayLock`.

## Parallelisation

Blocks run strictly in order. Stages inside a block are order-independent.

1. **01**
2. **03 + 04 + 11 + 12.** 03 and 04 each add one `mod` line to `src/main.rs`. 11 edits only
   `assets/`, `xtask/` and the icon line of `build.rs`. 12 edits only `.github/`. If fanned out,
   give each lane its own git worktree, then merge the `mod` lines.
3. **02** (needs `monitors::Monitors` from 04)
4. **05**
5. **06**
6. **07**
7. **08**
8. **09**
9. **10**
10. **13**
11. **14**

05 to 10 are one lane: each edits `src/overlay/` and `src/main.rs`. 08, 09 and 10 all edit
`src/hooks/`. Never overlap any two of 05 to 10. 11 needs the owner to pick a design, and no stage
before 14 needs the icon. If the owner does not answer, file the pick with `gambit_file_decision`,
carry on with block 3 and later, and finish 11 when the answer arrives, before 14.

## Reference

- `P:\Source\DotNet\OverlayLockLite\OverlayLockLite\Main.cs`: behaviour being ported.
- `P:\Source\DotNet\OverlayLockLite\OverlayLockLite\Main.Designer.cs`: window properties.
- `P:\Source\DotNet\OverlayLock\Forms\Overlay.vb`: older app, reference only.
- `windows` 0.62 crate docs: `https://microsoft.github.io/windows-docs-rs/`.
- Low-level hooks: `LowLevelKeyboardProc`, `LowLevelMouseProc`, `ToUnicodeEx` on Microsoft Learn.
