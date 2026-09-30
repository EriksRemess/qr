use super::{
    LogoLayout, ModuleStyle, RenderError, Style, is_dark_at, rounded::RoundedGeometry,
    symbol_coordinate, validate_size,
};
use crate::assets::PngLogo;
use crate::core::Symbol;
use zlib_rs::{DeflateConfig, ReturnCode, Strategy, compress_bound, compress_slice, crc32::crc32};

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Render an 8-bit RGBA, non-interlaced PNG.
///
/// Rasterization and compression happen exactly once. Scanlines use the Sub
/// filter because QR images contain long same-color runs; the zlib RLE strategy
/// is selected for the same reason. Both choices are benchmarked rather than
/// treated as permanent format requirements.
pub fn render_png(
    symbol: &Symbol,
    style: &Style,
    size: u32,
    logo: Option<&PngLogo>,
) -> Result<Vec<u8>, RenderError> {
    let size = validate_size(size)?;
    let total_modules = symbol.size + style.margin as usize * 2;
    if size < total_modules {
        return Err(RenderError::OutputTooSmall {
            size,
            minimum: total_modules,
        });
    }
    // Use RGBA for opaque and translucent output. The constant alpha lane
    // produces long zero runs after Sub filtering for RLE compression.
    let channels = 4;
    let row_bytes = size
        .checked_mul(channels)
        .ok_or(RenderError::InvalidSize(size as u32))?;
    let scanline_bytes = row_bytes
        .checked_add(1)
        .and_then(|value| value.checked_mul(size))
        .ok_or(RenderError::InvalidSize(size as u32))?;
    let mut filtered = vec![0_u8; scanline_bytes];
    let logo = logo.map(|logo| LogoPlacement::new(logo, style, size, total_modules));
    let mut square_row_cache: Vec<Option<Vec<u8>>> = vec![None; total_modules];
    let mut rounded = (style.module_style == ModuleStyle::Rounded).then(|| {
        RoundedGeometry::new(symbol, style.margin as usize, size).raster(size, total_modules)
    });
    let mut coverage = if rounded.is_some() {
        vec![0_u8; size]
    } else {
        Vec::new()
    };
    let mut previous_coverage = coverage.clone();
    let mut previous_row_cacheable = false;
    // Match SVG's foreground-over-background compositing, once per render
    // rather than once per pixel.
    let background = Pixel::from_color(style.background);
    let foreground = Pixel::from_color(style.foreground).over(background);
    // Rounded antialiasing has only 256 coverage values. Blend each value once
    // instead of repeating alpha multiplication and channel division per edge
    // pixel. Square rendering does not construct this table.
    let coverage_colors: Option<[Pixel; 256]> = rounded.as_ref().map(|_| {
        std::array::from_fn(|coverage| {
            let mut source = Pixel::from_color(style.foreground);
            source.alpha = ((u16::from(source.alpha) * coverage as u16 + 127) / 255) as u8;
            source.over(background)
        })
    });

    for y in 0..size {
        let row_start = y * (row_bytes + 1);
        let module_y = symbol_coordinate(y, size, total_modules);
        let outside_logo = logo
            .as_ref()
            .is_none_or(|placement| y < placement.backing_y || y >= placement.backing_bottom);
        let cacheable = rounded.is_none() && outside_logo;
        if cacheable && let Some(cached) = &square_row_cache[module_y] {
            filtered[row_start..row_start + row_bytes + 1].copy_from_slice(cached);
            continue;
        }

        if let Some(raster) = &mut rounded {
            raster.row(y, &mut coverage);
            // Straight sections of rounded contours still produce identical
            // scanlines. Sub filtering is row-independent, so reuse the whole
            // filtered row when coverage matches and no logo intersects it.
            if outside_logo && previous_row_cacheable && coverage == previous_coverage {
                let (previous, current) = filtered.split_at_mut(row_start);
                current[..row_bytes + 1]
                    .copy_from_slice(&previous[row_start - row_bytes - 1..row_start]);
                continue;
            }
            previous_coverage.copy_from_slice(&coverage);
            previous_row_cacheable = outside_logo;
        }

        filtered[row_start] = 1; // PNG Sub filter.
        let mut left = [0_u8; 4];
        for x in 0..size {
            // Rounded coverage already identifies the pixel's foreground.
            // Avoid a variable integer division per pixel on this path.
            let module_x = if rounded.is_some() {
                0
            } else {
                symbol_coordinate(x, size, total_modules)
            };
            let color = pixel_color(
                symbol,
                style,
                (x, y),
                (module_x, module_y),
                (foreground, background),
                coverage_colors
                    .as_ref()
                    .map(|colors| colors[coverage[x] as usize]),
                logo.as_ref(),
            );
            let rgba = [color.red, color.green, color.blue, color.alpha];
            for channel in 0..channels {
                filtered[row_start + 1 + x * channels + channel] =
                    rgba[channel].wrapping_sub(left[channel]);
            }
            left = rgba;
        }
        if cacheable {
            square_row_cache[module_y] =
                Some(filtered[row_start..row_start + row_bytes + 1].to_vec());
        }
    }

    let mut config = DeflateConfig::best_speed();
    config.strategy = Strategy::Rle;
    let mut compressed = vec![0_u8; compress_bound(filtered.len())];
    let compressed_len = {
        let (bytes, status) = compress_slice(&mut compressed, &filtered, config);
        if status != ReturnCode::Ok {
            return Err(RenderError::Compression);
        }
        bytes.len()
    };
    compressed.truncate(compressed_len);

    let mut output = Vec::with_capacity(compressed.len() + 80);
    output.extend_from_slice(SIGNATURE);
    let mut header = [0_u8; 13];
    header[0..4].copy_from_slice(&(size as u32).to_be_bytes());
    header[4..8].copy_from_slice(&(size as u32).to_be_bytes());
    header[8] = 8; // Bit depth.
    header[9] = 6; // RGBA.
    pack_chunk(&mut output, b"IHDR", &header);
    pack_chunk(&mut output, b"IDAT", &compressed);
    pack_chunk(&mut output, b"IEND", &[]);
    Ok(output)
}

#[derive(Clone, Copy)]
struct Pixel {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl Pixel {
    fn from_color(color: super::Color) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
            alpha: color.alpha,
        }
    }

    /// Composite this straight-alpha foreground over another straight-alpha
    /// pixel. Integer math keeps PNG and SVG transparency semantics aligned
    /// without a floating-point operation for every output pixel.
    fn over(self, background: Self) -> Self {
        if self.alpha == 0 {
            return background;
        }
        if self.alpha == 255 {
            return self;
        }

        let source_alpha = u32::from(self.alpha);
        let background_alpha = u32::from(background.alpha);
        let inverse = 255 - source_alpha;
        let alpha_numerator = source_alpha * 255 + background_alpha * inverse;
        if alpha_numerator == 0 {
            return Self {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            };
        }
        let blend = |foreground: u8, backdrop: u8| {
            let numerator = u32::from(foreground) * source_alpha * 255
                + u32::from(backdrop) * background_alpha * inverse;
            ((numerator + alpha_numerator / 2) / alpha_numerator) as u8
        };
        Self {
            red: blend(self.red, background.red),
            green: blend(self.green, background.green),
            blue: blend(self.blue, background.blue),
            alpha: ((alpha_numerator + 127) / 255) as u8,
        }
    }
}

struct LogoPlacement<'a> {
    logo: &'a PngLogo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    backing_x: usize,
    backing_y: usize,
    backing_right: usize,
    backing_bottom: usize,
    background: Pixel,
    x_samples: Vec<AxisSample>,
    y_samples: Vec<AxisSample>,
}

#[derive(Clone, Copy)]
struct AxisSample {
    lower: usize,
    upper: usize,
    upper_weight: u16,
}

impl<'a> LogoPlacement<'a> {
    fn new(logo: &'a PngLogo, style: &Style, size: usize, total_modules: usize) -> Self {
        let layout = LogoLayout::new(
            logo.width as f64,
            logo.height as f64,
            style,
            size,
            total_modules,
        );
        let (x, y, width, height) = (layout.x, layout.y, layout.width, layout.height);
        Self {
            logo,
            x,
            y,
            width,
            height,
            backing_x: layout.left,
            backing_y: layout.top,
            backing_right: layout.right,
            backing_bottom: layout.bottom,
            background: Pixel::from_color(style.logo_background),
            x_samples: sampling_axis(logo.width, width),
            y_samples: sampling_axis(logo.height, height),
        }
    }

    fn composite(&self, x: usize, y: usize, base: Pixel) -> Pixel {
        let base = if x >= self.backing_x
            && x < self.backing_right
            && y >= self.backing_y
            && y < self.backing_bottom
        {
            self.background.over(base)
        } else {
            base
        };
        if x < self.x || x >= self.x + self.width || y < self.y || y >= self.y + self.height {
            return base;
        }

        self.sample(x - self.x, y - self.y).over(base)
    }

    /// Bilinear filtering avoids magnifying source pixels and keeps raster
    /// logos visually aligned with their vector equivalents. Interpolation is
    /// performed in premultiplied-alpha space so transparent edge pixels do
    /// not introduce dark or colored fringes.
    fn sample(&self, x: usize, y: usize) -> Pixel {
        let horizontal = self.x_samples[x];
        let vertical = self.y_samples[y];
        let weights = [
            u64::from(256 - horizontal.upper_weight) * u64::from(256 - vertical.upper_weight),
            u64::from(horizontal.upper_weight) * u64::from(256 - vertical.upper_weight),
            u64::from(256 - horizontal.upper_weight) * u64::from(vertical.upper_weight),
            u64::from(horizontal.upper_weight) * u64::from(vertical.upper_weight),
        ];
        let offsets = [
            (vertical.lower * self.logo.width + horizontal.lower) * 4,
            (vertical.lower * self.logo.width + horizontal.upper) * 4,
            (vertical.upper * self.logo.width + horizontal.lower) * 4,
            (vertical.upper * self.logo.width + horizontal.upper) * 4,
        ];
        let mut alpha_sum = 0_u64;
        let mut premultiplied = [0_u64; 3];
        for (weight, offset) in weights.into_iter().zip(offsets) {
            let alpha = u64::from(self.logo.rgba[offset + 3]);
            alpha_sum += weight * alpha;
            for (channel, sum) in premultiplied.iter_mut().enumerate() {
                *sum += weight * alpha * u64::from(self.logo.rgba[offset + channel]);
            }
        }
        if alpha_sum == 0 {
            return Pixel {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            };
        }
        Pixel {
            red: ((premultiplied[0] + alpha_sum / 2) / alpha_sum) as u8,
            green: ((premultiplied[1] + alpha_sum / 2) / alpha_sum) as u8,
            blue: ((premultiplied[2] + alpha_sum / 2) / alpha_sum) as u8,
            alpha: ((alpha_sum + 32_768) / 65_536) as u8,
        }
    }
}

fn sampling_axis(source_length: usize, target_length: usize) -> Vec<AxisSample> {
    (0..target_length)
        .map(|target| {
            let position = ((target as f64 + 0.5) * source_length as f64 / target_length as f64
                - 0.5)
                .clamp(0.0, (source_length - 1) as f64);
            let lower = position.floor() as usize;
            let upper = (lower + 1).min(source_length - 1);
            AxisSample {
                lower,
                upper,
                upper_weight: ((position - lower as f64) * 256.0).round() as u16,
            }
        })
        .collect()
}

fn pixel_color(
    symbol: &Symbol,
    style: &Style,
    pixel: (usize, usize),
    module: (usize, usize),
    colors: (Pixel, Pixel),
    coverage: Option<Pixel>,
    logo: Option<&LogoPlacement<'_>>,
) -> Pixel {
    let (pixel_x, pixel_y) = pixel;
    let (module_x, module_y) = module;
    let pixel = match coverage {
        Some(pixel) => pixel,
        None if is_dark_at(symbol, style, module_x, module_y) => colors.0,
        None => colors.1,
    };
    logo.map_or(pixel, |placement| {
        placement.composite(pixel_x, pixel_y, pixel)
    })
}

fn pack_chunk(output: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let checksum_start = output.len();
    output.extend_from_slice(chunk_type);
    output.extend_from_slice(data);
    let checksum = crc32(0, &output[checksum_start..]);
    output.extend_from_slice(&checksum.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EcLevel, encode_text};

    #[test]
    fn emits_png_signature_and_required_chunks() {
        let symbol = encode_text("https://example.com/test", EcLevel::High).unwrap();
        let png = render_png(&symbol, &Style::default(), 256, None).unwrap();
        assert_eq!(&png[..8], SIGNATURE);
        assert!(png.windows(4).any(|window| window == b"IHDR"));
        assert!(png.windows(4).any(|window| window == b"IDAT"));
        assert_eq!(&png[png.len() - 12..png.len() - 8], &[0, 0, 0, 0]);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn straight_alpha_compositing_preserves_transparency() {
        let transparent = Pixel {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        };
        let half_red = Pixel {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128,
        };
        let result = half_red.over(transparent);
        assert_eq!(
            [result.red, result.green, result.blue, result.alpha],
            [255, 0, 0, 128]
        );

        let blue = Pixel {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        let result = half_red.over(blue);
        assert_eq!(
            [result.red, result.green, result.blue, result.alpha],
            [128, 0, 127, 255]
        );
    }

    #[test]
    fn logo_resampling_uses_premultiplied_alpha() {
        let logo = PngLogo {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
        };
        let placement = LogoPlacement {
            logo: &logo,
            x: 0,
            y: 0,
            width: 3,
            height: 1,
            backing_x: 0,
            backing_y: 0,
            backing_right: 3,
            backing_bottom: 1,
            background: Pixel {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            x_samples: sampling_axis(2, 3),
            y_samples: sampling_axis(1, 1),
        };

        let midpoint = placement.sample(1, 0);
        assert_eq!(
            [midpoint.red, midpoint.green, midpoint.blue, midpoint.alpha],
            [255, 0, 0, 128]
        );
    }
}
