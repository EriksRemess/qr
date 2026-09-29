# `@eriksremess/qr`

Fast, customizable QR Code generation for modern Node.js. The QR encoder,
SVG renderer, rasterizer, and PNG writer are implemented as one Rust pipeline
behind a small ESM API.

This is a new implementation. It does not preserve the API or internals of
`node-qrcode`, and it does not depend on `pngjs`.

## Status

The first implementation slice supports:

- QR Model 2 versions 1 through 40
- UTF-8 byte-mode input
- L, M, Q, and H error correction
- Automatic version and mask selection
- Square or circular data modules
- Independent foreground/background colors with alpha
- SVG and PNG logos compiled once per renderer
- Configurable logo scale, backing color, and module-relative padding
- Synchronous SVG and PNG output
- Linux x86-64 GNU and Apple Silicon native builds

Planned next: gradients, additional finder styling, optimal
numeric/alphanumeric segmentation, and asynchronous PNG rendering.

## Usage

```js
import { QrRenderer } from "@eriksremess/qr";

const qr = new QrRenderer({
  errorCorrection: "high",
  foreground: "#172554",
  background: "#eff6ffff",
  margin: 4,
  moduleShape: "square",
  logoSvg: '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="#2563eb"/></svg>',
  logoScale: 0.2,
  logoBackground: "#ffffff00",
  logoPadding: 0.25,
});

const svg = qr.svg("https://example.com/qr-demo", { size: 512 });
const png = qr.png("https://example.com/qr-demo", { size: 1024 });
```

### Runnable example

Generate both formats for a neutral example under `example/output/generic/`:

```sh
npm run example
```

Pass a different payload as the final argument:

```sh
npm run example -- "https://example.org/custom"
```

The example uses square modules and creates small SVG and PNG logo inputs in
memory, so it is self-contained. The complete source is
[example/generate.js](./example/generate.js).

`QrRenderer` is immutable. Construct it once per visual style and reuse it.
The application remains responsible for caching, filesystem access, and HTTP
responses.

### API defaults

| Option | Default | Accepted values |
| --- | --- | --- |
| `errorCorrection` | `"medium"` | `"low"`, `"medium"`, `"quartile"`, `"high"` |
| `foreground` | `"#000000ff"` | Hex color forms listed below |
| `background` | `"#ffffffff"` | Hex color forms listed below |
| `margin` | `4` | Integer modules from 0 through 32 |
| `moduleShape` | `"square"` | `"square"`, `"dot"` |
| `logoScale` | `1 / 3` | Finite number from 0.05 through 0.5 |
| `logoPadding` | `0.35` | Finite module count from 0 through 4 |
| `logoBackground` | QR background | Hex color forms listed below |
| `svgLogoOutlineColor` | none | Hex color used around the SVG logo |
| `svgLogoOutlineWidth` | `0` | Width from 0 through 128 in logo viewBox units |
| output `size` | `512` | Integer pixels from 21 through 4096 |

The constructor validates and compiles styling and logo inputs. Rendering then
accepts only the payload and output size. Both methods are synchronous; this is
intentional for low per-call latency and works well behind the application LRU
cache. A future worker-pool API should be evaluated for high uncached PNG
concurrency rather than making the fast SVG path asynchronous by default.

### Colors

Colors accept `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`. The leading `#` is
optional. Alpha is preserved in both SVG and PNG output.

### Module shapes

- `square` coalesces adjacent dark modules into compact SVG scanline paths and
  produces crisp raster output.
- `dot` renders circular data modules. Finder, timing, alignment, and metadata
  modules remain square to preserve reliable scanner acquisition.

### Output limits

Output size must be between 21 and 4096 pixels. Margin must be between 0 and 32
modules. PNG output must also have at least one pixel per symbol module,
including the margin. These limits are checked before allocating the raster
buffer.

### Logos

`logoSvg` must be a trusted standalone SVG with a numeric `viewBox`. Its body
is embedded in SVG output without sanitization because the intended inputs are
application-owned brand assets, not user uploads.

Set `logoBackground` to a transparent color and `logoPadding` to zero when the
asset supplies its own backing. `svgLogoOutlineColor` and
`svgLogoOutlineWidth` add a vector outline without applying the QR root's
`crispEdges` setting to curved logo geometry. The outline options affect only
SVG; a PNG logo should contain any desired raster outline in the source image.

`logoPng` accepts non-interlaced, 8-bit RGB or RGBA PNG data. The image is
decoded and checksum-validated once when the renderer is constructed. The
decoder rejects oversized, malformed, unsupported, or checksum-invalid input.
PNG output composites the decoded pixels into the QR raster before its single
compression pass.

## Development

Requirements:

- Node.js 26 or newer
- Rust 1.88 or newer
- A native linker for the host platform

```sh
npm install
npm run build:native
npm test
npm run bench
```

On Linux x86-64 GNU, the build produces
`native/qr-native-linux-x64-gnu.node`. On Apple Silicon it produces
`native/qr-native-darwin-arm64.node`.

The package has no install script. Published packages must include the native
artifacts; developers with Rust installed can rebuild the artifact explicitly.

See [NATIVE-BUILDS.md](./NATIVE-BUILDS.md) for the two-platform artifact and
release workflow.

Publishing is handled by the GitHub Actions release workflow. It builds and
tests Linux x64 and Apple Silicon artifacts independently, combines them into
one package, and publishes to GitHub Packages with the repository-provided
`GITHUB_TOKEN`. No custom registry token is required.

## Correctness policy

A renderer benchmark is only valid when its output independently decodes. Node
tests inflate the emitted PNG through Node's zlib implementation and decode the
pixels with `jsQR`, which is independent of both the Rust QR encoder and PNG
writer. Rust tests separately compare complete byte-mode matrices against an
independent encoder for every version from 1 through 40 at all four
error-correction levels. They also cover capacities, field arithmetic, option
parsing, and image container structure.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for design and safety boundaries and
[BENCHMARKS.md](./BENCHMARKS.md) for benchmark methodology.
