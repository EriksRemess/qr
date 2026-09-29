//! Format-independent styling and concrete SVG/PNG renderers.

mod png;
mod svg;

use crate::core::{EcLevel, Symbol};
use std::fmt;

pub use png::render_png;
pub use svg::render_svg;

pub const DEFAULT_SIZE: u32 = 512;
pub const MAX_SIZE: u32 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    };
    pub const WHITE: Self = Self {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    };

    pub fn parse(input: &str) -> Result<Self, RenderError> {
        let raw = input.strip_prefix('#').unwrap_or(input);
        let expanded;
        let value = match raw.len() {
            3 | 4 => {
                expanded = raw
                    .chars()
                    .flat_map(|character| [character, character])
                    .collect::<String>();
                expanded.as_str()
            }
            6 | 8 => raw,
            _ => return Err(RenderError::InvalidColor(input.to_owned())),
        };

        let byte = |start: usize| {
            u8::from_str_radix(&value[start..start + 2], 16)
                .map_err(|_| RenderError::InvalidColor(input.to_owned()))
        };
        Ok(Self {
            red: byte(0)?,
            green: byte(2)?,
            blue: byte(4)?,
            alpha: if value.len() == 8 { byte(6)? } else { 255 },
        })
    }

    fn svg_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    fn svg_opacity(self) -> Option<String> {
        if self.alpha == 255 {
            None
        } else {
            let opacity = f64::from(self.alpha) / 255.0;
            Some(format!("{opacity:.3}"))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ModuleShape {
    #[default]
    Square,
    /// Circular data modules. Functional patterns remain square for reliable
    /// acquisition and alignment.
    Dot,
}

#[derive(Clone, Debug)]
pub struct Style {
    pub foreground: Color,
    pub background: Color,
    pub margin: u32,
    pub shape: ModuleShape,
    pub error_correction: EcLevel,
    /// Maximum logo width/height as a fraction of the complete QR image.
    pub logo_scale: f64,
    /// Backing padding around the logo, measured in QR modules.
    pub logo_padding: f64,
    pub logo_background: Color,
    /// Optional vector outline around an embedded SVG logo.
    pub svg_logo_outline: Option<Color>,
    /// SVG logo outline width in the logo's own view-box coordinate system.
    pub svg_logo_outline_width: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            foreground: Color::BLACK,
            background: Color::WHITE,
            margin: 4,
            shape: ModuleShape::Square,
            error_correction: EcLevel::Medium,
            logo_scale: 1.0 / 3.0,
            logo_padding: 0.35,
            logo_background: Color::WHITE,
            svg_logo_outline: None,
            svg_logo_outline_width: 0.0,
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    InvalidColor(String),
    InvalidSize(u32),
    OutputTooSmall { size: usize, minimum: usize },
    InvalidPngLogo(&'static str),
    InvalidSvgLogo,
    Compression,
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidColor(value) => write!(formatter, "invalid color: {value}"),
            Self::InvalidSize(value) => write!(
                formatter,
                "size must be between 21 and {MAX_SIZE} pixels; received {value}"
            ),
            Self::OutputTooSmall { size, minimum } => write!(
                formatter,
                "PNG size {size} is too small for this symbol; use at least {minimum} pixels"
            ),
            Self::InvalidPngLogo(reason) => write!(formatter, "invalid PNG logo: {reason}"),
            Self::InvalidSvgLogo => formatter.write_str("invalid SVG logo"),
            Self::Compression => formatter.write_str("PNG compression failed"),
        }
    }
}

impl std::error::Error for RenderError {}

pub fn validate_size(size: u32) -> Result<usize, RenderError> {
    if !(21..=MAX_SIZE).contains(&size) {
        return Err(RenderError::InvalidSize(size));
    }
    Ok(size as usize)
}

/// Convert an output pixel coordinate to a symbol-space coordinate.
///
/// Integer arithmetic makes module boundaries deterministic even when the
/// requested image size is not divisible by the module count.
#[inline]
fn symbol_coordinate(pixel: usize, output_size: usize, total_modules: usize) -> usize {
    pixel * total_modules / output_size
}

#[inline]
fn is_dark_at(symbol: &Symbol, style: &Style, module_x: usize, module_y: usize) -> bool {
    let margin = style.margin as usize;
    if module_x < margin || module_y < margin {
        return false;
    }
    let x = module_x - margin;
    let y = module_y - margin;
    x < symbol.size && y < symbol.size && symbol.module(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_color_forms() {
        assert_eq!(
            Color::parse("#abc").unwrap(),
            Color {
                red: 0xAA,
                green: 0xBB,
                blue: 0xCC,
                alpha: 255
            }
        );
        assert_eq!(
            Color::parse("12345678").unwrap(),
            Color {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 0x78
            }
        );
        assert!(Color::parse("#12").is_err());
        assert!(Color::parse("#gggggg").is_err());
    }
}
