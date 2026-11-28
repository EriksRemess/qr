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
use render::{
    Color, DEFAULT_SIZE, ModuleStyle, Style, render_png_with_caption, render_svg, validate_size,
};

#[napi(object)]
pub struct PngLogoPalette {
    pub source_foreground: String,
    pub source_background: String,
    pub foreground: String,
    pub background: String,
}

#[napi(object)]
pub struct RendererOptions {
    pub module_style: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,
    pub margin: Option<f64>,
    pub error_correction: Option<String>,
    pub logo_svg: Option<String>,
    pub logo_png: Option<Buffer>,
    pub png_logo_palette: Option<PngLogoPalette>,
    pub logo_scale: Option<f64>,
    pub logo_padding: Option<f64>,
    pub logo_background: Option<String>,
    pub svg_logo_outline_color: Option<String>,
    pub svg_logo_outline_width: Option<f64>,
}

#[napi(object)]
pub struct OutputOptions {
    pub size: Option<f64>,
    pub caption_lines: Option<Vec<String>>,
}

#[napi]
pub struct QrRenderer {
    style: Style,
    svg_logo: Option<SvgLogo>,
    png_logo: Option<PngLogo>,
}

#[napi]
impl QrRenderer {
    #[napi(constructor, strict)]
    pub fn new(options: Option<RendererOptions>) -> napi::Result<Self> {
        let options = options.unwrap_or(RendererOptions {
            module_style: None,
            background: None,
            foreground: None,
            margin: None,
            error_correction: None,
            logo_svg: None,
            logo_png: None,
            png_logo_palette: None,
            logo_scale: None,
            logo_padding: None,
            logo_background: None,
            svg_logo_outline_color: None,
            svg_logo_outline_width: None,
        });
        let mut style = Style::default();
        if let Some(value) = options.module_style {
            style.module_style = match value.as_str() {
                "square" => ModuleStyle::Square,
                "rounded" => ModuleStyle::Rounded,
                _ => return Err(napi_error("moduleStyle must be square or rounded")),
            };
        }
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
        let mut png_logo = options
            .logo_png
            .as_deref()
            .map(PngLogo::decode)
            .transpose()
            .map_err(napi_error)?;
        if let Some(palette) = options.png_logo_palette {
            let colors = [
                &palette.source_foreground,
                &palette.source_background,
                &palette.foreground,
                &palette.background,
            ]
            .map(|value| Color::parse(value).map_err(napi_error));
            let [source_fg, source_bg, fg, bg] = colors;
            let (source_fg, source_bg, fg, bg) = (source_fg?, source_bg?, fg?, bg?);
            if [source_fg, source_bg, fg, bg]
                .iter()
                .any(|color| color.alpha != 255)
                || source_fg == source_bg
            {
                return Err(napi_error(
                    "pngLogoPalette needs opaque colors and distinct source colors",
                ));
            }
            if let Some(logo) = &mut png_logo {
                logo.recolor(source_fg, source_bg, fg, bg);
            }
        }
        Ok(Self {
            style,
            svg_logo,
            png_logo,
        })
    }

    /// Encode and render synchronously. SVG generation is short CPU work and
    /// avoids Promise/microtask overhead by design.
    #[napi(strict)]
    pub fn svg(&self, text: String, options: Option<OutputOptions>) -> napi::Result<String> {
        let (size, lines) = output_options(options)?;
        let symbol = encode_text(&text, self.style.error_correction).map_err(napi_error)?;
        if lines.is_empty() {
            return Ok(render_svg(
                &symbol,
                &self.style,
                size,
                self.svg_logo.as_ref(),
            ));
        }
        // The outer document paints one continuous background under both areas.
        let mut symbol_style = self.style.clone();
        symbol_style.background.alpha = 0;
        let svg = render_svg(&symbol, &symbol_style, size, self.svg_logo.as_ref());
        Ok(render::caption::Caption::layout(size, &lines).wrap_svg(
            &svg,
            size,
            self.style.foreground,
            self.style.background,
            &lines,
        ))
    }

    /// Encode, rasterize, and compress synchronously into a Node.js Buffer.
    #[napi(strict)]
    pub fn png(&self, text: String, options: Option<OutputOptions>) -> napi::Result<Buffer> {
        let (size, lines) = output_options(options)?;
        let symbol = encode_text(&text, self.style.error_correction).map_err(napi_error)?;
        let caption = (!lines.is_empty()).then(|| render::caption::Caption::layout(size, &lines));
        render_png_with_caption(
            &symbol,
            &self.style,
            size,
            self.png_logo.as_ref(),
            caption.as_ref(),
        )
        .map(Buffer::from)
        .map_err(napi_error)
    }
}

fn output_options(options: Option<OutputOptions>) -> napi::Result<(u32, Vec<String>)> {
    let value = options
        .as_ref()
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
    let lines = options
        .and_then(|options| options.caption_lines)
        .unwrap_or_default();
    if lines.len() > 2
        || lines.iter().any(|line| {
            line.chars().count() > 256
                || line
                    .chars()
                    .any(|ch| ch.is_control() || ch == '\u{fffe}' || ch == '\u{ffff}')
        })
    {
        return Err(napi_error(
            "captionLines accepts up to two lines of up to 256 printable characters",
        ));
    }
    Ok((
        size,
        lines.into_iter().filter(|line| !line.is_empty()).collect(),
    ))
}

fn napi_error(error: impl ToString) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}
