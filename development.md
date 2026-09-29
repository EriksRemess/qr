# Development

## Requirements

- Node.js 26 or newer
- Rust 1.88 or newer and a native linker
- Linux x86-64 with glibc or Apple Silicon macOS
- Optional: `rsvg-convert` for SVG rasterization tests

## Build and test

```sh
npm install
npm run check
npm test
```

`npm test` builds the native addon, then runs the Rust and Node.js tests.
`npm run check` checks Rust formatting and Clippy warnings. To build separately:

```sh
npm run build:native
```

Builds replace the native artifact atomically. Running Node processes retain
their loaded binary and must restart to use a new build.

Tests compare QR matrices against an independent encoder and decode generated
images with `jsQR`. They also cover input validation, PNG parsing, logo layout,
transparency, and safe native replacement. SVG rasterization tests run when
`rsvg-convert` is installed and explicitly skip otherwise.

## Examples

```sh
npm run example
npm run example -- "https://example.org/custom"
```

Both commands build the addon and write `qr.svg` and `qr.png` under
`example/output/generic/`. The [example source](./example/generate.js) creates
its logo assets in memory and has no service-specific dependencies.

## Benchmarks

```sh
npm run bench
```

This builds the addon and measures SVG/PNG throughput and output sizes.
Record the hardware, Node.js version, and Rust version alongside results.
See [BENCHMARKS.md](./BENCHMARKS.md) for methodology and recorded measurements.

For encoder, renderer, and asset-parser design, see
[ARCHITECTURE.md](./ARCHITECTURE.md).
