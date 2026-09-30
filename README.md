# `@eriksremess/qr`

Fast PNG and SVG QR codes for Node.js, powered by Rust. Supports UTF-8 text,
square or connected rounded modules, transparent colors, and custom logos.

## Install

Requires Node.js 26 or newer on Linux x86-64 with glibc or Apple Silicon macOS.
Native binaries are included; Rust is only needed to build from source.

```sh
npm install @eriksremess/qr --registry=https://npm.pkg.github.com
```

## Usage

```js
import { QrRenderer } from "@eriksremess/qr";

const qr = new QrRenderer({
  foreground: "#172554",
  background: "#eff6ff",
  errorCorrection: "high",
});

const svg = qr.svg("https://example.com", { size: 512 }); // string
const png = qr.png("https://example.com", { size: 1024 }); // Buffer
```

Both methods are synchronous. Create a renderer once per style and reuse it.

Set `moduleStyle: "rounded"` for connected shapes with softened corners and
rounded finder rings. Existing color and logo options work with either style.
Use 512px or larger for dense rounded symbols and check the final scan result.

## Options

Pass styling options to `new QrRenderer(options)`:

| Option | Default | Values |
| --- | --- | --- |
| `errorCorrection` | `"medium"` | `"low"`, `"medium"`, `"quartile"`, `"high"` |
| `moduleStyle` | `"square"` | `"square"`, `"rounded"` |
| `foreground` | `"#000000"` | Hex color |
| `background` | `"#ffffff"` | Hex color |
| `margin` | `4` | Integer, 0–32 modules |
| `logoSvg` | none | SVG string, for SVG output |
| `logoPng` | none | PNG `Buffer`, for PNG output |
| `logoScale` | `1 / 3` | Maximum logo dimension relative to output size, 0.05–0.5 |
| `logoPadding` | `0.35` | Backing padding, 0–4 modules, rounded up to pixels |
| `logoBackground` | QR background | Hex color |
| `svgLogoOutlineColor` | none | Hex color, SVG only |
| `svgLogoOutlineWidth` | `0` | 0–128 logo viewBox units, SVG only |

Colors accept `#RGB`, `#RGBA`, `#RRGGBB`, or `#RRGGBBAA`; the `#` is optional.
Alpha is supported in both formats.

Pass `{ size }` to `.svg()` or `.png()` for a square image, defaulting to 512
pixels. Size must be an integer from 21 to 4096. PNG also requires at least one
pixel per QR module, including the margin. Invalid options and oversized payloads
throw errors.

## Logos

Supply each format separately; SVG logos are not converted to PNG automatically.

```js
import { readFileSync } from "node:fs";
import { QrRenderer } from "@eriksremess/qr";

const branded = new QrRenderer({
  errorCorrection: "high",
  logoSvg: readFileSync("logo.svg", "utf8"),
  logoPng: readFileSync("logo.png"),
  logoScale: 0.2,
  logoBackground: "#ffffff00",
  logoPadding: 0,
});
```

- SVG: trusted, self-contained markup with a numeric `viewBox`. No external
  assets or DTDs; incomplete inline CSS is rejected. This is not a sanitizer.
- PNG: non-interlaced, 8-bit RGB or RGBA. RGB `tRNS` transparency is supported.
- SVG logos containing stylesheets use embedded data images. Allow these in your
  Content Security Policy and generate at the intended display/export size.

Keep logos small and test the final QR code with a scanner; error correction
does not guarantee readability after covering modules.

For source builds, tests, examples, and benchmarks, see [development.md](./development.md).
