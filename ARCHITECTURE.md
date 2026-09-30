# Architecture

## Source layout

| File | Responsibility |
| --- | --- |
| `index.js` | Platform detection and native addon loading |
| `src/lib.rs` | Node-API bindings and option validation |
| `src/core.rs` | QR encoding and mask selection |
| `src/render/mod.rs` | Colors, dimensions, and shared logo layout |
| `src/render/svg.rs` | SVG output |
| `src/render/png.rs` | Rasterization, compositing, and PNG output |
| `src/render/rounded.rs` | Shared rounded contours and scanline coverage |
| `src/assets.rs` | PNG logo decoding |
| `src/assets/svg.rs` | SVG logo preparation |

## Encoding

Each render call encodes UTF-8 text into a `Symbol`: the module grid, version,
and mask. Both output formats use the same encoder.

The encoder supports QR Model 2 versions 1–40 and all four error-correction
levels. It uses byte mode; numeric and alphanumeric segmentation are not supported.
Version selection chooses the smallest symbol that fits the payload.

Reed-Solomon error correction uses GF(256) with primitive polynomial `0x11D`.
Data and error-correction blocks are interleaved before module placement.
A separate function-module map protects finder, timing, alignment, format,
and version patterns.

All eight masks are scored for same-color runs, 2×2 blocks, finder-like patterns,
and dark/light balance. The lowest-penalty mask is applied to the final symbol.

## Rendering

SVG output combines adjacent dark modules into horizontal path runs.
Dimensions and colors are validated; payload text is not inserted into markup.

The optional rounded style traces connected module boundaries and rounds both
outer corners and holes with circular arcs. Diagonal-only contacts stay separate.
The three finder patterns use larger rounded rings with straight boundaries
aligned to output pixels to preserve scanner module-size estimates. SVG fills
these contours with the even-odd rule; PNG rasterizes them with antialiased
scanline coverage, sharing circular evaluations and solid-span work. The
encoded module matrix and logo layout are unchanged.

PNG output is non-interlaced, 8-bit RGBA with IHDR, IDAT, and IEND chunks.
Scanlines use the Sub filter and zlib's RLE strategy. Filtered rows are cached
by module row except where a logo needs per-pixel compositing. CRC-32 and
compression use `zlib-rs`.

Both formats composite foreground over background with source-over alpha.
They share pixel-aligned logo placement and a rectangular backing with
square corners. Padding is rounded up to whole output pixels.

## Logos

Assets are parsed once in the renderer constructor and reused.

PNG logos are decoded into RGBA pixels, resized with bilinear interpolation in
premultiplied-alpha space, and composited before PNG compression. The decoder
supports non-interlaced 8-bit RGB/RGBA and RGB `tRNS` transparency. It validates
chunk checksums, ordering, and critical chunk types.

SVG logos retain their viewBox and presentation, with root geometry overridden
to fit the configured bounds. CSS-free shape-based logos stay inline, with sizing
and outlines expressed as presentation attributes for strict CSP hosts. Logos
containing inline CSS, stylesheets, scripts, or foreignObject elements use isolated
SVG data images. These require data-image support in the viewer; some viewers cache them
at their nominal size when zooming.

Inline IDs and local references use content-derived namespaces. Normal and
outline copies have separate namespaces; outline IDs also account for rendered
stroke width. XML references and CSS URL escapes are decoded before rewriting.

Outlines are separate stroke-only copies of vector shapes and text.
Clipping, masking, and paint definitions retain their fills. An expanded viewport
allows strokes outside the source viewBox. Non-scaling strokes remove internal
transform scaling; the configured width is converted to fitted output pixels.

## Input boundaries

SVG input must be trusted and self-contained. The scanner checks tag nesting,
attribute references, and complete inline CSS boundaries; it is not an XML/CSS
validator or sanitizer. DTDs and custom entities are unsupported.

Limits enforced at the native boundary:

- Output: 21–4096 pixels; PNG also needs one pixel per module including margins.
- SVG viewBox: dimensions `1e-9..=1e12`, origin magnitudes at most `1e12`.
- PNG logos: dimensions at most 4096 pixels, compressed IDAT data at most
  16 MiB, inflated scanlines at most 64 MiB.
- Payloads exceeding version 40 capacity return an error.

The synchronous renderer has no filesystem, HTTP, or application-cache behavior.
The JavaScript loader selects a native artifact for a
[supported platform](./NATIVE-BUILDS.md); unsupported platforms fail without a
fallback renderer.

Runtime Rust dependencies are the Node-API bridge (`napi`/`napi-derive`) and
`zlib-rs`. There are no runtime npm dependencies.
