# 11 Icon

> **Status: done 2026-11-08.** The owner picked `LockscreenDesktop` (U+EE3F) with
> `gambit_pick_variant` from Lock (E72E), LockscreenDesktop (EE3F), ShieldLock (F5B4) and
> DeviceMonitorNoPic (E7FB); no notes. Differences: `xtask` also depends on `windows-numerics`
> (already in the tree through `windows`, which does not re-export `Matrix3x2`/`Vector2`); the
> fit uses Direct2D's tight outline bounds, where GDI+ `GetBounds` may include curve control
> points; the gradient interpolates with `D2D1_GAMMA_2_2`, as GDI+ blends in sRGB. Explorer's view
> was checked by extracting the exe's associated icon (`Icon.ExtractAssociatedIcon`), not by eye.

Repo: `P:\Source\Rust\OverlayLock`. Prerequisites: 01 done. Locked: OL2 (xtask uses only
`windows`), OL21. Needs the owner to pick a glyph.

The icon matches SpectraWrite's. Read
`P:\Source\DotNet\SpectraWrite\scripts\generate-spectrawrite-icon.ps1` and look at
`P:\Source\DotNet\SpectraWrite\src\SpectraWrite.UI\Assets\SpectraWrite.ico` before you start.

## Touch points

| File | Change |
|---|---|
| `xtask/Cargo.toml` | `windows` with the DirectWrite, Direct2D, WIC (`Win32_Graphics_Imaging`) and COM features. |
| `xtask/src/main.rs` | Add the `icon` command. |
| `xtask/src/icon.rs` | New. Glyph outline, gradient fill, PNG frames through WIC, ICO writer. |
| `assets/overlay-lock.ico` | Generated, committed. |
| `build.rs` | `res.set_icon("assets/overlay-lock.ico")` and `rerun-if-changed`. |

Conflict note: only stage to touch `assets/overlay-lock.ico` and `xtask/`. Its `build.rs` edit is
one line.

## The SpectraWrite style, exactly

- **Glyph:** one character from `Segoe Fluent Icons`, else `Segoe MDL2 Assets` if the first is not
  installed. Fail with a clear error if neither is present.
- **Background:** transparent. 32-bit RGBA PNG frames, no plate and no shadow.
- **Fit:** take the glyph's outline geometry, scale it so its larger bound is 80 % of the frame,
  and centre its bounds in the frame. Do this per frame size, as the script does, not by scaling one
  bitmap down.
- **Fill:** a linear gradient from `(0, 0)` to `(size, size)` with these stops:

  | Position | Colour |
  |---|---|
  | 0.00 | `#FF0000` |
  | 0.14 | `#FF8C00` |
  | 0.28 | `#FFD700` |
  | 0.42 | `#00C853` |
  | 0.56 | `#00BCD4` |
  | 0.70 | `#2196F3` |
  | 0.84 | `#9C27B0` |
  | 1.00 | `#FF0000` |

- **Quality:** anti-aliased (`D2D1_ANTIALIAS_MODE_PER_PRIMITIVE`), no ClearType.
- **ICO file:** an `ICONDIR` with one 16-byte entry per frame (width and height `0` for 256),
  32 bpp, PNG data, frames in ascending size: 16, 20, 24, 32, 40, 48, 64, 128, 256.

## Design

- Outline: `IDWriteFontFace::GetGlyphRunOutline` into an `ID2D1PathGeometry`, then `GetBounds`, a
  scale-and-translate `Matrix3x2`, and `FillGeometry` with an `ID2D1LinearGradientBrush` on a
  WIC-bitmap render target (`CreateWicBitmapRenderTarget`, `GUID_WICPixelFormat32bppPBGRA`).
  Encode each frame with the WIC PNG encoder. Write the ICO by hand (the format above is the whole
  of it). `CoInitializeEx(COINIT_MULTITHREADED)` once for WIC.
- Command: `cargo xtask icon [--glyph <U+XXXX>] [--preview <dir>]`. `--preview` writes the 256, 32
  and 16 px PNGs to a folder instead of the ICO. The default glyph is the one the owner picked,
  written as a named constant in `icon.rs` with a comment naming its Fluent Icons name.
- Candidates: find lock and screen glyphs in the Segoe Fluent Icons code-point list on Microsoft
  Learn (`https://learn.microsoft.com/windows/apps/design/style/segoe-fluent-icons-font`), for
  example Lock, a screen or display glyph and a privacy or shield glyph. Check each one renders
  (non-empty bounds) before you offer it. Render previews to `%TEMP%\overlay-lock-icons\`, not to
  the repository.
- Show four candidates with `gambit_pick_variant` (load it with ToolSearch first), each with its
  256 px and 16 px previews, and apply the owner's notes. If the owner does not answer, file the
  pick with `gambit_file_decision` and continue the plan; finish this stage when the answer arrives.

## Done when

- [x] The owner picked a glyph (record which, and their notes, in this file).
- [x] The 256 px frame, viewed beside SpectraWrite's 256 px frame, has the same fit, margin and
      gradient direction.
- [x] `cargo xtask icon` run twice gives byte-identical `assets/overlay-lock.ico` files.
- [x] Explorer shows the icon on `target\release\overlay-lock.exe` at small and large sizes.
- [x] `cargo tree -p overlay-lock` does not change: `xtask` adds no dependency to the app.
- [x] Clippy and fmt clean for the workspace. Status line flipped, stage table updated.
