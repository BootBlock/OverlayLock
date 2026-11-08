# 05 Overlay windows and the controller

> **Status: done 2026-11-08.** Differences: per-window state is a `Box<OverlayWindow>` holding the
> `Rc<App>` (set up and dropped by `overlay::dispatch`), so 07 could add the per-window text; the
> controller is destroyed by `main` after the loop, never inside its own handler. Live check 6
> found FluidBars (another topmost app) above the overlay after a resolution change, so after each
> display change the overlays are raised to the top of the topmost band, every 250 ms for 10 s
> (`RAISE_REPEATS` = 40, `App::after_display_change`). It was 2 s at first; in stage 14 an unplug
> left FluidBars on top (it rebuilds its bars more than 2 s after the change), and with 10 s the
> rerun covered it. Stage 14 also found that unplugging the monitor whose overlay had the focus
> lost the focus; the leaving overlay now hands the focus to another overlay just before it is
> destroyed (`App::hand_over_focus`), which takes nothing from another app (OL10). With
> `--monitors cursor` no other overlay exists at that moment; the owner amended OL10 so the
> replacement overlay takes the focus back in that one case. Checked live on 2026-11-08: the
> cursor on the top monitor, `--monitors cursor --timeout 60s`, that monitor unplugged; the new
> overlay on the main monitor took the focus (a traced build logged `WM_SETFOCUS` about 0.4 s after
> the change, and stayed on top until the timeout in two runs), and typing the password right
> after the unplug ended the lock (exit 0 after 8 s).
> The tests used `--timeout 20s` from the start (06 landed before any live run).

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 02, 03, 04 done. Locked: OL7, OL8 (GDI half),
OL9, OL13 (focus-mode half), OL17, OL18, OL19.

After this stage `overlay-lock --password go` works as Lite does, on every monitor, without the
focus retry, timeout or message (06, 07).

## Touch points

| File | Change |
|---|---|
| `src/app.rs` | New. `App`: options, `PasswordMatcher`, overlay set, `kept` cursor monitor, `unlock(cause)`. |
| `src/overlay/mod.rs` | New. `OverlaySet`: applies `monitors::Change`s, `hwnds()`, `destroy_all()`. |
| `src/overlay/window.rs` | New. Overlay class `OverlayLock.Overlay`, window procedure, per-window state. |
| `src/overlay/controller.rs` | New. Message-only window, class `OverlayLock.Controller`, `WM_APP_UNLOCK`. |
| `src/main.rs` | Build `App`, create windows, run the message loop, return `ExitStatus`. |
| `Cargo.toml` | `windows` features as needed (`Win32_UI_Input_KeyboardAndMouse`). |
| `tests/unlock.rs` | New process tests (below). |

## Design

- **Strings.** Use `windows::core::w!` for constant wide strings and `HSTRING` for runtime ones.
  Do not write a string helper module.
- **Classes.** Register both classes once with `RegisterClassExW`. Overlay: `CS_HREDRAW |
  CS_VREDRAW`, `hCursor` null, `hbrBackground` a `CreateSolidBrush(options.background)` kept for the
  process life. Controller: no brush. Use `GetModuleHandleW(None)` for the instance.
- **Overlay creation.** `CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, class, title
  "OverlayLock", WS_POPUP, rect..., parent None)`, then `ShowWindow(SW_SHOWNOACTIVATE)` for all but
  the cursor monitor's window, which gets `SW_SHOW`. The window title is fixed (tests find it by
  class, never by title).
- **Window procedure** (overlay):
  - `WM_SETCURSOR` → `SetCursor(None)`, return `TRUE` (OL9).
  - `WM_ERASEBKGND` → default (the class brush paints). `WM_PAINT` → `BeginPaint`/`EndPaint` only.
    Stage 07 adds text.
  - `WM_CHAR` → each UTF-16 unit to `App::matcher.push`. On `true`, `App::unlock(Password)`.
  - `WM_SYSCOMMAND` with `SC_CLOSE` and `WM_CLOSE` → `App::unlock(AltF4)` when
    `options.alt_f4_unlocks`, else return 0 (OL13).
  - `WM_DISPLAYCHANGE` → re-enumerate, `select`, `reconcile`, apply. Every overlay gets this
    broadcast, so handle it only once per change: the controller does the work, overlays forward
    with `PostMessageW(controller, WM_APP_DISPLAY_CHANGED)` and the controller coalesces duplicates
    (ignore while a reconcile is pending).
  - `WM_DPICHANGED` → keep the window at its monitor rectangle (ignore the suggested rect).
  - `WM_QUERYENDSESSION` → `TRUE`. `WM_ENDSESSION` with `wParam` true → `App::unlock(Session)`.
  - `WM_MOUSEACTIVATE` → `MA_ACTIVATE`. Clicks are otherwise ignored.
- **Controller.** `CreateWindowExW` with parent `HWND_MESSAGE`. Handles `WM_APP_UNLOCK`
  (`WM_APP + 1`, cause in `wParam`), `WM_APP_DISPLAY_CHANGED` (`WM_APP + 2`) and, from 06, timers.
  `App::unlock` posts `WM_APP_UNLOCK` rather than tearing down inside the caller's message, so the
  teardown never runs re-entrantly inside an overlay's window procedure.
- **Teardown.** On `WM_APP_UNLOCK`: destroy every overlay, destroy the controller, `PostQuitMessage(0)`.
  `main` leaves the loop and returns `ExitStatus::Unlocked`. Only the first unlock counts.
- **State access.** `App` lives in an `Rc<App>` created in `main`. Each window stores a raw pointer
  from `Rc::into_raw` in `GWLP_USERDATA` at `WM_NCCREATE` (from `CREATESTRUCTW::lpCreateParams`) and
  releases it at `WM_NCDESTROY`. Mutable parts of `App` are `RefCell`/`Cell`; follow the OL18 borrow
  rule. A message that arrives before `WM_NCCREATE` or after `WM_NCDESTROY` goes to `DefWindowProcW`.
- **Errors.** A failed `RegisterClassExW` or `CreateWindowExW` returns `windows::core::Error` up to
  `main`, which reports it (OL4) and returns `ExitStatus::Failure`.

## Done when

- [x] `tests/unlock.rs`, run with `--monitors cursor --timeout 20s` once 06 exists (until then
      without `--timeout`, and the test kills the child after 20 s): spawn the exe, wait up to 5 s
      for a top-level window of class `OverlayLock.Overlay` owned by the child PID
      (`EnumWindows` + `GetWindowThreadProcessId`), `PostMessageW(WM_CHAR)` each unit of `xgo`, and
      assert exit code 0 within 2 s. A second test sends `gx` and asserts the child is still alive.
      A third posts `WM_SYSCOMMAND SC_CLOSE` and asserts exit 0. A fourth does the same with
      `--ignore-alt-f4` and asserts the child is still alive, then unlocks with the password.
- [x] Live, by hand, with `--timeout` absent and a second terminal ready: every monitor goes black
      including the taskbar, the cursor is hidden over all of them, typing the password unlocks,
      Alt+F4 unlocks, and the cursor is back.
- [x] Change the display resolution or disconnect a monitor while locked: every remaining monitor
      stays covered at its new size.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.

Conflict note: 05 to 10 are one lane through `src/overlay/` and `src/main.rs`.
