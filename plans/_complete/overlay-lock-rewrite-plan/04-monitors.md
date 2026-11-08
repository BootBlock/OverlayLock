# 04 Monitors

> **Status: done 2026-11-08.** Clippy is clean apart from `dead_code`, which clears when 05 uses the module.
> `monitor_at_cursor` falls back to the primary monitor when `GetCursorPos` fails. Added an order test.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 done. Locked: OL7. This module owns the
`Monitors { All, Cursor }` enum. Stage 02 parses `--monitors` into it.

## Touch points

| File | Change |
|---|---|
| `src/monitors.rs` | New. `Monitors`, `Monitor`, `MonitorId`, `enumerate`, `select`, `reconcile`. |
| `src/main.rs` | `mod monitors;` only. |
| `Cargo.toml` | `windows` feature `Win32_Graphics_Gdi`. |

Conflict note: 03 also adds `mod` lines to `src/main.rs` in this block.

## Design

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)] pub enum Monitors { #[default] All, Cursor }
#[derive(Clone, PartialEq, Eq, Hash)] pub struct MonitorId(Box<[u16]>); // szDevice, trimmed at NUL
#[derive(Clone, Copy, PartialEq, Eq)] pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }
pub struct Monitor { pub id: MonitorId, pub rect: Rect, pub primary: bool }

pub fn enumerate() -> windows::core::Result<Vec<Monitor>>;          // EnumDisplayMonitors + GetMonitorInfoW(MONITORINFOEXW)
pub fn monitor_at_cursor() -> windows::core::Result<MonitorId>;     // GetCursorPos + MonitorFromPoint(MONITOR_DEFAULTTOPRIMARY)
pub fn select<'a>(all: &'a [Monitor], mode: Monitors, kept: Option<&MonitorId>) -> Vec<&'a Monitor>;
pub fn reconcile(current: &[(MonitorId, Rect)], wanted: &[&Monitor]) -> Vec<Change>;
pub enum Change { Create(Monitor), Move { id: MonitorId, rect: Rect }, Destroy(MonitorId) }
```

- `rect` is `rcMonitor` (the full monitor, as Lite used `Screen.Bounds`), never `rcWork`.
- `select` with `Monitors::All` returns every monitor. With `Monitors::Cursor` it returns the `kept`
  monitor if it still exists, else the primary one. The caller keeps the returned ID as the new
  `kept` (OL7).
- `reconcile` is pure: it emits `Move` only when the rectangle changed, and keeps a stable order
  (destroys first, then moves, then creates) so the overlay set in 05 applies it in one pass.
- The `EnumDisplayMonitors` callback pushes into a `Vec` passed through `LPARAM`. Mark the callback
  `extern "system"` and keep its `unsafe` block with a `// SAFETY:` comment.

## Done when

- [x] Unit tests for `select` (all; cursor kept; cursor gone falls back to primary) and `reconcile`
      (no change; one moved; one added; one removed; all replaced) with hand-built monitors.
- [x] A unit test calls `enumerate()` and checks at least one monitor and exactly one primary.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.
