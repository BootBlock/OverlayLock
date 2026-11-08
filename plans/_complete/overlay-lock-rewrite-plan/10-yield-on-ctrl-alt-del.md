# 10 Yield on Ctrl+Alt+Del

> **Status: done 2026-11-08.** The spike proved `SetWinEventHook(EVENT_SYSTEM_DESKTOPSWITCH)` with
> `WTSRegisterSessionNotification` (record below); polling was not needed. Differences: (1) the
> decision waits 500 ms after the return (`SETTLE_MS`), because `WTS_SESSION_UNLOCK` comes only
> 31 ms before the last switch back; the logic is a pure `SasDetector` with unit tests. (2) The
> yield lowers each overlay just below the foreground window (Task Manager) after
> `HWND_NOTOPMOST`, because `HWND_NOTOPMOST` alone puts the overlay at the top of the normal band,
> above a Task Manager that opened first. (3) Live check 11 found that the hooks swallowed the
> lock screen's input (the lock screen runs on the same desktop) and that Ctrl and Alt stayed
> "down" after Ctrl+Alt+Del (their key-ups go to the secure desktop). So the watch now runs
> whenever hooks run: the hooks pass input on between `WTS_SESSION_LOCK` and
> `WTS_SESSION_UNLOCK`, and reload the held keys (`GetAsyncKeyState`) on each return and unlock.
> (4) The controller handles the yield directly on the settle timer; no `WM_APP_YIELD` was needed.
> (5) Live runs used `--timeout 60s`, not 120 s (the seed caps every run at 60 s). (6) A UAC prompt
> on the secure desktop also yielded; `plans/_complete/uac-yield-spike-plan/` fixed that on
> 2026-11-08 (`consent.exe` against `LogonUI.exe` at the switch away).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 09 done. Locked: OL15.

This stage starts with a spike. The signal must be proven on this machine before the yield is built.

## Touch points

| File | Change |
|---|---|
| `src/secure_desktop.rs` | New. `SecureDesktopWatch`: reports "returned from the Ctrl+Alt+Del screen" to the controller. |
| `src/overlay/controller.rs` | `WM_APP_YIELD` (`WM_APP + 3`): call `OverlaySet::yield_topmost` and stop the hook thread. |
| `src/overlay/mod.rs` | `yield_topmost()`: `SetWindowPos(HWND_NOTOPMOST, ..., SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)` for each overlay; new overlays from a later reconcile are made without `WS_EX_TOPMOST`. |
| `src/hooks/mod.rs` | `stop` usable before teardown; the matcher returns to `App` for `WM_CHAR`, cleared. |
| `src/app.rs` | `yielded: Cell<bool>`; start the watch when `yield_on_ctrl_alt_del`. |
| `Cargo.toml` | `windows` features `Win32_UI_Accessibility`, `Win32_System_RemoteDesktop`, `Win32_System_StationsAndDesktops`. |

## Spike (do first, record the result in this file)

1. Write a throwaway binary under `%TEMP%` (not in the repo) that logs, with timestamps, every
   `EVENT_SYSTEM_DESKTOPSWITCH` from `SetWinEventHook(..., WINEVENT_OUTOFCONTEXT)` and every
   `WM_WTSSESSION_CHANGE` from `WTSRegisterSessionNotification(NOTIFY_FOR_THIS_SESSION)`.
2. Press Ctrl+Alt+Del, then Cancel. Press Ctrl+Alt+Del, then Task Manager. Press Win+L, then sign
   back in. Record the event sequence for each. The owner must do the key presses: ask with the
   question tool when they are at the desktop.
3. Expected: Ctrl+Alt+Del gives two desktop switches and no session change; Win+L gives
   `WTS_SESSION_LOCK` and `WTS_SESSION_UNLOCK` around its switches. Then the rule is: a desktop
   switch back to the default desktop that is not inside a lock/unlock pair means "returned from
   Ctrl+Alt+Del". Confirm the order of the lock message against the switch events.
4. If the events do not separate the cases, try a 250 ms poll of `OpenInputDesktop` +
   `GetUserObjectInformationW(UOI_NAME)` (`Winlogon` while the secure screen shows) combined with
   the session messages. If neither works, stop and report: OL15 is then impossible as written.

## Spike record (2026-11-08, Windows 11 26100)

Throwaway logger in `%TEMP%\overlay-lock-sas-spike` (out-of-context WinEvent hook, a hidden
top-level window registered for session notifications, and a 100 ms `OpenInputDesktop` poll).
"away" means `OpenInputDesktop` failed with `0x80070005` (the secure desktop); "back" means it
read `Default`. The first two attempts recorded nothing: the owner was waiting for a lock that
the spike never shows. The third:

| Time (s) | Event | Owner action |
|---|---|---|
| 44.580 | DESKTOPSWITCH, away | Ctrl+Alt+Del |
| 46.136 | DESKTOPSWITCH, back | Cancel |
| 50.514 | DESKTOPSWITCH, away | Ctrl+Alt+Del |
| 51.742 | DESKTOPSWITCH, back | Task Manager |
| 57.159 | DESKTOPSWITCH, away | Win+L |
| 57.170 | `WTS_SESSION_LOCK` | |
| 57.649 | DESKTOPSWITCH, back (the lock screen runs on `Default`) | |
| 58.659 | DESKTOPSWITCH, away | sign-in prompt |
| 62.042 | `WTS_SESSION_UNLOCK` | signed in |
| 62.073 | DESKTOPSWITCH, back | |

Ctrl+Alt+Del gives one away/back pair and no session message. Win+L gives two pairs with the lock
message 11 ms after the first switch away and the unlock message 31 ms before the last switch
back. Rule: judge each visit (away to back) as a whole, 500 ms after the return; a visit with no
lock or unlock message, while not locked, is a return from Ctrl+Alt+Del.

## Live record without `--yield-on-ctrl-alt-del`

With `--block-keys --block-mouse` and no yield flag, Ctrl+Alt+Del then Task Manager: Task Manager
stayed hidden behind the overlay and got no keys or clicks (UIPI aside, the topmost overlay covers
it and the hooks swallow its input). The lock ended only by its timeout. This feeds the README.

## Design

- `SecureDesktopWatch` runs on the main thread (WinEvent out-of-context callbacks arrive through
  the main message loop). The session notification goes to the controller window. Use whichever
  mechanism the spike proved; keep the other out.
- On "returned from Ctrl+Alt+Del": post `WM_APP_YIELD`. The yield is one-way until unlock. After
  the yield the password works through `WM_CHAR` on a focused overlay, and the timeout still works.
- The focus retry (06) does not restart on yield.

## Done when

- [x] The spike's event sequences are written in this file.
- [x] Live with `--block-keys --block-mouse --yield-on-ctrl-alt-del --timeout 120s`: Ctrl+Alt+Del →
      Task Manager → Task Manager is above the overlay and takes keys and clicks; ending
      `overlay-lock.exe` there unlocks.
- [x] Live with the same flags: Win+L then sign in → the overlay is still topmost and the hooks still
      swallow input.
- [x] Live without `--yield-on-ctrl-alt-del`: Ctrl+Alt+Del → Task Manager → record what happens
      (whether Task Manager gets input, given UIPI). This record feeds the README in 13.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.

Conflict note: 08, 09 and 10 all edit `src/hooks/`. Never overlap them.
