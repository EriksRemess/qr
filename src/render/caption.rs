use super::Color;
use ab_glyph::{Font, FontRef, Glyph, OutlineCurve, PxScale, ScaleFont, point};
use std::fmt::Write;
use std::sync::LazyLock;

static FONT: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!("../../fonts/MPLUS1p-Regular.ttf"))
        .expect("bundled caption font")
});

pub(crate) struct Caption {
    glyphs: Vec<Glyph>,
    pub(super) height: usize,
}

impl Caption {
    pub(crate) fn layout(size: u32, lines: &[String]) -> Self {
        let unit = size as f32 / 512.0;
        let mut glyphs = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            let mut font = FONT.as_scaled(PxScale::from(30.0 * unit));
            let start = glyphs.len();
            let mut width = 0.0;
            let mut previous = None;
            for ch in line.chars() {
                let id = font.glyph_id(ch);
                if let Some(last) = previous {
                    width += font.kern(last, id);
                }
                glyphs.push(id.with_scale_and_position(
                    font.scale(),
                    point(width, (28.0 + 36.0 * row as f32) * unit),
                ));
                width += font.h_advance(id);
                previous = Some(id);
            }
            let fit = (480.0 * unit / width.max(1.0)).min(1.0);
            font.scale.x *= fit;
            for glyph in &mut glyphs[start..] {
                glyph.scale.x = font.scale.x;
                glyph.position.x = (size as f32 - width * fit) / 2.0 + glyph.position.x * fit;
            }
        }
        Self {
            glyphs,
            height: ((6 + 36 * lines.len()) as f32 * unit).round() as usize,
        }
    }

    pub(crate) fn coverage(&self, width: usize) -> Vec<u8> {
        let mut pixels = vec![0; width * self.height];
        for glyph in &self.glyphs {
            if let Some(outline) = FONT.outline_glyph(glyph.clone()) {
                let bounds = outline.px_bounds();
                outline.draw(|x, y, alpha| {
                    let x = bounds.min.x as i32 + x as i32;
                    let y = bounds.min.y as i32 + y as i32;
                    if (0..width as i32).contains(&x) && (0..self.height as i32).contains(&y) {
                        let pixel = &mut pixels[y as usize * width + x as usize];
                        *pixel = (*pixel).max((alpha * 255.0).round() as u8);
                    }
                });
            }
        }
        pixels
    }

    pub(crate) fn wrap_svg(
        &self,
        symbol: &str,
        size: u32,
        fg: Color,
        bg: Color,
        lines: &[String],
    ) -> String {
        let height = size as usize + self.height;
        let mut svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{height}" viewBox="0 0 {size} {height}">"#
        );
        if bg.alpha != 0 {
            write!(
                svg,
                r#"<rect width="{size}" height="{height}" fill="{}" fill-opacity="{}"/>"#,
                bg.svg_hex(),
                f32::from(bg.alpha) / 255.0
            )
            .unwrap();
        }
        write!(
            svg,
            r#"{symbol}<g transform="translate(0 {size})" fill="{}" fill-opacity="{}"><title>"#,
            fg.svg_hex(),
            f32::from(fg.alpha) / 255.0
        )
        .unwrap();
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                svg.push(' ');
            }
            svg.push_str(
                &line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;"),
            );
        }
        svg.push_str("</title>");
        for glyph in &self.glyphs {
            let Some(outline) = FONT.outline(glyph.id) else {
                continue;
            };
            let scaled = FONT.as_scaled(glyph.scale);
            write!(
                svg,
                r#"<path transform="translate({} {}) scale({} {})" d=""#,
                glyph.position.x,
                glyph.position.y,
                scaled.h_scale_factor(),
                -scaled.v_scale_factor()
            )
            .unwrap();
            let mut last = None;
            for curve in outline.curves {
                let start = match curve {
                    OutlineCurve::Line(a, _)
                    | OutlineCurve::Quad(a, _, _)
                    | OutlineCurve::Cubic(a, _, _, _) => a,
                };
                if last != Some(start) {
                    if last.is_some() {
                        svg.push('Z');
                    }
                    write!(svg, "M{} {}", start.x, start.y).unwrap();
                }
                last = Some(match curve {
                    OutlineCurve::Line(_, b) => {
                        write!(svg, "L{} {}", b.x, b.y).unwrap();
                        b
                    }
                    OutlineCurve::Quad(_, b, c) => {
                        write!(svg, "Q{} {} {} {}", b.x, b.y, c.x, c.y).unwrap();
                        c
                    }
                    OutlineCurve::Cubic(_, b, c, d) => {
                        write!(svg, "C{} {} {} {} {} {}", b.x, b.y, c.x, c.y, d.x, d.y).unwrap();
                        d
                    }
                });
            }
            svg.push_str("Z\"/>");
        }
        svg.push_str("</g></svg>");
        svg
    }
}
