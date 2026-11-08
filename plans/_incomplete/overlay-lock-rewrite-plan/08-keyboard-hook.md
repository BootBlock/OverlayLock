# 08 Keyboard hook

> **Status: done 2026-11-08.** Differences: (1) the hooks install *before* the overlays show,
> because the first hook test found keys typed between the show and the install were lost (the
> matcher had already moved to the hook thread); the focus step uses no input, so it still works
> (checked: the overlay held the focus with each flag). (2) Live check 8 found that a Stream Deck
> text action types `KEYEVENTF_UNICODE` packets (`VK_PACKET`, unit in the scan code), which
> `ToUnicodeEx` does not translate; the translator now feeds the packet's unit, and
> `unicode_packets_unlock` covers it (it fails without the fix). (3) The "Notepad" test uses a
> test-owned edit window, so it never touches the owner's Notepad tabs. (4) Num Lock is seeded and
> toggled as well as Caps Lock; Alt without Ctrl types nothing (as `WM_SYSCHAR` in focus mode).
> No `Win32_UI_TextServices` feature was needed. Live: Win, Alt+Tab, Ctrl+Esc, Ctrl+Shift+Esc and
> Alt+F4 (`--ignore-alt-f4`) did nothing, Ctrl+Alt+Del opened, `go` unlocked; the Stream Deck
> button unlocked after the fix.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 07 done. Locked: OL11, OL13 (hook half), OL17.

## Touch points

| File | Change |
|---|---|
| `src/hooks/mod.rs` | New. `HookThread::start(config, controller) -> Result<HookThread>`, `stop(self)`. |
| `src/hooks/keyboard.rs` | New. `KeyboardHook` proc and its thread-local state. |
| `src/hooks/translate.rs` | New. `KeyTranslator`: own key state, `ToUnicodeEx`, Alt+F4 detection. |
| `src/app.rs` | Start the hook thread when `block_keys`; stop it in teardown. |
| `src/overlay/window.rs` | `WM_CHAR` reads `App::matcher`, now `RefCell<Option<PasswordMatcher>>`. In hook mode it is `None` and the character is ignored. |
| `Cargo.toml` | `windows` feature `Win32_UI_TextServices` if `ToUnicodeEx` needs it. |
| `tests/hooks.rs` | New, `#[ignore]` tests (below). |

## Design

- **Thread.** `HookThread::start` spawns a `std::thread` named `overlay-lock-hooks`. It raises its
  priority with `SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST)`, builds its
  thread-local state, calls `SetWindowsHookExW(WH_KEYBOARD_LL, proc, GetModuleHandleW(None), 0)`,
  reports success or the error back through a `std::sync::mpsc` channel, and runs
  `GetMessageW` until `WM_QUIT`. `start` waits on the channel, so a failed install is a reported
  start-up failure (OL4, exit 1), never a silent unlocked overlay. `stop` posts `WM_QUIT` with
  `PostThreadMessageW` and joins. Unhook before the thread ends.
- **State.** The proc has no user pointer. Keep `thread_local!` `RefCell<Option<KeyboardState>>`
  holding `KeyTranslator`, the `PasswordMatcher` (taken out of `App::matcher` at start, OL6), the
  controller `HWND` and `alt_f4_unlocks`. The proc does no allocation and no blocking call. Its
  only cross-thread call is `PostMessageW(controller, WM_APP_UNLOCK, cause)`.
- **Proc.** For `nCode == HC_ACTION`: read `KBDLLHOOKSTRUCT`. Update the key state for every down
  and up (`WM_KEYDOWN`, `WM_SYSKEYDOWN`, `WM_KEYUP`, `WM_SYSKEYUP`). On a down, check Alt+F4 first,
  then translate. Return `LRESULT(1)` for every event. For `nCode < 0` call `CallNextHookEx`.
- **Translator.** `[u8; 256]` key state. Seed it at install from `GetAsyncKeyState` for every
  virtual key (high bit → `0x80`) and `GetKeyState(VK_CAPITAL) & 1` for the Caps Lock toggle. Keep
  `VK_SHIFT`, `VK_CONTROL`, `VK_MENU` in step with their left and right keys, as `ToUnicodeEx`
  reads the generic ones. Toggle Caps Lock on its own key-down. Translate with
  `ToUnicodeEx(vk, scan, &state, &mut buf[..8], 0x4, layout)`, where `layout` is
  `GetKeyboardLayout(0)` read at install. A result `> 0` feeds each unit to the matcher. A result
  `< 0` (dead key) and `0` feed nothing.
- **AltGr** arrives as Ctrl+Alt with an injected left Ctrl. Keep it in the state as Windows sends
  it; `ToUnicodeEx` then gives the AltGr character.
- **Alt+F4.** F4 down while `VK_MENU` is down in the translator's state gives
  `WM_APP_UNLOCK(AltF4)` when `alt_f4_unlocks`, else nothing. Either way the key is swallowed.
- Win+L and Ctrl+Alt+Del are not seen by the hook (overview grounding). The README says so.

## Done when

- [x] Unit tests for `KeyTranslator` with the US layout (`LoadKeyboardLayoutW("00000409",
      KLF_NOTELLSHELL)` in the test only): `a` → `a`; Shift+`a` → `A`; Caps Lock then `a` → `A`;
      Caps Lock + Shift + `a` → `a`; `1` with Shift → `!`; Alt+F4 detected; F4 alone not.
- [x] `tests/hooks.rs` (`#[ignore]`, `--block-keys --monitors cursor --timeout 20s`): `SendInput`
      types `xgo` (as virtual keys, not `KEYEVENTF_UNICODE`) and the child exits 0. A second test
      gives Notepad the focus first and checks Notepad receives no characters while the child
      unlocks. A third sends Alt+F4 and the child exits 0, and with `--ignore-alt-f4` stays alive.
- [x] Live with `--timeout 60s`: Win, Alt+Tab, Ctrl+Esc, Ctrl+Shift+Esc and Alt+F4 (with
      `--ignore-alt-f4`) do nothing; Ctrl+Alt+Del still opens its screen; typing the password unlocks.
- [x] Live: a blocked-keys lock started from a Stream Deck text action that types the password
      unlocks (injected input counts). Mark outstanding if no Stream Deck is at hand.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.

Conflict note: 08, 09 and 10 all edit `src/hooks/`. Never overlap them.
