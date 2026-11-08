# UAC prompt and the Ctrl+Alt+Del yield

> **Status: complete, 2026-11-08.** `--yield-on-ctrl-alt-del` no longer yields after a UAC
> prompt. Stage 01 found the signal (`consent.exe` against `LogonUI.exe` at the switch away), and
> stage 02 applied it, with unit tests and two live checks on Windows 11 (build 26100). Windows 10
> 1809 is unchecked: the owner waived that part of UY1 (amendment below). Carved out of the rewrite
> plan (`plans/_complete/overlay-lock-rewrite-plan/`, stage 10).

## What this adds

Before this plan, `SasDetector` (`src/secure_desktop.rs`) treated every visit to the secure desktop with no
session lock or unlock as a return from Ctrl+Alt+Del. Windows shows UAC prompts on the same
secure desktop (`Winlogon`), so a UAC prompt raised by a background program during a lock also
made the lock step back, and the README said so. The error was fail-open (OL17): the lock gets
easier to leave, never harder.

## Locked decisions

- **UY1.** OL15 and the stage 10 rule stay as they are unless the spike proves a signal that
  separates the two cases on Windows 10 1809 and Windows 11. If no signal works, close this plan
  as "not possible" and keep the README sentence.
  - *Amendment, 2026-11-08, owner-approved:* the Windows 10 1809 proof is waived. No Windows 10
    machine was available, and the rule fails open (stage 01, "Fail-open"): if Windows 10 differs,
    the worst case is the old behaviour, a yield after a UAC prompt. The auto-review at medium
    flagged the gap, and the owner chose the waiver.
- **UY2.** No polling while idle: the locked app's idle CPU time stays at 0 ms (stage 14 measure).
- **UY3.** Spike code goes in `%TEMP%`, never in the repository.

## Stage map

| № | File | Gist | Status |
|---|---|---|---|
| 01 | `01-spike.md` | Log what a UAC prompt gives, beside Ctrl+Alt+Del | done 2026-11-08 (Windows 11 only) |
| 02 | `02-apply-signal.md` | Apply the proven signal to `SasDetector`, with unit tests and a live check | done 2026-11-08 (Windows 10 waived) |

## Reference

- `plans/_complete/overlay-lock-rewrite-plan/10-yield-on-ctrl-alt-del.md`: the spike record
  for Ctrl+Alt+Del and Win+L, and the current rule.
- `src/secure_desktop.rs`: `SasDetector`, `SecureDesktopWatch`.
