# 07 Message rendering

> **Status: done 2026-11-08, except the live scale-change check.** Changing a monitor's scale on
> this machine asks for a sign-out before it applies, so `WM_DPICHANGED` could not be triggered
> live; that check is outstanding. Live, the owner saw two centred, sharp lines on both monitors,
> and Comic Sans MS. `--font NoSuchFont` exited 0 with empty stderr. The window keeps its
> `WindowText` in a `Cell` and takes it out while drawing, so no borrow spans a Direct2D call. The
> app depends on `windows-numerics` directly for `Vector2` (already in the tree via `windows`).
> Record capture: `target/icon-candidates/message-crop.png`.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 06 done. Locked: OL8.

## Touch points

| File | Change |
|---|---|
| `src/render.rs` | New. `TextRenderer` (shared factories, text format) and `WindowText` (per-window target and layout). |
| `src/overlay/window.rs` | `WM_PAINT`, `WM_SIZE`, `WM_DPICHANGED` call `WindowText` when a message is set. |
| `src/app.rs` | `Option<TextRenderer>`, built only when `options.message` is `Some`. |
| `Cargo.toml` | `windows` features `Win32_Graphics_Direct2D`, `Win32_Graphics_Direct2D_Common`, `Win32_Graphics_DirectWrite`, `Win32_Graphics_Dxgi_Common`, `Win32_UI_HiDpi`, `Win32_System_Com`. |

## Design

- `TextRenderer::new(style)`: `D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED)`,
  `DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)`. Resolve the family: the `--font` value, or
  `Segoe UI Variable Display` if `GetSystemFontCollection` + `FindFamilyName` finds it, else
  `Segoe UI`. A `--font` that is not installed is not an error: DirectWrite falls back, and the
  README says so. Make one `IDWriteTextFormat` at `size_points × 96 / 72` DIPs, centred both ways
  (`DWRITE_TEXT_ALIGNMENT_CENTER`, `DWRITE_PARAGRAPH_ALIGNMENT_CENTER`), word wrap on.
- `WindowText` per overlay, made lazily on first `WM_PAINT`: `CreateHwndRenderTarget` with
  `D2D1_PRESENT_OPTIONS_NONE`, pixel size from `GetClientRect`, then `SetDpi(dpi, dpi)` with
  `GetDpiForWindow`. Layout width is 80 % of the client width in DIPs, height the full client
  height in DIPs, origin centred. One solid brush for the text colour.
- Paint: `BeginDraw`, `Clear(background)`, `DrawTextLayout`, `EndDraw`. On `D2DERR_RECREATE_TARGET`
  drop the target and brush and `InvalidateRect`. Call `ValidateRect` through `BeginPaint`/`EndPaint`
  so `WM_PAINT` does not repeat. With a message, the class brush still paints first, so there is no
  white flash before Direct2D.
- `WM_SIZE` → `target.Resize` and rebuild the layout. `WM_DPICHANGED` → `SetDpi` and rebuild.
- No COM apartment is needed for these factories; do not call `CoInitializeEx`.
- Without `--message`, `render.rs` code never runs. Check that `d2d1.dll` is still loaded at start
  (the import table loads it) and measure the cost in stage 14 before anyone adds delay-loading.

## Done when

- [x] Live: `--message "Locked\nBack soon" --text-size 48 --text-colour gold --colour "#102030"`
      shows two centred lines on every monitor, sharp at each monitor's scale. Capture one monitor
      for the record.
- [ ] Live: change one monitor's scale in Settings while locked: the text on it keeps its size in
      points and stays sharp, and the other monitors do not change.
- [x] Live: `--font "Comic Sans MS"` changes the font. `--font "NoSuchFont"` falls back with no
      error.
- [x] A unit test builds `App` from options without `--message` and asserts it holds no
      `TextRenderer`, and one with `--message` asserts it holds one.
- [x] Clippy and fmt clean. Status line flipped, stage table updated.
