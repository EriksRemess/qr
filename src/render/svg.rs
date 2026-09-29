use super::{ModuleShape, Style};
use crate::assets::SvgLogo;
use crate::core::Symbol;
use std::fmt::Write;

/// Render a standalone SVG document.
///
/// Square modules are coalesced into horizontal runs. This substantially
/// reduces both generation time and document size compared with one element
/// per module. Dot styling uses circles only for data modules; the fixed QR
/// patterns share the compact run path and remain geometrically exact.
pub fn render_svg(symbol: &Symbol, style: &Style, size: u32, logo: Option<&SvgLogo>) -> String {
    let margin = style.margin as usize;
    let extent = symbol.size + margin * 2;
    let mut output = String::with_capacity(symbol.modules.len() * 4);
    write!(
        output,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" viewBox=\"0 0 {extent} {extent}\""
    )
    .expect("writing to a String cannot fail");
    if style.shape == ModuleShape::Square {
        output.push_str(" shape-rendering=\"crispEdges\"");
    }
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

    let mut square_path = String::with_capacity(symbol.modules.len() * 2);
    for y in 0..symbol.size {
        let mut x = 0;
        while x < symbol.size {
            let dark_square = symbol.module(x, y)
                && (style.shape == ModuleShape::Square || symbol.is_function(x, y));
            if !dark_square {
                x += 1;
                continue;
            }
            let start = x;
            x += 1;
            while x < symbol.size
                && symbol.module(x, y)
                && (style.shape == ModuleShape::Square || symbol.is_function(x, y))
            {
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

    if !square_path.is_empty() {
        write!(
            output,
            "<path fill=\"{}\"{} d=\"{square_path}\"/>",
            style.foreground.svg_hex(),
            opacity_attribute("fill", style.foreground.svg_opacity())
        )
        .expect("writing to a String cannot fail");
    }

    if style.shape == ModuleShape::Dot {
        output.push_str("<path");
        write!(output, " fill=\"{}\"", style.foreground.svg_hex())
            .expect("writing to a String cannot fail");
        output.push_str(&opacity_attribute("fill", style.foreground.svg_opacity()));
        output.push_str(" d=\"");
        for y in 0..symbol.size {
            for x in 0..symbol.size {
                if symbol.module(x, y) && !symbol.is_function(x, y) {
                    // Two half-circle arcs form a circle without requiring one
                    // SVG element per module.
                    write!(
                        output,
                        "M{} {}a.5.5 0 1 0 1 0a.5.5 0 1 0-1 0",
                        x + margin,
                        y + margin
                    )
                    .expect("writing to a String cannot fail");
                }
            }
        }
        output.push_str("\"/>");
    }

    if let Some(logo) = logo {
        append_logo(&mut output, logo, style, extent);
    }

    output.push_str("</svg>");
    output
}

fn append_logo(output: &mut String, logo: &SvgLogo, style: &Style, extent: usize) {
    let target = extent as f64 * style.logo_scale;
    let scale = (target / logo.width).min(target / logo.height);
    let width = logo.width * scale;
    let height = logo.height * scale;
    let x = (extent as f64 - width) / 2.0;
    let y = (extent as f64 - height) / 2.0;
    let padding = style.logo_padding;
    if style.logo_background.alpha != 0 {
        write!(
            output,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" shape-rendering=\"geometricPrecision\" fill=\"{}\"{}/>",
            decimal(x - padding),
            decimal(y - padding),
            decimal(width + padding * 2.0),
            decimal(height + padding * 2.0),
            decimal(padding),
            style.logo_background.svg_hex(),
            opacity_attribute("fill", style.logo_background.svg_opacity())
        )
        .expect("writing to a String cannot fail");
    }
    write!(
        output,
        "<g transform=\"translate({} {}) scale({}) translate({} {})\" shape-rendering=\"geometricPrecision\" text-rendering=\"geometricPrecision\">",
        decimal(x),
        decimal(y),
        decimal(scale),
        decimal(-logo.view_x),
        decimal(-logo.view_y)
    )
    .expect("writing to a String cannot fail");
    if let Some(outline) = style
        .svg_logo_outline
        .filter(|color| color.alpha != 0 && style.svg_logo_outline_width > 0.0)
    {
        write!(
            output,
            "<g aria-hidden=\"true\" fill-opacity=\"0\" stroke=\"{}\"{} stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\">{}</g>",
            outline.svg_hex(),
            opacity_attribute("stroke", outline.svg_opacity()),
            decimal(style.svg_logo_outline_width),
            logo.body
        )
        .expect("writing to a String cannot fail");
    }
    output.push_str(&logo.body);
    output.push_str("</g>");
}

fn decimal(value: f64) -> String {
    let formatted = format!("{value:.4}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
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
        let logo =
            SvgLogo::parse(r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5"/></svg>"#)
                .unwrap();
        let style = Style {
            svg_logo_outline: Some(Color::WHITE),
            svg_logo_outline_width: 8.0,
            ..Style::default()
        };
        let svg = render_svg(&symbol, &style, 512, Some(&logo));
        assert!(svg.contains("shape-rendering=\"crispEdges\""));
        assert_eq!(
            svg.matches("shape-rendering=\"geometricPrecision\"")
                .count(),
            2
        );
        assert!(svg.contains("text-rendering=\"geometricPrecision\""));
        assert!(svg.contains("aria-hidden=\"true\" fill-opacity=\"0\" stroke=\"#ffffff\""));
    }
}
