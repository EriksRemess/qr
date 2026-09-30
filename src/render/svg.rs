use super::{ModuleStyle, Style, rounded::RoundedGeometry};
use crate::assets::SvgLogo;
use crate::core::Symbol;
use std::borrow::Cow;
use std::fmt::Write;

/// Render a standalone SVG document.
///
/// Square modules are coalesced into horizontal runs. This substantially
/// reduces both generation time and document size compared with one element
/// per module, while keeping every module geometrically exact.
/// The rounded style uses the same connected contours as the PNG rasterizer.
pub fn render_svg(symbol: &Symbol, style: &Style, size: u32, logo: Option<&SvgLogo>) -> String {
    let margin = style.margin as usize;
    let extent = symbol.size + margin * 2;
    let mut output = String::with_capacity(symbol.modules.len() * 4);
    write!(
        output,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 {extent} {extent}\""
    )
    .expect("writing to a String cannot fail");
    output.push_str(" shape-rendering=\"crispEdges\"");
    output.push('>');

    if style.background.alpha != 0 {
        write!(
            output,
            "<path fill=\"{}\"{} d=\"M0 0h{extent}v{extent}H0z\"/>",
            style.background.svg_hex(),
            opacity_attribute("fill", style.background.svg_opacity())
        )
        .expect("writing to a String cannot fail");
    }

    let module_path = if style.module_style == ModuleStyle::Rounded {
        RoundedGeometry::new(symbol, margin, size as usize).svg_path()
    } else {
        let mut square_path = String::with_capacity(symbol.modules.len() * 2);
        for y in 0..symbol.size {
            let mut x = 0;
            while x < symbol.size {
                if !symbol.module(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                x += 1;
                while x < symbol.size && symbol.module(x, y) {
                    x += 1;
                }
                write!(
                    square_path,
                    "M{} {}h{}v1H{}z",
                    start + margin,
                    y + margin,
                    x - start,
                    start + margin
                )
                .expect("writing to a String cannot fail");
            }
        }
        square_path
    };

    if !module_path.is_empty() {
        write!(
            output,
            "<path fill=\"{}\"{}{} d=\"{module_path}\"/>",
            style.foreground.svg_hex(),
            opacity_attribute("fill", style.foreground.svg_opacity()),
            if style.module_style == ModuleStyle::Rounded {
                " shape-rendering=\"geometricPrecision\" fill-rule=\"evenodd\""
            } else {
                ""
            }
        )
        .expect("writing to a String cannot fail");
    }

    if let Some(logo) = logo {
        append_logo(&mut output, logo, style, extent, size);
    }

    output.push_str("</svg>");
    output
}

fn append_logo(output: &mut String, logo: &SvgLogo, style: &Style, extent: usize, size: u32) {
    let layout = super::LogoLayout::new(logo.width, logo.height, style, size as usize, extent);
    let coordinate = |pixel: usize| decimal(pixel as f64 * extent as f64 / f64::from(size));
    if style.logo_background.alpha != 0 {
        write!(output,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" shape-rendering=\"crispEdges\" fill=\"{}\"{}/>",
            coordinate(layout.left), coordinate(layout.top),
            coordinate(layout.right - layout.left), coordinate(layout.bottom - layout.top),
            style.logo_background.svg_hex(),
            opacity_attribute("fill", style.logo_background.svg_opacity())
        ).expect("writing to a String cannot fail");
    }
    // A data-URI SVG is a separate document: its CSS/IDs cannot affect QR
    // paths or the other image. Image dimensions are expressed in output
    // pixels, not QR modules, to avoid tiny intermediate SVG image surfaces
    // in rasterizers such as librsvg.
    write!(
        output,
        "<g transform=\"scale({})\">",
        decimal(extent as f64 / f64::from(size))
    )
    .unwrap();
    for (uri, padding, is_outline) in logo
        .outline_uri
        .iter()
        .map(|uri| (uri, logo.outline_padding, true))
        .chain(std::iter::once((&logo.image_uri, 0.0, false)))
    {
        let pad_x = layout.width as f64 * padding / logo.width;
        let pad_y = layout.height as f64 * padding / logo.height;
        // Inline strokes stay in logo units and scale with the QR, including
        // CSS resizing. Isolated outlines remove internal shape transforms;
        // their stroke width is specified in the image viewport's pixels.
        let stroke_width = decimal(if logo.inline_outline.is_some() {
            logo.outline_width
        } else {
            logo.outline_width
                * (layout.width as f64 / logo.width).min(layout.height as f64 / logo.height)
        });
        let inline = if !is_outline {
            &logo.inline_svg
        } else {
            &logo.inline_outline
        };
        if let Some(document) = inline {
            let document = if !is_outline {
                Cow::Borrowed(document.as_str())
            } else {
                Cow::Owned(document.replace(&logo.outline_marker, &stroke_width))
            };
            write!(output, "<g transform=\"translate({} {}) scale({} {})\" shape-rendering=\"geometricPrecision\" text-rendering=\"geometricPrecision\">{document}</g>",
                decimal(layout.x as f64 - pad_x), decimal(layout.y as f64 - pad_y),
                decimal(layout.width as f64 / logo.width), decimal(layout.height as f64 / logo.height)).unwrap();
            continue;
        }
        let uri = if !is_outline {
            Cow::Borrowed(uri.as_str())
        } else {
            Cow::Owned(uri.replace(&logo.outline_marker, &stroke_width))
        };
        write!(output,
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" image-rendering=\"auto\" href=\"{uri}\"/>",
            decimal(layout.x as f64 - pad_x), decimal(layout.y as f64 - pad_y),
            decimal(layout.width as f64 + 2.0 * pad_x), decimal(layout.height as f64 + 2.0 * pad_y)
        ).expect("writing to a String cannot fail");
    }
    output.push_str("</g>");
}

fn decimal(value: f64) -> String {
    // Shortest round-tripping representation: a large logo viewBox can need
    // a scale far below 0.0001, which fixed precision would round to zero.
    value.to_string()
}

fn opacity_attribute(name: &str, opacity: Option<String>) -> String {
    opacity.map_or_else(String::new, |value| format!(" {name}-opacity=\"{value}\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EcLevel, encode_text};
    use crate::render::Color;

    #[test]
    fn emits_a_standalone_svg_with_exact_dimensions() {
        let symbol = encode_text("https://example.com/test", EcLevel::High).unwrap();
        let svg = render_svg(&symbol, &Style::default(), 512, None);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("width=\"512\" height=\"512\""));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn logo_overrides_crisp_module_rendering() {
        let symbol = encode_text("https://example.com/test", EcLevel::High).unwrap();
        let mut logo =
            SvgLogo::parse(r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5"/></svg>"#)
                .unwrap();
        let style = Style {
            svg_logo_outline: Some(Color::WHITE),
            svg_logo_outline_width: 8.0,
            ..Style::default()
        };
        logo.set_outline(Color::WHITE, 8.0).unwrap();
        let svg = render_svg(&symbol, &style, 512, Some(&logo));
        assert!(svg.contains("shape-rendering=\"crispEdges\""));
        assert!(!svg.contains("<image "));
        assert!(svg.contains("fill=\"none\" stroke=\"#ffffff\""));
        assert!(!svg.contains(" style="));
        assert!(svg.contains("shape-rendering=\"geometricPrecision\""));
    }
}
