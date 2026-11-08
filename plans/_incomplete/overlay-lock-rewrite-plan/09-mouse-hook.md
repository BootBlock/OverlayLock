# 09 Mouse hook

> **Status: done 2026-11-08, with one unexplained live failure.** In the first live run with
> `--block-mouse` the owner moved the mouse, clicked and scrolled, and typed `go` did not unlock
> (it timed out at 60 s). It did not reproduce: on the rerun `go` unlocked at once with the
> overlay in the foreground from 0.4 s, and `typed_password_unlocks_after_swallowed_clicks`
> (moves, clicks and wheel, then typed keys) passes. Stage 14 repeats this case. The mouse hook is
> installed with the keyboard hook before the overlays show (see 08).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 08 done. Locked: OL12.

## Touch points

| File | Change |
|---|---|
| `src/hooks/mouse.rs` | New. `WH_MOUSE_LL` proc that returns `LRESULT(1)` for every `HC_ACTION`. |
| `src/hooks/mod.rs` | `HookConfig { keyboard: Option<KeyboardConfig>, mouse: bool }`. Install what is set on the one thread. |
| `src/app.rs` | Start the hook thread when either flag is set. |
| `tests/hooks.rs` | Tests below. |

## Design

- One hook thread serves both hooks (OL12). `HookThread::start` installs the keyboard hook, then the
  mouse hook, and reports the first failure. A config with neither is a programming error:
  `App` does not call `start` then.
- The mouse proc holds no state and reads nothing beyond `nCode`.
- With `--block-mouse` and without `--block-keys`, the password still arrives through `WM_CHAR` on
  the focused overlay (05). The focus is taken before the hook installs, so the hook never blocks
  the focus step.

## Done when

- [x] `tests/hooks.rs` (`#[ignore]`): with `--block-mouse --timeout 20s`, `SendInput` moves the
      mouse by 200 px and `GetCursorPos` shows the cursor did not move; then `PostMessageW(WM_CHAR)`
      unlocks and the cursor moves again after exit.
- [x] Live: with `--block-mouse`, clicks, wheel and movement do nothing on every monitor, and the
      password unlocks. With both flags, both kinds of input are swallowed.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.

Conflict note: 08, 09 and 10 all edit `src/hooks/`. Never overlap them.
