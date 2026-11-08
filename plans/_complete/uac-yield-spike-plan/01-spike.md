# 01 Spike: what a UAC prompt gives

> **Status: done 2026-11-08.** Signal chosen: at the switch away from the user's desktop,
> `consent.exe` is running and `LogonUI.exe` is not for a UAC prompt; `LogonUI.exe` is running and
> `consent.exe` is not for Ctrl+Alt+Del. One process snapshot per switch away, so no idle polling
> (UY2). Proven on Windows 11 (build 26100) only; no Windows 10 1809 machine was available (see
> "Windows 10" below).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: none. Locked: UY1, UY3.

## Steps

1. Extend the stage 10 logger (rebuild it in `%TEMP%`: out-of-context
   `EVENT_SYSTEM_DESKTOPSWITCH` hook, session notifications, `OpenInputDesktop` name) to also log,
   on each switch: the foreground window's process before and after, `consent.exe` present or
   not (process snapshot), and any `EVENT_SYSTEM_FOREGROUND` events in between.
2. With the owner at the desktop, record: Ctrl+Alt+Del then Cancel; Ctrl+Alt+Del then Task
   Manager; a UAC prompt (for example `Start-Process cmd -Verb RunAs`) answered Yes; one answered
   No. Ask with the question tool, and say plainly that no lock appears during the spike.
3. Look for a signal present for UAC and absent for Ctrl+Alt+Del (the strongest candidate is
   `consent.exe` running during the visit). Check it does not need polling while idle (UY2): a
   snapshot taken only when a visit ends is acceptable.
4. Write the sequences and the chosen signal (or "none") in this file, then write stage 02.

## Spike record (2026-11-08, Windows 11 26100)

Logger in `%TEMP%\overlay-lock-uac-spike`: the stage 10 hooks plus `EVENT_SYSTEM_FOREGROUND`, and
a 100 ms Toolhelp process snapshot that logged every process start and exit and the presence of
`consent.exe`, `LogonUI.exe`, `CredentialUIBroker.exe`, `LockApp.exe` and `Taskmgr.exe`. "away"
means `OpenInputDesktop` failed with `0x80070005` (the secure desktop). UAC policy on this
machine: `EnableLUA` 1, `ConsentPromptBehaviorAdmin` 5, `PromptOnSecureDesktop` 1 (the defaults).
The UAC prompts were raised with `Start-Process cmd -Verb RunAs` from the agent's shell.

| Time (s) | Event | Owner action |
|---|---|---|
| 10.967 | `LogonUI.exe` starts | Ctrl+Alt+Del |
| 11.116 | switch away; snapshot: `LogonUI.exe`, no `consent.exe`; no foreground window | |
| 13.940 | switch back; foreground: Gambit | Cancel |
| 14.029 | `LogonUI.exe` exits | |
| 18.076 | `LogonUI.exe` starts | Ctrl+Alt+Del |
| 18.329 | switch away; snapshot: `LogonUI.exe`, no `consent.exe` | |
| 19.691 | switch back; `LogonUI.exe` still running | Task Manager |
| 19.826 | `LogonUI.exe` exits, `LaunchTM.exe` runs | |
| 20.042 | `consent.exe` runs for about 100 ms, with **no** desktop switch | (Task Manager auto-elevates) |
| 20.121 | foreground: `Taskmgr.exe` | |
| 119.041 | `consent.exe` starts | UAC prompt |
| 119.146 | switch away; snapshot: `consent.exe`, no `LogonUI.exe`; no foreground window | |
| 120.135 | switch back; `consent.exe` still running | Yes |
| 120.217 | foreground: the elevated `cmd.exe` | |
| 133.922 | `consent.exe` starts | UAC prompt |
| 134.058 | switch away; snapshot: `consent.exe`, no `LogonUI.exe` | |
| 135.718 | switch back; `consent.exe` still running; the launch failed with "canceled by the user" | No |

No session message arrived in any of the four visits, as stage 10 found for Ctrl+Alt+Del.
Each prompt process started 100 ms to 250 ms before the switch away and was still running at the
switch back. The foreground events give nothing useful: there is no foreground window on the
secure desktop in either case.

## Findings

- **The signal.** The process that owns the secure desktop tells the two apart: `LogonUI.exe` for
  Ctrl+Alt+Del, `consent.exe` for a UAC prompt. Both are already running when the switch away
  arrives, so one snapshot at that moment is enough. Nothing runs while idle (UY2).
- **`consent.exe` alone is not enough.** It also runs, briefly and with no secure desktop, when
  an auto-elevating program starts (Task Manager at 20.042 s). A snapshot taken only at a switch
  away cannot see that run, and the rule also requires `LogonUI.exe` to be absent.
- **Fail-open (OL17).** The rule must yield whenever it is not sure: a visit is a UAC prompt only
  if `consent.exe` is running and `LogonUI.exe` is not, at every switch away in the visit. A failed
  snapshot counts as "not sure". A process named `consent.exe` started by anyone else cannot stop
  the yield, because a real Ctrl+Alt+Del always has `LogonUI.exe` running.
- **Windows 10.** Not measured. `consent.exe` (UAC) and `LogonUI.exe` (Ctrl+Alt+Del and the
  sign-in screen) are the same programs on every Windows version since Vista, and the fail-open
  rule means a difference there can only bring back today's behaviour (a yield after a UAC
  prompt), never block a yield. UY1 asks for Windows 10 1809 too; stage 02 records this as an
  open check rather than a proven one.

## Done when

- [x] The four sequences are recorded here.
- [x] A signal is chosen, or the plan is closed as not possible under UY1.
