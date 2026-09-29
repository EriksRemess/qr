//! Parsing and preparation of trusted, reusable logo assets.
//!
//! SVG logos remain vector fragments for SVG output. PNG logos are decoded once
//! in the renderer constructor and retained as RGBA pixels. The PNG decoder is
//! intentionally limited to the static logo format needed by this package; it
//! is not a general replacement for an image library.

use crate::render::RenderError;
use zlib_rs::{InflateConfig, ReturnCode, crc32::crc32, decompress_slice};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_LOGO_DIMENSION: usize = 4096;
const MAX_LOGO_COMPRESSED_BYTES: usize = 16 * 1024 * 1024;
const MAX_LOGO_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct SvgLogo {
    pub body: String,
    pub view_x: f64,
    pub view_y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub struct PngLogo {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl SvgLogo {
    /// Extract an SVG root's view box and body.
    ///
    /// Logo SVG is trusted application configuration, not user input. The
    /// parser still rejects malformed roots and non-finite geometry so invalid
    /// assets fail during renderer construction instead of corrupting output.
    pub fn parse(input: &str) -> Result<Self, RenderError> {
        let root_start = input.find("<svg").ok_or(RenderError::InvalidSvgLogo)?;
        let root_end = input[root_start..]
            .find('>')
            .map(|index| root_start + index)
            .ok_or(RenderError::InvalidSvgLogo)?;
        let close = input.rfind("</svg>").ok_or(RenderError::InvalidSvgLogo)?;
        if close <= root_end {
            return Err(RenderError::InvalidSvgLogo);
        }
        let attributes = &input[root_start + 4..root_end];
        let view_box = attribute(attributes, "viewBox").ok_or(RenderError::InvalidSvgLogo)?;
        let values = view_box
            .split(|character: char| character.is_ascii_whitespace() || character == ',')
            .filter(|part| !part.is_empty())
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RenderError::InvalidSvgLogo)?;
        if values.len() != 4
            || values.iter().any(|value| !value.is_finite())
            || values[2] <= 0.0
            || values[3] <= 0.0
        {
            return Err(RenderError::InvalidSvgLogo);
        }
        Ok(Self {
            body: input[root_end + 1..close].trim().to_owned(),
            view_x: values[0],
            view_y: values[1],
            width: values[2],
            height: values[3],
        })
    }
}

impl PngLogo {
    /// Decode an 8-bit, non-interlaced RGB or RGBA PNG.
    pub fn decode(input: &[u8]) -> Result<Self, RenderError> {
        if input.len() < PNG_SIGNATURE.len() || &input[..8] != PNG_SIGNATURE {
            return Err(RenderError::InvalidPngLogo("invalid PNG signature"));
        }

        let mut offset = 8;
        let mut header = None;
        let mut compressed = Vec::new();
        let mut saw_end = false;
        while offset < input.len() {
            if input.len() - offset < 12 {
                return Err(RenderError::InvalidPngLogo("truncated PNG chunk"));
            }
            let length = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
            let data_start = offset + 8;
            let data_end = data_start
                .checked_add(length)
                .ok_or(RenderError::InvalidPngLogo("PNG chunk is too large"))?;
            let chunk_end = data_end
                .checked_add(4)
                .ok_or(RenderError::InvalidPngLogo("PNG chunk is too large"))?;
            if chunk_end > input.len() {
                return Err(RenderError::InvalidPngLogo("truncated PNG chunk"));
            }
            let chunk_type: &[u8; 4] = input[offset + 4..offset + 8].try_into().unwrap();
            let expected_crc = u32::from_be_bytes(input[data_end..chunk_end].try_into().unwrap());
            if crc32(0, &input[offset + 4..data_end]) != expected_crc {
                return Err(RenderError::InvalidPngLogo("PNG chunk checksum mismatch"));
            }

            match chunk_type {
                b"IHDR" => {
                    if header.is_some() || length != 13 {
                        return Err(RenderError::InvalidPngLogo("invalid IHDR chunk"));
                    }
                    let width =
                        u32::from_be_bytes(input[data_start..data_start + 4].try_into().unwrap())
                            as usize;
                    let height = u32::from_be_bytes(
                        input[data_start + 4..data_start + 8].try_into().unwrap(),
                    ) as usize;
                    let depth = input[data_start + 8];
                    let color_type = input[data_start + 9];
                    let compression = input[data_start + 10];
                    let filtering = input[data_start + 11];
                    let interlace = input[data_start + 12];
                    if width == 0
                        || height == 0
                        || width > MAX_LOGO_DIMENSION
                        || height > MAX_LOGO_DIMENSION
                        || depth != 8
                        || !matches!(color_type, 2 | 6)
                        || compression != 0
                        || filtering != 0
                        || interlace != 0
                    {
                        return Err(RenderError::InvalidPngLogo(
                            "logo PNG must be non-interlaced 8-bit RGB or RGBA",
                        ));
                    }
                    header = Some((width, height, color_type));
                }
                b"IDAT" => {
                    if compressed.len() + length > MAX_LOGO_COMPRESSED_BYTES {
                        return Err(RenderError::InvalidPngLogo("compressed logo is too large"));
                    }
                    compressed.extend_from_slice(&input[data_start..data_end]);
                }
                b"IEND" => {
                    if length != 0 {
                        return Err(RenderError::InvalidPngLogo("invalid IEND chunk"));
                    }
                    saw_end = true;
                    break;
                }
                _ => {}
            }
            offset = chunk_end;
        }

        let (width, height, color_type) =
            header.ok_or(RenderError::InvalidPngLogo("missing IHDR chunk"))?;
        if !saw_end || compressed.is_empty() {
            return Err(RenderError::InvalidPngLogo("incomplete PNG logo"));
        }
        let channels = if color_type == 6 { 4 } else { 3 };
        let row_bytes = width
            .checked_mul(channels)
            .ok_or(RenderError::InvalidPngLogo("logo dimensions overflow"))?;
        let inflated_len = row_bytes
            .checked_add(1)
            .and_then(|value| value.checked_mul(height))
            .ok_or(RenderError::InvalidPngLogo("logo dimensions overflow"))?;
        if inflated_len > MAX_LOGO_DECOMPRESSED_BYTES {
            return Err(RenderError::InvalidPngLogo(
                "decompressed logo is too large",
            ));
        }
        let mut filtered = vec![0_u8; inflated_len];
        let (inflated, status) =
            decompress_slice(&mut filtered, &compressed, InflateConfig::default());
        if status != ReturnCode::Ok || inflated.len() != inflated_len {
            return Err(RenderError::InvalidPngLogo("invalid compressed image data"));
        }

        let mut raw = vec![0_u8; row_bytes * height];
        for y in 0..height {
            let filter = filtered[y * (row_bytes + 1)];
            for x in 0..row_bytes {
                let encoded = filtered[y * (row_bytes + 1) + 1 + x];
                let left = if x >= channels {
                    raw[y * row_bytes + x - channels]
                } else {
                    0
                };
                let up = if y > 0 {
                    raw[(y - 1) * row_bytes + x]
                } else {
                    0
                };
                let upper_left = if y > 0 && x >= channels {
                    raw[(y - 1) * row_bytes + x - channels]
                } else {
                    0
                };
                raw[y * row_bytes + x] = match filter {
                    0 => encoded,
                    1 => encoded.wrapping_add(left),
                    2 => encoded.wrapping_add(up),
                    3 => encoded.wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                    4 => encoded.wrapping_add(paeth(left, up, upper_left)),
                    _ => return Err(RenderError::InvalidPngLogo("unsupported PNG row filter")),
                };
            }
        }

        let rgba = if channels == 4 {
            raw
        } else {
            let mut rgba = Vec::with_capacity(width * height * 4);
            for pixel in raw.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            rgba
        };
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

fn attribute<'a>(attributes: &'a str, name: &str) -> Option<&'a str> {
    let start = attributes.find(name)? + name.len();
    let rest = attributes[start..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.as_bytes().first().copied()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let value = &rest[1..];
    let end = value.find(quote as char)?;
    Some(&value[..end])
}

fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let upper_left = i32::from(upper_left);
    let prediction = left + up - upper_left;
    let left_distance = (prediction - left).abs();
    let up_distance = (prediction - up).abs();
    let diagonal_distance = (prediction - upper_left).abs();
    if left_distance <= up_distance && left_distance <= diagonal_distance {
        left as u8
    } else if up_distance <= diagonal_distance {
        up as u8
    } else {
        upper_left as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_svg_view_box_and_body() {
        let logo = SvgLogo::parse(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="8 16 234 117"><path d="M0 0"/></svg>"#,
        )
        .unwrap();
        assert_eq!(
            (logo.view_x, logo.view_y, logo.width, logo.height),
            (8.0, 16.0, 234.0, 117.0)
        );
        assert_eq!(logo.body, r#"<path d="M0 0"/>"#);
    }
}
