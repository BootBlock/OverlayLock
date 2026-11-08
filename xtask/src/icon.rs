//! `cargo xtask icon`: renders one icon-font glyph in SpectraWrite's style and writes
//! `assets/overlay-lock.ico`, plus its 256 px frame as `assets/overlay-lock-256.png` for the
//! README.
//!
//! The style matches `SpectraWrite/scripts/generate-spectrawrite-icon.ps1`: the glyph outline is
//! scaled so its larger bound is 80 % of the frame, centred, and filled with a diagonal eight-stop
//! rainbow gradient on a transparent background. Each frame size is drawn from the outline, never
//! scaled from another frame.

use std::fs;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_GRADIENT_STOP, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1CreateFactory, ID2D1Factory, ID2D1Geometry,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory, IDWriteFontFace,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
    GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapCacheOnDemand,
    WICBitmapDitherTypeNone, WICBitmapEncoderNoCache, WICBitmapPaletteTypeCustom,
};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STATFLAG_NONAME,
    STATSTG, STREAM_SEEK_SET,
};
use windows::core::{HSTRING, Interface};
use windows_numerics::{Matrix3x2, Vector2};

/// The glyph the owner picked on 2026-11-08: Segoe Fluent Icons `LockscreenDesktop` (U+EE3F).
const DEFAULT_GLYPH: u32 = 0xEE3F;

/// The icon fonts to try, in order.
const FONT_FAMILIES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];

/// The frame sizes written to the ICO file, in ascending order.
const ICO_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

/// The frame sizes written by `--preview`.
const PREVIEW_SIZES: [u32; 3] = [256, 32, 16];

/// The share of the frame the glyph's larger bound fills.
const FIT: f32 = 0.80;

/// SpectraWrite's gradient stops: position and `0xRRGGBB` colour.
const GRADIENT: [(f32, u32); 8] = [
    (0.00, 0xFF_00_00),
    (0.14, 0xFF_8C_00),
    (0.28, 0xFF_D7_00),
    (0.42, 0x00_C8_53),
    (0.56, 0x00_BC_D4),
    (0.70, 0x21_96_F3),
    (0.84, 0x9C_27_B0),
    (1.00, 0xFF_00_00),
];

const USAGE: &str = "Usage: cargo xtask icon [--glyph <U+XXXX>] [--preview <dir>]\n\n\
  --glyph <U+XXXX>  the code point to draw (default: U+EE3F, LockscreenDesktop)\n\
  --preview <dir>   write the 256, 32 and 16 px PNG frames to <dir> instead of the ICO";

/// Runs the `icon` command with the arguments that follow it.
pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let mut glyph = DEFAULT_GLYPH;
    let mut preview: Option<PathBuf> = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--glyph" => {
                let value = args.next().ok_or("--glyph needs a value")?;
                glyph = parse_code_point(&value)?;
            }
            "--preview" => {
                preview = Some(args.next().ok_or("--preview needs a folder")?.into());
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument '{other}'\n\n{USAGE}")),
        }
    }

    let renderer = Renderer::new(glyph).map_err(|error| error.to_string())?;
    if let Some(folder) = preview {
        fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;
        for size in PREVIEW_SIZES {
            let png = renderer
                .render_png(size)
                .map_err(|error| error.to_string())?;
            let path = folder.join(format!("U+{glyph:04X}-{size}.png"));
            write(&path, &png)?;
        }
        println!("Wrote previews of U+{glyph:04X} to {}", folder.display());
    } else {
        let frames = ICO_SIZES
            .iter()
            .map(|&size| renderer.render_png(size).map(|png| (size, png)))
            .collect::<windows::core::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("the xtask folder has no parent")?
            .join("assets");
        let icon = assets.join("overlay-lock.ico");
        write(&icon, &ico(&frames)?)?;
        println!("Wrote {}", icon.display());
        // The README shows the largest frame, because GitHub does not render ICO files.
        if let Some((_, png)) = frames.last() {
            let preview = assets.join("overlay-lock-256.png");
            write(&preview, png)?;
            println!("Wrote {}", preview.display());
        }
    }
    Ok(())
}

/// Parses `U+E72E`, `E72E` or `0xE72E`.
fn parse_code_point(text: &str) -> Result<u32, String> {
    let digits = text
        .strip_prefix("U+")
        .or_else(|| text.strip_prefix("u+"))
        .or_else(|| text.strip_prefix("0x"))
        .unwrap_or(text);
    u32::from_str_radix(digits, 16)
        .ok()
        .filter(|&point| char::from_u32(point).is_some())
        .ok_or_else(|| format!("--glyph: '{text}' is not a code point such as U+E72E"))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// Draws one glyph at any frame size.
struct Renderer {
    d2d: ID2D1Factory,
    wic: IWICImagingFactory,
    /// The glyph outline at an em size of 1, in DIPs, with its origin on the baseline.
    outline: ID2D1Geometry,
    /// The outline's bounds.
    bounds: D2D_RECT_F,
}

impl Renderer {
    fn new(glyph: u32) -> windows::core::Result<Self> {
        // SAFETY: COM is initialised once for this thread before any WIC call, and the process
        // never uninitialises it while a COM object is alive.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        // SAFETY: plain factory constructors with valid arguments.
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        // SAFETY: as above.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        // SAFETY: as above; WIC is an in-process COM server and COM is initialised.
        let wic: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }?;

        let face = icon_font_face(&dwrite)?;
        let mut index = 0u16;
        // SAFETY: one code point in, one glyph index out, both pointers valid for one element.
        unsafe { face.GetGlyphIndices(&raw const glyph, 1, &raw mut index) }?;
        if index == 0 {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("the icon font has no glyph for U+{glyph:04X}"),
            ));
        }

        // SAFETY: the factory is valid; the sink is opened, filled and closed in order.
        let path = unsafe { d2d.CreatePathGeometry() }?;
        // SAFETY: as above.
        let sink = unsafe { path.Open() }?;
        // SAFETY: `index` is one valid glyph index; the sink receives the outline at em size 1.
        unsafe {
            face.GetGlyphRunOutline(1.0, &raw const index, None, None, 1, false, false, &sink)
        }?;
        // SAFETY: closes the sink opened above.
        unsafe { sink.Close() }?;
        // SAFETY: the geometry is closed and valid.
        let bounds = unsafe { path.GetBounds(None) }?;
        if bounds.right - bounds.left <= 0.0 || bounds.bottom - bounds.top <= 0.0 {
            return Err(windows::core::Error::new(
                windows::Win32::Foundation::E_INVALIDARG,
                format!("U+{glyph:04X} has an empty outline"),
            ));
        }
        Ok(Self {
            d2d,
            wic,
            outline: path.cast()?,
            bounds,
        })
    }

    /// Renders one square frame and encodes it as a 32-bit RGBA PNG.
    fn render_png(&self, size: u32) -> windows::core::Result<Vec<u8>> {
        #[allow(clippy::cast_precision_loss, reason = "frame sizes are at most 256")]
        let extent = size as f32;
        let width = self.bounds.right - self.bounds.left;
        let height = self.bounds.bottom - self.bounds.top;
        let scale = extent * FIT / width.max(height);
        let dx = (extent - width * scale) / 2.0 - self.bounds.left * scale;
        let dy = (extent - height * scale) / 2.0 - self.bounds.top * scale;
        let transform = Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: dx,
            M32: dy,
        };

        // SAFETY: valid size and pixel format GUID.
        let bitmap = unsafe {
            self.wic.CreateBitmap(
                size,
                size,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnDemand,
            )
        }?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        // SAFETY: the bitmap and properties are valid for the call.
        let target = unsafe {
            self.d2d
                .CreateWicBitmapRenderTarget(&bitmap, &raw const properties)
        }?;
        // SAFETY: the geometry and the matrix are valid for the call.
        let glyph = unsafe {
            self.d2d
                .CreateTransformedGeometry(&self.outline, &raw const transform)
        }?;

        let stops = GRADIENT.map(|(position, rgb)| D2D1_GRADIENT_STOP {
            position,
            color: colour(rgb),
        });
        // SAFETY: the target is valid and the stops slice outlives the call.
        let stops = unsafe {
            target.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
        }?;
        let line = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
            startPoint: Vector2 { X: 0.0, Y: 0.0 },
            endPoint: Vector2 {
                X: extent,
                Y: extent,
            },
        };
        // SAFETY: the properties and the stop collection are valid for the call.
        let brush = unsafe { target.CreateLinearGradientBrush(&raw const line, None, &stops) }?;

        // SAFETY: one BeginDraw/EndDraw pair on a valid target with valid resources.
        unsafe {
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            target.BeginDraw();
            target.Clear(Some(&raw const TRANSPARENT));
            target.FillGeometry(&glyph, &brush, None);
            target.EndDraw(None, None)?;
        }
        self.encode_png(&bitmap, size)
    }

    /// Encodes a premultiplied bitmap as a straight-alpha 32-bit PNG.
    fn encode_png(
        &self,
        bitmap: &windows::Win32::Graphics::Imaging::IWICBitmap,
        size: u32,
    ) -> windows::core::Result<Vec<u8>> {
        // SAFETY: a null HGLOBAL asks for a new growable stream, freed with the stream.
        let stream = unsafe { CreateStreamOnHGlobal(HGLOBAL::default(), true) }?;
        // SAFETY: valid container GUID, no preferred vendor.
        let encoder = unsafe {
            self.wic
                .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
        }?;
        // SAFETY: the stream is valid and writable.
        unsafe { encoder.Initialize(&stream, WICBitmapEncoderNoCache) }?;
        let mut frame = None;
        // SAFETY: both out pointers are valid; the property bag is not needed.
        unsafe { encoder.CreateNewFrame(&raw mut frame, std::ptr::null_mut()) }?;
        let frame = frame.ok_or_else(windows::core::Error::empty)?;
        // SAFETY: the converter turns the premultiplied pixels into the straight alpha PNG
        // stores; every interface is valid and each call follows the WIC encode sequence.
        unsafe {
            let converter = self.wic.CreateFormatConverter()?;
            converter.Initialize(
                bitmap,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )?;
            frame.Initialize(None)?;
            frame.SetSize(size, size)?;
            let mut format = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&raw mut format)?;
            frame.WriteSource(&converter, std::ptr::null())?;
            frame.Commit()?;
            encoder.Commit()?;
        }
        let mut stat = STATSTG::default();
        // SAFETY: `stat` is writable; NONAME means no name string to free.
        unsafe { stream.Stat(&raw mut stat, STATFLAG_NONAME) }?;
        let len = usize::try_from(stat.cbSize).map_err(|_| windows::core::Error::empty())?;
        let mut bytes = vec![0u8; len];
        let mut read = 0u32;
        // SAFETY: the buffer holds `len` bytes, which fits in u32 for an icon frame.
        unsafe {
            stream.Seek(0, STREAM_SEEK_SET, None)?;
            stream
                .Read(
                    bytes.as_mut_ptr().cast(),
                    u32::try_from(len).map_err(|_| windows::core::Error::empty())?,
                    Some(&raw mut read),
                )
                .ok()?;
        }
        bytes.truncate(read as usize);
        Ok(bytes)
    }
}

static TRANSPARENT: D2D1_COLOR_F = D2D1_COLOR_F {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// Converts `0xRRGGBB` to an opaque Direct2D colour.
fn colour(rgb: u32) -> D2D1_COLOR_F {
    let channel = |shift: u32| f32::from(u8::try_from((rgb >> shift) & 0xFF).unwrap_or(0)) / 255.0;
    D2D1_COLOR_F {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.0,
    }
}

/// Finds the first installed icon font and returns its regular face.
fn icon_font_face(dwrite: &IDWriteFactory) -> windows::core::Result<IDWriteFontFace> {
    let mut collection = None;
    // SAFETY: the out pointer is valid.
    unsafe { dwrite.GetSystemFontCollection(&raw mut collection, false) }?;
    let collection = collection.ok_or_else(windows::core::Error::empty)?;
    for name in FONT_FAMILIES {
        let mut index = 0u32;
        let mut exists = windows::core::BOOL::default();
        // SAFETY: the name is a valid wide string and both out pointers are valid.
        unsafe {
            collection.FindFamilyName(&HSTRING::from(name), &raw mut index, &raw mut exists)
        }?;
        if exists.as_bool() {
            // SAFETY: `index` came from `FindFamilyName` on this collection.
            let font = unsafe {
                collection.GetFontFamily(index)?.GetFirstMatchingFont(
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                )
            }?;
            // SAFETY: the font is valid.
            return unsafe { font.CreateFontFace() };
        }
    }
    Err(windows::core::Error::new(
        windows::Win32::Foundation::E_FAIL,
        format!(
            "none of these fonts is installed: {}",
            FONT_FAMILIES.join(", ")
        ),
    ))
}

/// Writes an ICO file whose frames are PNG data, in the order given.
fn ico(frames: &[(u32, Vec<u8>)]) -> Result<Vec<u8>, String> {
    const HEADER: usize = 6;
    const ENTRY: usize = 16;
    let count = u16::try_from(frames.len()).map_err(|_| "too many frames")?;
    let mut offset = HEADER + ENTRY * frames.len();
    let mut out =
        Vec::with_capacity(offset + frames.iter().map(|(_, png)| png.len()).sum::<usize>());
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&count.to_le_bytes());
    for (size, png) in frames {
        // A width or height of 0 means 256.
        let side = u8::try_from(*size).unwrap_or(0);
        out.extend_from_slice(&[side, side, 0, 0]); // width, height, colour count, reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        let len = u32::try_from(png.len()).map_err(|_| "frame too large")?;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(offset)
                .map_err(|_| "file too large")?
                .to_le_bytes(),
        );
        offset += png.len();
    }
    for (_, png) in frames {
        out.extend_from_slice(png);
    }
    Ok(out)
}
