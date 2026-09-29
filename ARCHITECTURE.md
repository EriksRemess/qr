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
- a parallel function-module map;
- the selected version and mask.

The function map is not incidental metadata. Renderers use it to prevent style
choices such as circular modules from changing finder, timing, alignment,
format, or version patterns.

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
element per module and keeps output deterministic. Dot mode emits circles as
subpaths for data modules while fixed QR patterns remain in the square path.

SVG strings contain no payload text and therefore do not interpolate untrusted
input into markup. Numeric dimensions and colors are parsed and validated before
rendering.

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
