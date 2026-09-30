//! Shared connected contours for rounded SVG and PNG output. Each dark region
//! is painted once, including its holes, so translucent modules have no seams.

use crate::core::Symbol;
use std::collections::BTreeMap;
use std::fmt::Write;

const MODULE_RADIUS: f64 = 0.2;
const RASTER_SAMPLES: usize = 8;

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy)]
enum Segment {
    Line(Point),
    Arc {
        end: Point,
        center: Point,
        radius: f64,
        clockwise: bool,
    },
}

struct Contour {
    start: Point,
    segments: Vec<Segment>,
}

pub(super) struct RoundedGeometry {
    contours: Vec<Contour>,
}

impl RoundedGeometry {
    pub fn new(symbol: &Symbol, margin: usize, output_size: usize) -> Self {
        let size = symbol.size;
        let stride = size + 1;
        // Directions E/S/W/N. Exposed module edges run with dark on the right.
        let mut edges = vec![0_u8; stride * stride];
        let dark = |x: isize, y: isize| {
            x >= 0
                && y >= 0
                && x < size as isize
                && y < size as isize
                && !finder_cell(x as usize, y as usize, size)
                && symbol.module(x as usize, y as usize)
        };
        for y in 0..size {
            for x in 0..size {
                if !dark(x as isize, y as isize) {
                    continue;
                }
                if !dark(x as isize, y as isize - 1) {
                    edges[y * stride + x] |= 1;
                }
                if !dark(x as isize + 1, y as isize) {
                    edges[y * stride + x + 1] |= 2;
                }
                if !dark(x as isize, y as isize + 1) {
                    edges[(y + 1) * stride + x + 1] |= 4;
                }
                if !dark(x as isize - 1, y as isize) {
                    edges[(y + 1) * stride + x] |= 8;
                }
            }
        }
        let mut contours = Vec::new();
        for start in 0..edges.len() {
            while edges[start] != 0 {
                let initial = edges[start].trailing_zeros() as usize;
                let mut direction = initial;
                let mut vertex = start;
                let mut vertices = Vec::new();
                loop {
                    vertices.push(Point {
                        x: (vertex % stride + margin) as f64,
                        y: (vertex / stride + margin) as f64,
                    });
                    edges[vertex] &= !(1 << direction);
                    vertex = match direction {
                        0 => vertex + 1,
                        1 => vertex + stride,
                        2 => vertex - 1,
                        _ => vertex - stride,
                    };
                    // A right turn separates cells touching only diagonally.
                    let available = edges[vertex] | if vertex == start { 1 << initial } else { 0 };
                    let next = [
                        (direction + 1) % 4,
                        direction,
                        (direction + 3) % 4,
                        (direction + 2) % 4,
                    ]
                    .into_iter()
                    .find(|next| available & (1 << next) != 0);
                    if vertex == start && next == Some(initial) {
                        break;
                    }
                    direction = next.expect("closed module boundary");
                }
                let corners = (0..vertices.len())
                    .filter_map(|i| {
                        let before = vertices[(i + vertices.len() - 1) % vertices.len()];
                        let point = vertices[i];
                        let after = vertices[(i + 1) % vertices.len()];
                        ((point.x - before.x) * (after.y - point.y)
                            != (point.y - before.y) * (after.x - point.x))
                            .then_some(point)
                    })
                    .collect::<Vec<_>>();
                contours.push(rounded_contour(&corners, MODULE_RADIUS));
            }
        }
        // Keep finder transitions crisp on their horizontal/vertical axes.
        // Fractional straight boundaries can change a scanner's module-size
        // estimate after antialiasing. Use the same pixel boundaries as square
        // modules, while keeping the circular corners antialiased.
        let extent = size + margin * 2;
        let scale = output_size as f64 / extent as f64;
        let align = |coordinate: usize| {
            if output_size >= extent {
                (coordinate * output_size).div_ceil(extent) as f64 / scale
            } else {
                coordinate as f64
            }
        };
        for (x, y) in [
            (margin, margin),
            (margin + size - 7, margin),
            (margin, margin + size - 7),
        ] {
            for (inset, side, radius) in [(0, 7, 1.5_f64), (1, 5, 1.0), (2, 3, 0.65)] {
                let right = align(x + inset + side);
                let bottom = align(y + inset + side);
                let x = align(x + inset);
                let y = align(y + inset);
                let radius = radius.min((right - x).min(bottom - y) / 2.0);
                contours.push(rounded_contour(
                    &[
                        Point { x, y },
                        Point { x: right, y },
                        Point {
                            x: right,
                            y: bottom,
                        },
                        Point { x, y: bottom },
                    ],
                    radius,
                ));
            }
        }
        Self { contours }
    }

    pub fn svg_path(&self) -> String {
        let mut output = String::new();
        for contour in &self.contours {
            output.push('M');
            write_point(&mut output, contour.start);
            for segment in &contour.segments {
                match segment {
                    Segment::Line(end) => {
                        output.push('L');
                        write_point(&mut output, *end);
                    }
                    Segment::Arc {
                        end,
                        radius,
                        clockwise,
                        ..
                    } => {
                        write!(output, "A{radius} {radius} 0 0 {} ", u8::from(*clockwise)).unwrap();
                        write_point(&mut output, *end);
                    }
                }
            }
            output.push('Z');
        }
        output
    }

    pub fn raster(&self, size: usize, extent: usize) -> RoundedRaster {
        let scale = size as f64 / extent as f64;
        let mut edges = Vec::new();
        let mut rows = vec![Vec::new(); size];
        let mut kernels = Vec::new();
        let mut kernel_ids = BTreeMap::new();
        for contour in &self.contours {
            let mut start = contour.start;
            for segment in &contour.segments {
                let end = match segment {
                    Segment::Line(end) | Segment::Arc { end, .. } => *end,
                };
                if start.y != end.y {
                    let edge = RasterEdge {
                        minimum: start.y.min(end.y) * scale,
                        maximum: start.y.max(end.y) * scale,
                        curve: match segment {
                            Segment::Line(_) => Curve::Vertical(start.x * scale),
                            Segment::Arc { center, radius, .. } => {
                                let key = (
                                    (center.y * 100.0).round() as i32,
                                    (radius * 100.0).round() as i32,
                                );
                                let kernel = *kernel_ids.entry(key).or_insert_with(|| {
                                    let index = kernels.len();
                                    kernels.push(CircleKernel {
                                        y: center.y * scale,
                                        radius: radius * scale,
                                    });
                                    index
                                });
                                Curve::Circle {
                                    x: center.x * scale,
                                    kernel,
                                    sign: if start.x + end.x > 2.0 * center.x {
                                        1.0
                                    } else {
                                        -1.0
                                    },
                                }
                            }
                        },
                    };
                    let index = edges.len();
                    for row in rows
                        .iter_mut()
                        .take((edge.maximum.ceil() as usize).min(size))
                        .skip(edge.minimum.floor() as usize)
                    {
                        row.push(index);
                    }
                    edges.push(edge);
                }
                start = end;
            }
        }
        let mut row_kernels = Vec::with_capacity(size);
        for (y, row) in rows.iter_mut().enumerate() {
            row.sort_unstable_by(|a, b| {
                edges[*a]
                    .x_at(
                        (y as f64 + 0.5).clamp(edges[*a].minimum, edges[*a].maximum),
                        &kernels,
                    )
                    .total_cmp(&edges[*b].x_at(
                        (y as f64 + 0.5).clamp(edges[*b].minimum, edges[*b].maximum),
                        &kernels,
                    ))
            });
            let mut ids = row
                .iter()
                .filter_map(|index| match edges[*index].curve {
                    Curve::Circle { kernel, .. } => Some(kernel),
                    _ => None,
                })
                .collect::<Vec<_>>();
            ids.sort_unstable();
            ids.dedup();
            row_kernels.push(ids);
        }
        RoundedRaster {
            edges,
            rows,
            roots: vec![0.0; kernels.len()],
            kernels,
            row_kernels,
            intersections: Vec::new(),
            boundary: vec![0.0; size],
            delta: vec![0; size + 1],
        }
    }
}

// Data contours use hundredth-module coordinates; pixel-aligned finders may
// need more precision. Six decimal places avoid floating-point tails while
// retaining subpixel accuracy even at the largest supported output size.
fn write_point(output: &mut String, point: Point) {
    for (index, value) in [point.x, point.y].into_iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        let units = (value * 1_000_000.0).round() as usize;
        let mut fraction = units % 1_000_000;
        if fraction == 0 {
            write!(output, "{}", units / 1_000_000).unwrap();
        } else {
            let mut digits = 6;
            while fraction.is_multiple_of(10) {
                fraction /= 10;
                digits -= 1;
            }
            write!(output, "{}.{fraction:0digits$}", units / 1_000_000).unwrap();
        }
    }
}

fn finder_cell(x: usize, y: usize, size: usize) -> bool {
    (y < 7 && (x < 7 || x >= size - 7)) || (x < 7 && y >= size - 7)
}

fn rounded_contour(corners: &[Point], radius: f64) -> Contour {
    let mut segments = Vec::with_capacity(corners.len() * 2);
    let mut first = None;
    for i in 0..corners.len() {
        let before = corners[(i + corners.len() - 1) % corners.len()];
        let point = corners[i];
        let after = corners[(i + 1) % corners.len()];
        let direction = |delta: f64| if delta == 0.0 { 0.0 } else { delta.signum() };
        let incoming = Point {
            x: direction(point.x - before.x),
            y: direction(point.y - before.y),
        };
        let outgoing = Point {
            x: direction(after.x - point.x),
            y: direction(after.y - point.y),
        };
        let entry = Point {
            x: point.x - incoming.x * radius,
            y: point.y - incoming.y * radius,
        };
        let exit = Point {
            x: point.x + outgoing.x * radius,
            y: point.y + outgoing.y * radius,
        };
        if first.is_none() {
            first = Some(entry);
        } else {
            segments.push(Segment::Line(entry));
        }
        segments.push(Segment::Arc {
            end: exit,
            center: Point {
                x: entry.x + outgoing.x * radius,
                y: entry.y + outgoing.y * radius,
            },
            radius,
            clockwise: incoming.x * outgoing.y - incoming.y * outgoing.x > 0.0,
        });
    }
    let start = first.expect("a module boundary has corners");
    segments.push(Segment::Line(start));
    Contour { start, segments }
}

enum Curve {
    Vertical(f64),
    Circle { x: f64, kernel: usize, sign: f64 },
}

struct CircleKernel {
    y: f64,
    radius: f64,
}

impl CircleKernel {
    fn root(&self, height: f64) -> f64 {
        (self.radius * self.radius - (height - self.y).powi(2))
            .max(0.0)
            .sqrt()
    }
}

struct RasterEdge {
    minimum: f64,
    maximum: f64,
    curve: Curve,
}

impl RasterEdge {
    fn x_at(&self, height: f64, kernels: &[CircleKernel]) -> f64 {
        match self.curve {
            Curve::Vertical(x) => x,
            Curve::Circle { x, kernel, sign } => x + sign * kernels[kernel].root(height),
        }
    }
}

pub(super) struct RoundedRaster {
    edges: Vec<RasterEdge>,
    rows: Vec<Vec<usize>>,
    kernels: Vec<CircleKernel>,
    row_kernels: Vec<Vec<usize>>,
    roots: Vec<f64>,
    intersections: Vec<f64>,
    boundary: Vec<f64>,
    delta: Vec<i32>,
}

impl RoundedRaster {
    /// Eight vertical samples with exact horizontal interval coverage. Long
    /// solid spans use a difference array instead of touching every pixel for
    /// every sample. Circles are evaluated analytically, without a tessellator.
    pub fn row(&mut self, y: usize, output: &mut [u8]) {
        self.boundary.fill(0.0);
        self.delta.fill(0);
        let samples = if self.row_kernels[y].is_empty()
            && self.rows[y].iter().all(|index| {
                let edge = &self.edges[*index];
                edge.minimum <= y as f64 && edge.maximum >= (y + 1) as f64
            }) {
            1
        } else {
            RASTER_SAMPLES
        };
        for sample in 0..samples {
            let height = y as f64 + (sample as f64 + 0.5) / samples as f64;
            // All module corners at the same height/radius share a root. This
            // replaces hundreds of repeated square roots with a few per row.
            for index in &self.row_kernels[y] {
                self.roots[*index] = self.kernels[*index].root(height);
            }
            self.intersections.clear();
            for index in &self.rows[y] {
                let edge = &self.edges[*index];
                if height < edge.minimum || height >= edge.maximum {
                    continue;
                }
                let x = match edge.curve {
                    Curve::Vertical(x) => x,
                    Curve::Circle { x, kernel, sign } => x + sign * self.roots[kernel],
                };
                self.intersections.push(x.clamp(0.0, output.len() as f64));
            }
            // Boundaries keep their order between row samples. Retain a
            // fallback for any order changes near an endpoint.
            if self.intersections.windows(2).any(|pair| pair[0] > pair[1]) {
                self.intersections.sort_unstable_by(f64::total_cmp);
            }
            debug_assert!(self.intersections.len().is_multiple_of(2));
            for span in self.intersections.as_chunks::<2>().0 {
                let left = span[0];
                let right = span[1];
                let start = left.floor() as usize;
                let end = right.floor() as usize;
                if start >= output.len() {
                    continue;
                }
                if start == end {
                    self.boundary[start] += right - left;
                } else {
                    self.boundary[start] += 1.0 - left.fract();
                    if end < output.len() {
                        self.boundary[end] += right.fract();
                    }
                    self.delta[start + 1] += 1;
                    self.delta[end] -= 1;
                }
            }
        }
        let mut interior = 0;
        for (x, pixel) in output.iter_mut().enumerate() {
            interior += self.delta[x];
            let coverage = self.boundary[x] + f64::from(interior);
            *pixel = if coverage <= 0.0 {
                0
            } else if coverage >= samples as f64 {
                255
            } else {
                (coverage * 255.0 / samples as f64).round() as u8
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EcLevel, encode_text};

    #[test]
    fn all_two_by_two_connections_preserve_module_centers() {
        for mask in 0..16 {
            let mut symbol = Symbol {
                size: 21,
                modules: vec![false; 21 * 21],
                version: 1,
                mask: 0,
            };
            for index in 0..4 {
                symbol.modules[(10 + index / 2) * 21 + 10 + index % 2] = mask & (1 << index) != 0;
            }
            let mut raster = RoundedGeometry::new(&symbol, 0, 210).raster(210, 21);
            let mut row = vec![0; 210];
            for y in 0..2 {
                raster.row((10 + y) * 10 + 5, &mut row);
                for x in 0..2 {
                    assert_eq!(
                        row[(10 + x) * 10 + 5],
                        if symbol.module(10 + x, 10 + y) {
                            255
                        } else {
                            0
                        },
                        "mask {mask}"
                    );
                }
            }
        }
    }

    #[test]
    fn finder_axes_keep_square_module_pixel_boundaries_at_fractional_scales() {
        for text in ["a".to_owned(), "x".repeat(450)] {
            let symbol = encode_text(&text, EcLevel::Quartile).unwrap();
            let extent = symbol.size + 8;
            for size in [extent, extent + 1, 256, 512, 1024] {
                let mut raster = RoundedGeometry::new(&symbol, 4, size).raster(size, extent);
                let mut row = vec![0; size];
                let align = |module: usize| (module * size).div_ceil(extent);
                for (left, top) in [(4, 4), (symbol.size - 3, 4), (4, symbol.size - 3)] {
                    let center_x = (align(left + 2) + align(left + 5)) / 2;
                    let center_y = (align(top + 2) + align(top + 5)) / 2;
                    let expected = |x, y| {
                        if symbol.module(x * extent / size - 4, y * extent / size - 4) {
                            255
                        } else {
                            0
                        }
                    };
                    raster.row(center_y, &mut row);
                    for (x, pixel) in row
                        .iter()
                        .enumerate()
                        .take(align(left + 7))
                        .skip(align(left))
                    {
                        assert_eq!(*pixel, expected(x, center_y), "horizontal, {size}px");
                    }
                    for y in align(top)..align(top + 7) {
                        raster.row(y, &mut row);
                        assert_eq!(row[center_x], expected(center_x, y), "vertical, {size}px");
                    }
                }
            }
        }
    }

    #[test]
    fn rounded_symbols_preserve_encoded_dark_and_light_centers() {
        for text in [
            "a".to_owned(),
            "https://example.com/ābols?日本語=✓".to_owned(),
            "https://example.com/".repeat(30),
        ] {
            for level in [
                EcLevel::Low,
                EcLevel::Medium,
                EcLevel::Quartile,
                EcLevel::High,
            ] {
                let symbol = encode_text(&text, level).unwrap();
                let extent = symbol.size + 8;
                let mut raster =
                    RoundedGeometry::new(&symbol, 4, extent * 10).raster(extent * 10, extent);
                let mut row = vec![0; extent * 10];
                for y in 0..symbol.size {
                    raster.row((y + 4) * 10 + 5, &mut row);
                    for x in 0..symbol.size {
                        assert_eq!(
                            row[(x + 4) * 10 + 5] >= 128,
                            symbol.module(x, y),
                            "version {}, ({x}, {y})",
                            symbol.version
                        );
                    }
                }
            }
        }
    }
}
