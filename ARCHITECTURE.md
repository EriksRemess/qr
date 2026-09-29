# Architecture

## Goals

1. Minimize uncached QR generation latency for a Node.js web server.
2. Produce SVG and PNG from exactly the same encoded module matrix.
3. Support deliberate visual customization without weakening fixed QR patterns.
4. Keep runtime dependencies and the JavaScript API small.
5. Make platform support explicit rather than silently selecting a slow fallback.

## Pipeline

```text
UTF-8 text
  -> byte-mode bitstream
  -> version selection
  -> Reed-Solomon blocks
  -> interleaved codewords
  -> function-pattern matrix
  -> data placement
  -> eight mask candidates
  -> lowest-penalty symbol
  -> SVG path OR RGBA raster
  -> PNG filtering and zlib compression
```

The pipeline never encodes a PNG and decodes it again. Logos are composited
into the in-memory raster before the single PNG compression pass.

## QR encoder

`src/core.rs` implements QR Model 2 directly. The central output type is
`Symbol`, containing:

- the final dark/light module grid;
- the selected version and mask.

An internal function-module map protects finder, timing, alignment, format, and
version patterns during data placement and masking. It is discarded after
encoding; renderers only need the final module grid.

The initial encoder always uses byte mode, which works for arbitrary UTF-8
payloads. Numeric and alphanumeric segmentation are future size optimizations
and will be introduced only with differential tests and benchmarks.

The byte-mode implementation is differentially tested against `qrcodegen`, a
development-only dependency, for all 40 versions and all four error-correction
levels. The comparison covers the complete final matrix, not merely successful
decoding, so mask scoring, function-pattern placement, block interleaving, and
metadata errors cannot hide behind error correction.

## Reed-Solomon encoding

Error correction operates over GF(256) using the QR primitive polynomial
`x^8 + x^4 + x^3 + x^2 + 1` (`0x11D`). Each version/error-correction pair uses
the standard block count and error-codeword count. Short and long data blocks
are encoded independently and then interleaved as required by QR Model 2.

## Mask selection

All eight standard masks are evaluated. Scoring includes:

- long same-color runs;
- same-color 2x2 blocks;
- finder-like 1:1:3:1:1 patterns with surrounding light modules;
- deviation from 50% dark modules.

The selected mask and corresponding format bits are stored in the final symbol.

## SVG renderer

Square dark modules are coalesced into horizontal path runs. This avoids one SVG
element per module and keeps output deterministic. Square modules are the only
supported shape.

SVG strings contain no payload text and therefore do not interpolate untrusted
input into markup. Numeric dimensions and colors are parsed and validated before
rendering.

SVG logos retain their own viewport and presentation attributes. A small scanner
checks tag nesting and skips comments, processing instructions and CDATA, rather
than searching for the first literal `<svg`. It deliberately rejects DTDs; it is
not a general XML validator or sanitizer. ViewBox dimensions are bounded to
`1e-9..=1e12` and origin magnitudes to `1e12`, avoiding degenerate transforms.
Transform numbers use round-tripping decimal precision.
The normalized root viewport also has final inline `!important` geometry styles;
source CSS cannot override its dimensions or placement. XML attribute references
are decoded before parsing viewBox geometry or matching IDs, and edited values
are XML-escaped again during serialization (including referenced whitespace).
All decoded inline styles are checked for complete lexical boundaries before
compilation: unterminated comments/strings, dangling escapes, and unbalanced
blocks are rejected. Otherwise CSS EOF recovery could consume appended viewport
or stroke-only declarations, causing incorrect sizing or double-painted fills.
Quoted strings and unquoted URL tokens retain literal comment-looking text.
This is a boundary check, not a general CSS grammar/property validator.

Shape-only logos stay inline. Their definition IDs and URL references are
namespaced independently for normal and outline copies. Prefixes use a stable
FNV-1a-128 fingerprint of the compiled document, so different logos or outline
styles embedded in one parent document do not resolve each other's definitions.
Outline IDs also include the rendered stroke width: definitions containing
non-scaling strokes differ between output sizes. Identical documents may share
identical definitions; output stays deterministic without counters or randomness.
The fingerprint is an identifier, not a security boundary. CSS URL references
are decoded for hexadecimal/simple escapes before matching IDs, then emitted as
quoted, CSS-escaped strings. URL-looking text in strings/comments is left alone.
Whitespace and comments after a quoted URL argument are consumed as CSS syntax;
comment-looking text inside strings or unquoted URLs remains literal.
Logos with stylesheets use isolated, percent-encoded SVG image subdocuments; CSS selectors cannot reach
QR paths. Those image URIs are compiled once, without an added dependency.
Their image dimensions use output pixels rather than QR modules to avoid tiny
intermediate image surfaces in librsvg. Some consumers still cache these
subdocuments at nominal resolution when zooming, so callers should select the
intended output size. Inline logos do not have that limitation.

Outlines use a separately compiled vector copy with inline `!important`
stroke-only styling on drawable nodes. Clipping, masking and paint definitions
retain their fills. An explicit pass flag, rather than nonzero padding, selects
the outline: subnormal widths can underflow to zero padding. The original logo
is not repainted in the outline pass;
bitmap/foreignObject content is excluded. An expanded viewport prevents strokes
at the source viewBox edges from being clipped.
Outline nodes use `vector-effect: non-scaling-stroke`, so internal transforms do
not distort thickness. The compiled outline has a collision-checked width slot;
rendering substitutes the configured logo-unit width times the fitted pixel
scale. The normal logo remains borrowed without an extra copy. This keeps SVG
transform handling in the viewer instead of introducing a second geometry engine.

SVG and PNG share `LogoLayout`: integer logo bounds, centered placement, and a
square backing whose padding is rounded up to pixels. This keeps backing shape
and coverage identical across both formats.

## PNG renderer

The PNG writer emits a deliberately constrained format:

- PNG signature;
- one IHDR chunk;
- one IDAT chunk;
- one IEND chunk;
- 8-bit RGBA (retained for its faster measured Sub/RLE compression pattern);
- non-interlaced scanlines.

The renderer generates filtered scanlines directly, without retaining a second
full RGBA image. PNG chunk CRC-32 and zlib compression come from `zlib-rs`; PNG
chunk construction and filtering are local code. Sub filtering and RLE-oriented
compression are current performance choices and remain benchmark-controlled.
PNG logos are resized with bilinear interpolation in premultiplied-alpha space,
which preserves smooth transparent edges without introducing color fringes.
Foreground-over-background colors are composited once per render with the same
source-over semantics as SVG, then reused for every module pixel.

The logo decoder accepts 8-bit RGB/RGBA images, including RGB `tRNS` color keys.
It validates chunk checksums and ordering, rejects unknown critical chunks,
and requires consecutive IDAT chunks and a final IEND. Suggested RGB palettes
and unknown ancillary chunks are accepted without affecting pixel decoding.

## Native boundary

`QrRenderer` parses style options once in its constructor. Each render call only
receives payload text and output size. The synchronous API minimizes per-call
Node-API and Promise overhead.

Node-API provides JavaScript-engine ABI stability, but operating-system, CPU,
and libc compatibility still require distinct binaries. The loader supports:

- `linux-x64-gnu` for the production server;
- `darwin-arm64` for Apple Silicon development.

Unsupported targets fail with a precise error. There is intentionally no hidden
JavaScript or WASM fallback.

## Safety boundaries

- Raster dimensions are capped before multiplication or allocation.
- Payloads exceeding QR version 40 return an error.
- Margin and enum-like string options are validated at construction.
- Rust input paths return errors instead of indexing user-controlled lengths.
- Native output contains no filesystem or network behavior.
- PNG logo decoding checks compressed and decompressed size limits before
  allocating or inflating data.

## Dependency policy

Runtime Rust dependencies are limited to the Node-API bridge and `zlib-rs`.
General QR, image, SVG, canvas, and PNG libraries are intentionally absent.
Independent QR implementations and decoders may be used as test or benchmark
oracles without becoming production dependencies.
