# 02 Apply the signal to the Ctrl+Alt+Del detector

> **Status: done 2026-11-08 (Windows 10 waived, UY1 amendment).** As planned, with one addition: the watch
> takes the snapshot only when it was started for `--yield-on-ctrl-alt-del`
> (`SecureDesktopWatch::start(controller, find_elevation_prompts)`). Six new unit tests (13 in the
> module, 77 in the crate); mutations proved them: dropping the prompt check from the decision
> failed 3, replacing `&=` with `=` failed 1, dropping the `LogonUI.exe` check failed 1. Gates
> pass on Rust 1.98.1 and 1.99.0. Live (debug build, owner at the desktop, `--block-keys
> --block-mouse --yield-on-ctrl-alt-del --timeout 60s`): a UAC prompt raised 10 s into the lock
> and answered No left the lock in place (the Windows key did nothing, the screen stayed black);
> Ctrl+Alt+Del then Task Manager in the same lock showed Task Manager above the overlay, and
> ending `overlay-lock.exe` there ended the lock (exit 1 after 26 s).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01. Locked: UY1, UY2, OL15, OL17.

## Touch points

- `src/secure_desktop.rs`: the signal, the detector, the snapshot, the module docs.
- `src/overlay/controller.rs`: builds the signal from `WM_APP_DESKTOP_SWITCH`.
- `src/app.rs`: starts the watch and passes signals on.
- `Cargo.toml`: the `windows` feature `Win32_System_Diagnostics_ToolHelp` (a feature of an
  existing dependency, no new crate).
- `README.md`: the sentence about UAC prompts.

## Steps

1. Split `Signal::DesktopSwitch { ours }` into `Signal::DesktopLeft { elevation_prompt }` and
   `Signal::DesktopReturned`, so the "away" signal is the only one that carries the prompt flag.
2. Add a pure classifier over process names: `true` only if `consent.exe` is present and
   `LogonUI.exe` is absent (ASCII case-insensitive). Unit-test it.
3. In the WinEvent callback, take one Toolhelp process snapshot on a switch away, only when the
   watch was started for `--yield-on-ctrl-alt-del` (the hooks alone do not need it), and pass the
   flag in `lParam` of `WM_APP_DESKTOP_SWITCH`. A failed snapshot gives `false` (fail-open).
4. In `SasDetector`, a visit is a UAC prompt only if every switch away in it had the flag. Such a
   visit never yields. Unit-test: a UAC visit does not yield; a Ctrl+Alt+Del visit still yields; a
   visit that leaves again during the settle time with a mixed flag yields; a UAC visit does not
   stop a later Ctrl+Alt+Del visit from yielding.
5. Update the module docs and the README sentence.
6. Live, with the owner, each run with `--block-keys --block-mouse --yield-on-ctrl-alt-del
   --timeout 60s`: (a) a UAC prompt raised from the agent's shell during the lock, answered No: the
   lock must not step back (Task Manager from the taskbar is not reachable, the overlay stays on
   top); (b) then Ctrl+Alt+Del then Task Manager in the same lock: it must step back.

## Done when

- [x] All four gates pass, and the new tests fail on the old code.
- [x] Both live checks pass.
- [x] README and module docs say a UAC prompt no longer makes the lock step back, and that a lock
      yields whenever it cannot tell.
- [x] Windows 10 1809 recorded as unchecked (no machine), with the fail-open reason.
