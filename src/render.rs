//! Draws the centred `--message` with Direct2D and DirectWrite (OL8).
//!
//! Without `--message` nothing here runs: the overlay class brush paints the background alone.

use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{D2D_SIZE_U, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::{BOOL, HSTRING, w};
use windows_numerics::Vector2;

use crate::cli::MessageStyle;
use crate::colour::Colour;

/// The share of the client width the text wraps at.
const WRAP_WIDTH: f32 = 0.8;

/// The pixels per inch at which one DIP is one pixel.
const BASE_DPI: f32 = 96.0;

/// The family used when `--font` is absent, if it is installed.
const PREFERRED_FAMILY: &str = "Segoe UI Variable Display";

/// The family used when `--font` is absent and the preferred one is not installed.
const FALLBACK_FAMILY: &str = "Segoe UI";

/// The factories and the text format shared by every overlay.
pub struct TextRenderer {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    format: IDWriteTextFormat,
    text: Vec<u16>,
    background: D2D1_COLOR_F,
    foreground: D2D1_COLOR_F,
}

impl TextRenderer {
    /// Initialises a new instance of the [`TextRenderer`] type for a message on a background.
    pub fn new(style: &MessageStyle, background: Colour) -> windows::core::Result<Self> {
        // SAFETY: plain factory constructors. These factories need no COM apartment.
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        // SAFETY: as above.
        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let family = match &style.font {
            Some(font) => HSTRING::from(font.as_str()),
            None if family_exists(&dwrite, PREFERRED_FAMILY)? => HSTRING::from(PREFERRED_FAMILY),
            None => HSTRING::from(FALLBACK_FAMILY),
        };
        // A family that is not installed is not an error: DirectWrite falls back by itself.
        // SAFETY: the strings are valid for the call; `None` is the system font collection.
        let format = unsafe {
            dwrite.CreateTextFormat(
                &family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size_points * BASE_DPI / 72.0,
                w!(""),
            )
        }?;
        // SAFETY: plain setters on the format made above.
        unsafe {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;
        }
        Ok(Self {
            d2d,
            dwrite,
            format,
            text: style.text.encode_utf16().collect(),
            background: d2d_colour(background),
            foreground: d2d_colour(style.colour),
        })
    }

    /// Paints the message on `hwnd`, making its target on first use.
    ///
    /// `text` is the window's own [`WindowText`] slot. When the device is lost the slot is emptied,
    /// and the caller invalidates the window so the next paint makes a new target.
    pub fn paint(&self, hwnd: HWND, text: &mut Option<WindowText>) -> windows::core::Result<()> {
        if text.is_none() {
            *text = Some(self.window_text(hwnd)?);
        }
        let Some(window) = text.as_ref() else {
            return Ok(());
        };
        let origin = Vector2 {
            X: window.origin_x,
            Y: 0.0,
        };
        // SAFETY: one BeginDraw/EndDraw pair on this window's own target and resources.
        let drawn = unsafe {
            window.target.BeginDraw();
            window.target.Clear(Some(&raw const self.background));
            window.target.DrawTextLayout(
                origin,
                &window.layout,
                &window.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            window.target.EndDraw(None, None)
        };
        match drawn {
            Err(error) if error.code() == D2DERR_RECREATE_TARGET => {
                *text = None;
                Err(error)
            }
            other => other,
        }
    }

    /// Fits the target and the layout to the window's new size or DPI.
    pub fn resize(&self, hwnd: HWND, text: &mut Option<WindowText>) -> windows::core::Result<()> {
        let Some(window) = text.as_mut() else {
            return Ok(());
        };
        let (pixels, dpi) = client_size(hwnd)?;
        // SAFETY: plain calls on this window's own target.
        unsafe {
            window.target.Resize(&raw const pixels)?;
            window.target.SetDpi(dpi, dpi);
        }
        let (layout, origin_x) = self.layout(pixels, dpi)?;
        window.layout = layout;
        window.origin_x = origin_x;
        Ok(())
    }

    fn window_text(&self, hwnd: HWND) -> windows::core::Result<WindowText> {
        let (pixels, dpi) = client_size(hwnd)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES::default();
        let hwnd_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: pixels,
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: both property structures are valid for the call; `hwnd` is a live overlay.
        let target = unsafe {
            self.d2d
                .CreateHwndRenderTarget(&raw const properties, &raw const hwnd_properties)
        }?;
        // SAFETY: plain calls on the target made above.
        let brush = unsafe {
            target.SetDpi(dpi, dpi);
            target.CreateSolidColorBrush(&raw const self.foreground, None)
        }?;
        let (layout, origin_x) = self.layout(pixels, dpi)?;
        Ok(WindowText {
            target,
            brush,
            layout,
            origin_x,
        })
    }

    /// Lays the text out at 80 % of the client width and the full client height, in DIPs, and
    /// returns the layout with the x coordinate that centres it.
    fn layout(
        &self,
        pixels: D2D_SIZE_U,
        dpi: f32,
    ) -> windows::core::Result<(IDWriteTextLayout, f32)> {
        #[allow(
            clippy::cast_precision_loss,
            reason = "monitor sizes are far below 2^24"
        )]
        let (width, height) = (
            pixels.width as f32 * BASE_DPI / dpi,
            pixels.height as f32 * BASE_DPI / dpi,
        );
        let wrap = width * WRAP_WIDTH;
        // SAFETY: the text and the format are valid for the call.
        let layout = unsafe {
            self.dwrite
                .CreateTextLayout(&self.text, &self.format, wrap, height)
        }?;
        Ok((layout, (width - wrap) / 2.0))
    }
}

/// The Direct2D target, brush and text layout of one overlay window.
pub struct WindowText {
    target: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    layout: IDWriteTextLayout,
    origin_x: f32,
}

/// Gets a window's client size in pixels and its DPI.
fn client_size(hwnd: HWND) -> windows::core::Result<(D2D_SIZE_U, f32)> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is writable and `hwnd` is a live overlay.
    unsafe { GetClientRect(hwnd, &raw mut rect) }?;
    // SAFETY: as above.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let size = D2D_SIZE_U {
        width: u32::try_from(rect.right - rect.left).unwrap_or(0),
        height: u32::try_from(rect.bottom - rect.top).unwrap_or(0),
    };
    #[allow(clippy::cast_precision_loss, reason = "a DPI is a small integer")]
    let dpi = if dpi == 0 { BASE_DPI } else { dpi as f32 };
    Ok((size, dpi))
}

fn family_exists(dwrite: &IDWriteFactory, family: &str) -> windows::core::Result<bool> {
    let mut collection = None;
    // SAFETY: the out pointer is valid.
    unsafe { dwrite.GetSystemFontCollection(&raw mut collection, false) }?;
    let collection = collection.ok_or_else(windows::core::Error::empty)?;
    let mut index = 0u32;
    let mut exists = BOOL::default();
    // SAFETY: the name is a valid wide string and both out pointers are valid.
    unsafe { collection.FindFamilyName(&HSTRING::from(family), &raw mut index, &raw mut exists) }?;
    Ok(exists.as_bool())
}

fn d2d_colour(colour: Colour) -> D2D1_COLOR_F {
    let [r, g, b] = colour.to_unit_rgb();
    D2D1_COLOR_F { r, g, b, a: 1.0 }
}
