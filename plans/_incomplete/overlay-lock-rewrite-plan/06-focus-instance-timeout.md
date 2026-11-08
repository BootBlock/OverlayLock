# 06 Focus, single instance and timeout

> **Status: done 2026-11-08.** The focus timer starts only when the first attempt did not leave an
> overlay in the foreground. Live: Firefox had the focus at launch, and typing `go` unlocked in 2.1 s.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 05 done. Locked: OL10, OL14, OL16.

## Touch points

| File | Change |
|---|---|
| `src/focus.rs` | New. `force_foreground(hwnd)`, `is_ours_foreground(&[HWND])`. |
| `src/instance.rs` | New. `InstanceGuard::acquire() -> Result<Option<InstanceGuard>>`. |
| `src/overlay/controller.rs` | Timers `TIMER_FOCUS` (250 ms, repeating) and `TIMER_TIMEOUT` (one-shot). |
| `src/main.rs` | Take the guard after parsing; exit `AlreadyLocked` when `None`. |
| `src/app.rs` | Focus attempt counter, unlock cause `Timeout`. |
| `Cargo.toml` | `windows` feature `Win32_System_Threading`, `Win32_Security`. |
| `tests/unlock.rs` | Tests below. |

## Design

- `force_foreground` ports `Main.cs:90-110`: read `GetWindowThreadProcessId(GetForegroundWindow())`
  and `GetCurrentThreadId()`. If they differ, `AttachThreadInput(fg, own, true)`, then
  `BringWindowToTop`, `SetForegroundWindow`, `ShowWindow(SW_SHOW)`, then detach. A null foreground
  window (thread 0) skips the attach. Return whether an overlay is foreground afterwards.
- The controller calls `force_foreground` on the cursor monitor's overlay at once, then on each
  `TIMER_FOCUS` tick until an overlay is foreground or 8 ticks passed; then it kills the timer and
  never starts it again (OL10).
- `InstanceGuard` wraps `CreateMutexW(None, true, name)`. `ERROR_ALREADY_EXISTS` gives `Ok(None)`.
  It closes the handle in `Drop`. The guard lives until `main` returns.
- `TIMER_TIMEOUT`: `SetTimer` takes `u32` milliseconds. 7 days is 604,800,000 ms and fits. On the
  tick, `App::unlock(Timeout)`.

## Done when

- [x] Test: `--timeout 2s` alone exits 0 in 2 to 4 s.
- [x] Test: start one instance, start a second with the same options, the second exits 3 within
      1 s and the first is still alive. Then unlock the first.
- [x] Test: `tests/unlock.rs` from 05 now passes `--timeout 20s` as the safety net.
- [x] Live: start the exe from a Stream Deck button or with
      `Start-Process` from a script while another window has the focus (`powershell -c "Start-Sleep 3;
      & target\release\overlay-lock.exe -p go --timeout 60s"`, then click another window): typing
      `go` still unlocks, which shows the overlay took the focus.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.
