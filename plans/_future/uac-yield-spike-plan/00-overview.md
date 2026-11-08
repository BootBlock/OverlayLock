# UAC prompt and the Ctrl+Alt+Del yield

> **Status: deferred, 2026-11-08.** Adds a way to tell a UAC prompt on the secure desktop apart
> from the Ctrl+Alt+Del screen, so `--yield-on-ctrl-alt-del` yields only after Ctrl+Alt+Del. It
> waits because the rewrite plan (`plans/_incomplete/overlay-lock-rewrite-plan/`, stage 10)
> shipped the yield first; the owner chose "Investigate" on 2026-11-08. Revive it when the owner
> asks for it, or before the first tagged release if a UAC prompt during a lock is a real case.

## What this adds

Today `SasDetector` (`src/secure_desktop.rs`) treats every visit to the secure desktop with no
session lock or unlock as a return from Ctrl+Alt+Del. Windows shows UAC prompts on the same
secure desktop (`Winlogon`), so a UAC prompt raised by a background program during a lock also
makes the lock step back. The README states this. The error is fail-open (OL17): the lock gets
easier to leave, never harder.

## Locked decisions

- **UY1.** OL15 and the stage 10 rule stay as they are unless the spike proves a signal that
  separates the two cases on Windows 10 1809 and Windows 11. If no signal works, close this plan
  as "not possible" and keep the README sentence.
- **UY2.** No polling while idle: the locked app's idle CPU time stays at 0 ms (stage 14 measure).
- **UY3.** Spike code goes in `%TEMP%`, never in the repository.

## Stage map

| № | File | Gist | Status |
|---|---|---|---|
| 01 | `01-spike.md` | Log what a UAC prompt gives, beside Ctrl+Alt+Del | not started |
| 02 | (written after 01) | Apply the proven signal to `SasDetector`, with unit tests and a live check | not started |

## Reference

- `plans/_incomplete/overlay-lock-rewrite-plan/10-yield-on-ctrl-alt-del.md`: the spike record
  for Ctrl+Alt+Del and Win+L, and the current rule.
- `src/secure_desktop.rs`: `SasDetector`, `SecureDesktopWatch`.
