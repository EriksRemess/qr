//! Native Node.js bindings for the QR encoder and renderers.
//!
//! The JavaScript boundary is intentionally narrow: construct an immutable
//! renderer once, then request SVG strings or PNG buffers. Filesystem, HTTP,
//! caching, and response handling remain application concerns.

pub mod assets;
pub mod core;
pub mod render;

use assets::{PngLogo, SvgLogo};
use core::{EcLevel, encode_text};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use render::{Color, DEFAULT_SIZE, Style, render_png, render_svg, validate_size};

#[napi(object)]
pub struct RendererOptions {
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub margin: Option<f64>,
    pub error_correction: Option<String>,
    pub logo_svg: Option<String>,
    pub logo_png: Option<Buffer>,
    pub logo_scale: Option<f64>,
    pub logo_padding: Option<f64>,
    pub logo_background: Option<String>,
    pub svg_logo_outline_color: Option<String>,
    pub svg_logo_outline_width: Option<f64>,
}

#[napi(object)]
pub struct OutputOptions {
    pub size: Option<f64>,
}

#[napi]
pub struct QrRenderer {
    style: Style,
    svg_logo: Option<SvgLogo>,
    png_logo: Option<PngLogo>,
}

#[napi]
impl QrRenderer {
    #[napi(constructor)]
    pub fn new(options: Option<RendererOptions>) -> napi::Result<Self> {
        let options = options.unwrap_or(RendererOptions {
            background: None,
            foreground: None,
            margin: None,
            error_correction: None,
            logo_svg: None,
            logo_png: None,
            logo_scale: None,
            logo_padding: None,
            logo_background: None,
            svg_logo_outline_color: None,
            svg_logo_outline_width: None,
        });
        let mut style = Style::default();
        if let Some(value) = options.background {
            style.background = Color::parse(&value).map_err(napi_error)?;
        }
        style.logo_background = style.background;
        if let Some(value) = options.foreground {
            style.foreground = Color::parse(&value).map_err(napi_error)?;
        }
        if let Some(value) = options.margin {
            if !value.is_finite() || value.fract() != 0.0 || !(0.0..=32.0).contains(&value) {
                return Err(napi_error(
                    "margin must be an integer between 0 and 32 modules",
                ));
            }
            style.margin = value as u32;
        }
        if let Some(value) = options.error_correction {
            style.error_correction = match value.as_str() {
                "low" => EcLevel::Low,
                "medium" => EcLevel::Medium,
                "quartile" => EcLevel::Quartile,
                "high" => EcLevel::High,
                _ => {
                    return Err(napi_error(
                        "errorCorrection must be low, medium, quartile, or high",
                    ));
                }
            };
        }
        if let Some(value) = options.logo_scale {
            if !value.is_finite() || !(0.05..=0.5).contains(&value) {
                return Err(napi_error("logoScale must be between 0.05 and 0.5"));
            }
            style.logo_scale = value;
        }
        if let Some(value) = options.logo_padding {
            if !value.is_finite() || !(0.0..=4.0).contains(&value) {
                return Err(napi_error("logoPadding must be between 0 and 4 modules"));
            }
            style.logo_padding = value;
        }
        if let Some(value) = options.logo_background {
            style.logo_background = Color::parse(&value).map_err(napi_error)?;
        }
        if let Some(value) = options.svg_logo_outline_color {
            style.svg_logo_outline = Some(Color::parse(&value).map_err(napi_error)?);
        }
        if let Some(value) = options.svg_logo_outline_width {
            if !value.is_finite() || !(0.0..=128.0).contains(&value) {
                return Err(napi_error(
                    "svgLogoOutlineWidth must be between 0 and 128 logo units",
                ));
            }
            style.svg_logo_outline_width = value;
        }
        let mut svg_logo = options
            .logo_svg
            .as_deref()
            .map(SvgLogo::parse)
            .transpose()
            .map_err(napi_error)?;
        if let (Some(logo), Some(color)) = (&mut svg_logo, style.svg_logo_outline) {
            logo.set_outline(color, style.svg_logo_outline_width)
                .map_err(napi_error)?;
        }
        let png_logo = options
            .logo_png
            .as_deref()
            .map(PngLogo::decode)
            .transpose()
            .map_err(napi_error)?;
        Ok(Self {
            style,
            svg_logo,
            png_logo,
        })
    }

    /// Encode and render synchronously. SVG generation is short CPU work and
    /// avoids Promise/microtask overhead by design.
    #[napi]
    pub fn svg(&self, text: String, options: Option<OutputOptions>) -> napi::Result<String> {
        let size = output_size(options)?;
        let symbol = encode_text(&text, self.style.error_correction).map_err(napi_error)?;
        Ok(render_svg(
            &symbol,
            &self.style,
            size,
            self.svg_logo.as_ref(),
        ))
    }

    /// Encode, rasterize, and compress synchronously into a Node.js Buffer.
    #[napi]
    pub fn png(&self, text: String, options: Option<OutputOptions>) -> napi::Result<Buffer> {
        let size = output_size(options)?;
        let symbol = encode_text(&text, self.style.error_correction).map_err(napi_error)?;
        render_png(&symbol, &self.style, size, self.png_logo.as_ref())
            .map(Buffer::from)
            .map_err(napi_error)
    }
}

fn output_size(options: Option<OutputOptions>) -> napi::Result<u32> {
    let value = options
        .and_then(|options| options.size)
        .unwrap_or(f64::from(DEFAULT_SIZE));
    if !value.is_finite()
        || value.fract() != 0.0
        || !(21.0..=f64::from(render::MAX_SIZE)).contains(&value)
    {
        return Err(napi_error(format!(
            "size must be between 21 and {} pixels and must be an integer; received {value}",
            render::MAX_SIZE
        )));
    }
    let size = value as u32;
    validate_size(size).map_err(napi_error)?;
    Ok(size)
}

fn napi_error(error: impl ToString) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}
