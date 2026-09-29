# Development

Requires Node.js 26+, Rust 1.88+, and a native linker on a
[supported platform](./NATIVE-BUILDS.md).

```sh
npm install
npm run check
npm test
```

## Commands

| Command | Purpose |
| --- | --- |
| `npm run build:native` | Build and copy the release addon |
| `npm run check` | Check Rust formatting and Clippy warnings |
| `npm test` | Build the addon and run Rust/Node tests |
| `npm run test:rust` | Run Rust tests |
| `npm run test:node` | Run Node tests against the existing addon |
| `npm run bench` | Build and run benchmarks |
| `npm run example` | Build and generate example SVG/PNG files |

Restart processes using the addon after rebuilding.

## Tests

Rust tests compare complete QR matrices against `qrcodegen` across all versions
and error-correction levels. Node tests independently decode PNG output with
`jsQR` and cover validation, logos, transparency, and native replacement.

Install `rsvg-convert` to enable independent SVG rasterization tests. These
tests explicitly skip when the tool is unavailable.

## Example

```sh
npm run example -- "https://example.org/custom"
```

Writes `qr.svg` and `qr.png` to `example/output/generic/`.
The [example](./example/generate.js) creates its own logo assets in memory.

See [BENCHMARKS.md](./BENCHMARKS.md) for benchmark settings and measurements,
and [ARCHITECTURE.md](./ARCHITECTURE.md) for implementation details.
