import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { crc32, deflateSync, inflateSync } from "node:zlib";
import test from "node:test";

import jsQR from "jsqr";

import { QrRenderer } from "../index.js";

function embeddedSvgs(svg) {
  return [...svg.matchAll(/href="data:image\/svg\+xml,([^"]+)"/gu)]
    .map((match) => decodeURIComponent(match[1]));
}

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

/**
 * Decode the deliberately small PNG subset emitted by this package. Keeping
 * this reader in the test suite makes structural assertions independent of the
 * production encoder and avoids adding a general PNG library to runtime code.
 */
function decodeRgbaPng(png) {
  assert.deepStrictEqual(png.subarray(0, 8), PNG_SIGNATURE);
  let offset = 8;
  let width;
  let height;
  let headerColorType;
  const idat = [];

  while (offset < png.length) {
    const length = png.readUInt32BE(offset);
    const type = png.toString("ascii", offset + 4, offset + 8);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    assert.ok(dataEnd + 4 <= png.length, `truncated ${type} chunk`);
    if (type === "IHDR") {
      width = png.readUInt32BE(dataStart);
      height = png.readUInt32BE(dataStart + 4);
      assert.equal(png[dataStart + 8], 8, "expected 8-bit PNG output");
      assert.ok([2, 6].includes(png[dataStart + 9]), "expected RGB or RGBA PNG output");
      headerColorType = png[dataStart + 9];
      assert.equal(png[dataStart + 12], 0, "expected non-interlaced PNG output");
    } else if (type === "IDAT") {
      idat.push(png.subarray(dataStart, dataEnd));
    } else if (type === "IEND") {
      break;
    }
    offset = dataEnd + 4;
  }

  assert.ok(Number.isInteger(width) && Number.isInteger(height));
  const channels = headerColorType === 6 ? 4 : 3;
  const filtered = inflateSync(Buffer.concat(idat));
  const rowBytes = width * channels;
  assert.equal(filtered.length, (rowBytes + 1) * height);
  const raw = new Uint8Array(width * height * channels);

  for (let y = 0; y < height; y += 1) {
    const sourceStart = y * (rowBytes + 1);
    const filter = filtered[sourceStart];
    assert.ok(filter <= 4, "expected a standard PNG filter");
    for (let x = 0; x < rowBytes; x += 1) {
      const left = x >= channels ? raw[y * rowBytes + x - channels] : 0;
      const up = y > 0 ? raw[(y - 1) * rowBytes + x] : 0;
      const upperLeft = y > 0 && x >= channels ? raw[(y - 1) * rowBytes + x - channels] : 0;
      const prediction = left + up - upperLeft;
      const distances = [left, up, upperLeft].map((value) => Math.abs(prediction - value));
      const paeth = [left, up, upperLeft][distances.indexOf(Math.min(...distances))];
      const adjustment = [0, left, up, Math.floor((left + up) / 2), paeth][filter];
      raw[y * rowBytes + x] = (filtered[sourceStart + 1 + x] + adjustment) & 0xff;
    }
  }

  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    rgba[pixel * 4] = raw[pixel * channels];
    rgba[pixel * 4 + 1] = raw[pixel * channels + 1];
    rgba[pixel * 4 + 2] = raw[pixel * channels + 2];
    rgba[pixel * 4 + 3] = channels === 4 ? raw[pixel * channels + 3] : 255;
  }

  return { data: rgba, height, width };
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const checksumInput = Buffer.concat([typeBytes, data]);
  const chunk = Buffer.allocUnsafe(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(checksumInput), 8 + data.length);
  return chunk;
}

function createLogoPng() {
  const width = 48;
  const height = 20;
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  const scanlines = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const row = y * (width * 4 + 1);
    for (let x = 0; x < width; x += 1) {
      const pixel = row + 1 + x * 4;
      scanlines[pixel] = 37;
      scanlines[pixel + 1] = 99;
      scanlines[pixel + 2] = 235;
      scanlines[pixel + 3] = x < 2 || x >= width - 2 || y < 2 || y >= height - 2 ? 0 : 255;
    }
  }
  return Buffer.concat([
    PNG_SIGNATURE,
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(scanlines)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

test("native PNG round-trips UTF-8 payload with square modules", () => {
  const text = "https://example.com/ābols?日本語=✓";
  const renderer = new QrRenderer({
    background: "#ffffffff",
    errorCorrection: "high",
    foreground: "#172554ff",
    margin: 4,
  });
  const decodedPng = decodeRgbaPng(renderer.png(text, { size: 512 }));
  const decodedQr = jsQR(decodedPng.data, decodedPng.width, decodedPng.height, {
    inversionAttempts: "attemptBoth",
  });
  assert.ok(decodedQr, "jsQR should locate the generated symbol");
  assert.equal(decodedQr.data, text);
});

test("PNG and SVG logos are compiled once and remain independently decodable", () => {
  const text = "https://example.com/branded";
  const renderer = new QrRenderer({
    errorCorrection: "high",
    foreground: "#172554",
    logoBackground: "#ffffff",
    logoPadding: 0.5,
    logoPng: createLogoPng(),
    logoScale: 0.22,
    logoSvg: '<svg viewBox="0 0 48 20"><rect width="48" height="20" rx="3" fill="#2563eb"/></svg>',
  });

  const decodedPng = decodeRgbaPng(renderer.png(text, { size: 512 }));
  const decodedQr = jsQR(decodedPng.data, decodedPng.width, decodedPng.height, {
    inversionAttempts: "attemptBoth",
  });
  assert.ok(decodedQr, "jsQR should locate the branded symbol");
  assert.equal(decodedQr.data, text);

  const svg = renderer.svg(text, { size: 512 });
  assert.match(svg, /<g transform=/u);
  assert.match(svg, /<rect width="48" height="20"/u);
});

test("SVG output contains the configured dimensions and colors", () => {
  const renderer = new QrRenderer({
    background: "#abcdef80",
    foreground: "#123456",
  });
  const svg = renderer.svg("https://example.com/svg", { size: 320 });
  assert.match(svg, /width="320" height="320"/u);
  assert.match(svg, /fill="#abcdef" fill-opacity="0\.5019607843137255"/u);
  assert.match(svg, /fill="#123456"/u);
  assert.match(svg, /shape-rendering="crispEdges"/u);
  assert.doesNotMatch(svg, /<circle|a\.5\.5/u);
});

test("square is the default and unsupported module styles fail validation", () => {
  const implicit = new QrRenderer();
  const explicit = new QrRenderer({ moduleStyle: "square" });
  assert.equal(implicit.svg("default"), explicit.svg("default"));
  assert.deepEqual(implicit.png("default"), explicit.png("default"));
  for (const moduleStyle of ["dots", "round", "", "Rounded"]) {
    assert.throws(() => new QrRenderer({ moduleStyle }), /moduleStyle must be square or rounded/u);
  }
});

test("rounded PNG symbols decode at several sizes, payloads and correction levels", () => {
  for (const errorCorrection of ["low", "medium", "quartile", "high"]) {
    const renderer = new QrRenderer({ moduleStyle: "rounded", errorCorrection });
    for (const text of ["a", "https://example.com/rounded", "https://example.com/ābols?日本語=✓", "https://example.com/".repeat(12)]) {
      for (const size of text.length > 100 ? [512, 1024] : [256, 512]) {
        const png = decodeRgbaPng(renderer.png(text, { size }));
        assert.equal(jsQR(png.data, png.width, png.height)?.data, text, `${errorCorrection}, ${size}px, ${text}`);
      }
    }
  }
});

function* roundedFinderRegressions() {
  yield { errorCorrection: "quartile", text: "x".repeat(450) };
  // Retain failing dense URL layouts as compact, deterministic fixtures.
  for (const [errorCorrection, length, initialSeed] of [
    ["quartile", 453, 3_882_932_285],
    ["high", 348, 2_981_535_062],
    ["quartile", 459, 3_941_585_329],
    ["high", 350, 3_002_117_140],
  ]) {
    let seed = initialSeed;
    let text = "https://example.org/";
    while (text.length < length) {
      seed = (Math.imul(seed, 1_664_525) + 1_013_904_223) >>> 0;
      text += "abcdefghijklmnopqrstuvwxyz0123456789"[Math.floor(seed / 2 ** 32 * 36)];
    }
    yield { errorCorrection, text };
  }
}

test("dense rounded PNG finder patterns retain the correct scan dimension", () => {
  for (const { errorCorrection, text } of roundedFinderRegressions()) {
    const renderer = new QrRenderer({ errorCorrection, moduleStyle: "rounded" });
    for (const size of [512, 1024]) {
      const png = decodeRgbaPng(renderer.png(text, { size }));
      assert.equal(jsQR(png.data, size, size)?.data, text, `${errorCorrection}, ${text.length} bytes, ${size}px`);
    }
  }
});

test("rounded finder rings and module edges retain their centers and alpha", () => {
  const renderer = new QrRenderer({ moduleStyle: "rounded", foreground: "#ff000080", background: "#0000" });
  const png = decodeRgbaPng(renderer.png("a", { size: 290 }));
  const at = (x, y) => [...png.data.slice((y * 290 + x) * 4, (y * 290 + x) * 4 + 4)];
  assert.equal(at(40, 40)[3], 0, "rounded outer finder corner");
  assert.deepEqual(at(75, 75), [255, 0, 0, 128], "finder center");
  assert.equal(at(55, 75)[3], 0, "white finder ring");
  assert.ok(png.data.some((value, index) => index % 4 === 3 && value > 0 && value < 128), "antialiased curved edges");
  for (let i = 3; i < png.data.length; i += 4) { assert.ok(png.data[i] <= 128, "connected regions paint once"); }
  assert.match(renderer.svg("a"), /fill-rule="evenodd"/u);
  assert.match(renderer.svg("a"), /A1\.5 1\.5/u);
});

test("non-ASCII and malformed colors throw instead of aborting Node", () => {
  for (const value of ["#💥aa", "💥", "#ééé", "#１２", "#12345", "#12 456", "#zzzzzz", ""]) {
    for (const option of ["foreground", "background", "logoBackground", "svgLogoOutlineColor"]) {
      assert.throws(() => new QrRenderer({ [option]: value }), /invalid color/u);
    }
  }
});

test("PNG foreground alpha uses SVG source-over semantics", () => {
  for (const [foreground, background, expected] of [
    ["#ff000080", "#0000ffff", [128, 0, 127, 255]],
    ["#ff000080", "#0000ff80", [170, 0, 85, 192]],
    ["#ff000000", "#0000ff80", [0, 0, 255, 128]],
    ["#ff000080", "#0000ff00", [255, 0, 0, 128]],
    ["#ff0000ff", "#0000ff80", [255, 0, 0, 255]],
  ]) {
    const renderer = new QrRenderer({ foreground, background, margin: 0 });
    const { data } = decodeRgbaPng(renderer.png("alpha", { size: 210 }));
    // The top-left finder corner is always dark, independent of the mask.
    assert.deepEqual([...data.slice(0, 4)], expected, `${foreground} over ${background}`);
  }
});

test("SVG logos retain root presentation attributes and exact scale", () => {
  const renderer = new QrRenderer({
    logoSvg: '<svg width="300" height="200" viewBox="0 0 1000000 1000000" fill="red" style="opacity:.5" xmlns:xlink="http://www.w3.org/1999/xlink" data-label="a > b"><style/><rect width="1000000" height="1000000"/></svg>',
  });
  const svg = renderer.svg("logo");
  const logo = embeddedSvgs(svg)[0];
  assert.match(logo, /fill="red"/u);
  assert.match(logo, /style="opacity:\.5;/u);
  assert.match(logo, /xmlns:xlink=/u);
  assert.match(logo, /data-label="a > b"/u);
  assert.match(logo, /viewBox="0 0 1000000 1000000"/u);
  const width = Number(svg.match(/<image[^>]+ width="([^"]+)"/u)[1]);
  assert.ok(width > 0);
  assert.equal(width, Math.round(512 / 3));
  assert.doesNotMatch(svg, /NaN|Infinity|scale\(0\)/u);
});

test("SVG scanning ignores markup in comments, instructions, CDATA and quoted attributes", () => {
  const logoSvg = '\uFEFF<?xml version="1.0"?><!-- <svg viewBox="0 0 1 1"> -->' +
    '<svg viewBox="0 0 10 10" data-label="a > b"><style><![CDATA[/* <svg> */ rect {fill:red}]]></style>' +
    '<rect width="10" height="10"/></svg><!-- </svg> -->';
  const embedded = embeddedSvgs(new QrRenderer({ logoSvg }).svg("scan"))[0];
  assert.match(embedded, /viewBox="0 0 10 10"/u);
  assert.match(embedded, /<!\[CDATA\[/u);
  assert.match(embedded, /<rect width="10" height="10"/u);
  for (const malformed of [
    '<svg viewBox="0 0 1 1"><path></svg>',
    '<svg viewBox="0 0 1 1"></svg><svg viewBox="0 0 1 1"/>',
    '<!-- unclosed <svg viewBox="0 0 1 1"></svg>',
    '<!DOCTYPE svg><svg viewBox="0 0 1 1"/>',
  ]) {
    assert.throws(() => new QrRenderer({ logoSvg: malformed }), /invalid SVG logo/u);
  }
});

test("SVG logo geometry rejects degenerate transforms and false attribute matches", () => {
  for (const attributes of [
    'viewBox="0 0 5e-324 5e-324"',
    'viewBox="0 0 1e309 1"',
    'viewBox="1e15 0 1 1"',
    'viewBox="0 0 -1 1"',
    'data-viewBox="0 0 1 1"',
    'viewBox="0 0 1 1" viewBox="0 0 2 2"',
    'viewBox="0 0 1 1"fill="red"',
  ]) {
    assert.throws(() => new QrRenderer({ logoSvg: `<svg ${attributes}><path/></svg>` }), /invalid SVG logo/u);
  }
});

function tinyLogoChunks(colorType = 2) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(1, 0);
  header.writeUInt32BE(1, 4);
  header[8] = 8;
  header[9] = colorType;
  return {
    header: pngChunk("IHDR", header),
    data: pngChunk("IDAT", deflateSync(Buffer.from(colorType === 2 ? [0, 255, 0, 0] : [0, 255, 0, 0, 255]))),
    end: pngChunk("IEND", Buffer.alloc(0)),
    key: pngChunk("tRNS", Buffer.from([0, 255, 0, 0, 0, 0])),
  };
}

test("RGB PNG logos honor exact tRNS keys", () => {
  const { header, data, end, key } = tinyLogoChunks();
  for (const [transparency, expected] of [
    [key, [0, 255, 0, 255]],
    [pngChunk("tRNS", Buffer.from([0, 254, 0, 0, 0, 0])), [255, 0, 0, 255]],
    [Buffer.alloc(0), [255, 0, 0, 255]],
  ]) {
    const renderer = new QrRenderer({
      foreground: "#00ff00", background: "#00ff00", logoBackground: "#0000",
      logoPng: Buffer.concat([PNG_SIGNATURE, header, transparency, data, end]),
    });
    const png = decodeRgbaPng(renderer.png("key", { size: 210 }));
    const center = (105 * png.width + 105) * 4;
    assert.deepEqual([...png.data.slice(center, center + 4)], expected);
  }
});

test("PNG logo parser validates chunk ordering, critical chunks and transparency", () => {
  const { header, data, end, key } = tinyLogoChunks();
  const ancillary = pngChunk("tEXt", Buffer.from("key\0value"));
  const palette = pngChunk("PLTE", Buffer.from([255, 0, 0]));
  for (const chunks of [
    [data, header, end],
    [ancillary, header, data, end],
    [header, header, data, end],
    [header, data, ancillary, data, end],
    [header, pngChunk("ABCD", Buffer.alloc(0)), data, end],
    [header, pngChunk("abcd", Buffer.alloc(0)), data, end],
    [header, data, key, end],
    [header, key, key, data, end],
    [header, pngChunk("tRNS", Buffer.from([0, 255])), data, end],
    [header, pngChunk("tRNS", Buffer.from([1, 0, 0, 0, 0, 0])), data, end],
    [tinyLogoChunks(6).header, key, tinyLogoChunks(6).data, end],
    [header, data, palette, end],
    [header, palette, palette, data, end],
    [header, key, palette, data, end],
    [header, end],
    [header, data],
    [header, data, end, ancillary],
    [header, data, end, Buffer.from([0])],
  ]) {
    assert.throws(() => new QrRenderer({ logoPng: Buffer.concat([PNG_SIGNATURE, ...chunks]) }), /invalid PNG logo/u);
  }
  // Ancillary chunks and a suggested truecolor palette are legal. Adjacent
  // empty IDAT chunks are legal too, unlike an interrupted IDAT sequence.
  assert.doesNotThrow(() => new QrRenderer({ logoPng: Buffer.concat([
    PNG_SIGNATURE, header, palette, key, ancillary,
    pngChunk("IDAT", Buffer.alloc(0)), data, ancillary, end,
  ]) }));
});

// Optional independent rasterizer: no npm/runtime dependency. These tests run
// wherever librsvg's CLI is installed and explicitly skip otherwise.
const hasSvgRasterizer = spawnSync("rsvg-convert", ["--version"], { timeout: 5_000 }).status === 0;
function rasterizeSvg(svg) {
  const directory = mkdtempSync(join(tmpdir(), "qr-raster-test-"));
  try {
    const path = join(directory, "input.svg");
    writeFileSync(path, svg);
    const result = spawnSync("rsvg-convert", [path], {
      timeout: 10_000, maxBuffer: 16 * 1024 * 1024,
    });
    assert.equal(result.status, 0, result.error?.message ?? result.stderr?.toString());
    return decodeRgbaPng(result.stdout);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("dense rounded SVG finder patterns retain the correct scan dimension", { skip: !hasSvgRasterizer }, () => {
  for (const { errorCorrection, text } of roundedFinderRegressions()) {
    const renderer = new QrRenderer({ errorCorrection, moduleStyle: "rounded" });
    for (const size of [512, 1024]) {
      const svg = rasterizeSvg(renderer.svg(text, { size }));
      assert.equal(jsQR(svg.data, size, size)?.data, text, `${errorCorrection}, ${text.length} bytes, ${size}px`);
    }
  }
});

test("rounded SVG and PNG share shape geometry and remain independently decodable with logos", { skip: !hasSvgRasterizer }, () => {
  const text = "https://example.com/rounded-logo";
  for (const branded of [false, true]) {
    const renderer = new QrRenderer({
      moduleStyle: "rounded", errorCorrection: "high", foreground: "#172554",
      logoScale: 0.18,
      ...(branded ? { logoSvg: '<svg viewBox="0 0 48 20"><rect width="48" height="20" rx="4" fill="#2563eb"/></svg>', logoPng: createLogoPng() } : {}),
    });
    for (const size of [290, 512, 1024]) {
      const png = decodeRgbaPng(renderer.png(text, { size }));
      const svg = rasterizeSvg(renderer.svg(text, { size }));
      assert.equal(jsQR(png.data, png.width, png.height)?.data, text, `PNG, ${size}, branded ${branded}`);
      assert.equal(jsQR(svg.data, svg.width, svg.height)?.data, text, `SVG, ${size}, branded ${branded}`);
      if (!branded) {
        let difference = 0;
        for (let offset = 0; offset < png.data.length; offset++) { difference += Math.abs(png.data[offset] - svg.data[offset]); }
        assert.ok(difference / png.data.length < 1, `matching rasterized contours, size ${size}`);
      }
    }
  }
});

test("SVG and PNG alpha match after independent SVG rasterization", { skip: !hasSvgRasterizer }, () => {
  for (const [foreground, background] of [
    ["#ff000080", "#0000ffff"],
    ["#ff000080", "#0000ff80"],
    ["#ff000080", "#0000ff00"],
  ]) {
    const renderer = new QrRenderer({ foreground, background });
    const png = decodeRgbaPng(renderer.png("alpha", { size: 290 }));
    const svg = rasterizeSvg(renderer.svg("alpha", { size: 290 }));
    assert.equal(png.data.length, svg.data.length);
    for (let offset = 0; offset < png.data.length; offset += 4) {
      // Fully transparent RGB values are immaterial. Cairo uses 8-bit
      // premultiplication, so compare premultiplied colors within one byte.
      const pngAlpha = png.data[offset + 3];
      const svgAlpha = svg.data[offset + 3];
      assert.ok(Math.abs(pngAlpha - svgAlpha) <= 1);
      for (let channel = 0; channel < 3; channel += 1) {
        assert.ok(Math.abs(png.data[offset + channel] * pngAlpha / 255
          - svg.data[offset + channel] * svgAlpha / 255) <= 1);
      }
    }
  }
});

test("root-styled and large-viewBox SVG logos render correctly", { skip: !hasSvgRasterizer }, () => {
  for (const logoSvg of [
    '<svg viewBox="0 0 10 10" fill="red"><rect width="10" height="10"/></svg>',
    '<svg viewBox="0 0 10 10" style="fill:red"><rect width="10" height="10"/></svg>',
    '<svg viewBox="0 0 1000000 1000000" fill="red"><rect width="1000000" height="1000000"/></svg>',
    '<svg viewBox="100 200 10 10" fill="red"><rect x="100" y="200" width="10" height="10"/></svg>',
  ]) {
    const renderer = new QrRenderer({ logoSvg, errorCorrection: "high", logoScale: 0.2 });
    const svg = rasterizeSvg(renderer.svg("https://example.com/logo", { size: 512 }));
    const center = (256 * svg.width + 256) * 4;
    assert.deepEqual([...svg.data.slice(center, center + 4)], [255, 0, 0, 255]);
    const decoded = jsQR(svg.data, svg.width, svg.height);
    assert.equal(decoded?.data, "https://example.com/logo");
  }
});

test("embedded SVG CSS cannot recolor QR paths", { skip: !hasSvgRasterizer }, () => {
  const logoSvg = '<!-- <svg viewBox="0 0 1 1"> -->' +
    '<svg viewBox="0 0 10 10"><style>path {fill:red!important}</style><path d="M0 0h10v10H0z"/></svg>';
  const renderer = new QrRenderer({ logoSvg, logoScale: 0.2, errorCorrection: "high" });
  const svg = renderer.svg("https://example.com/css", { size: 512 });
  assert.doesNotMatch(svg, /<style>/u);
  const pixels = rasterizeSvg(svg);
  const color = (x, y) => [...pixels.data.slice((y * pixels.width + x) * 4, (y * pixels.width + x) * 4 + 4)];
  assert.deepEqual(color(0, 0), [255, 255, 255, 255]);
  assert.deepEqual(color(256, 256), [255, 0, 0, 255]);
  assert.equal(jsQR(pixels.data, pixels.width, pixels.height)?.data, "https://example.com/css");
});

test("incomplete inline CSS is rejected before renderer overrides are appended", () => {
  const invalidStyles = [
    "fill:red;fill-opacity:.5;/*", "fill:red;&#47;* unfinished",
    "fill:red;--note:'unfinished", "fill:red;--note:&quot;unfinished",
    "fill:red;--note:func(", "fill:red;--note:[value", "fill:red;--note:{value",
    "fill:red;--note:([)]", "fill:red;--note:value\\", "fill:url('#paint'/*)",
  ];
  for (const style of invalidStyles) {
    for (const rootStyle of [false, true]) {
      for (const isolated of [false, true]) {
        for (const svgLogoOutlineWidth of [0, 1]) {
          const logoSvg = `<svg viewBox="0 0 10 10"${rootStyle ? ` style="${style}"` : ""}>` +
            `${isolated ? "<style/>" : ""}<rect width="10" height="10"${rootStyle ? "" : ` style="${style}"`}/></svg>`;
          assert.throws(() => new QrRenderer({ logoSvg, svgLogoOutlineColor: "#fff", svgLogoOutlineWidth }), /invalid SVG logo/u);
        }
      }
    }
  }
});

test("complete inline comments and quoted comment text preserve translucent fills", { skip: !hasSvgRasterizer }, () => {
  for (const extra of ["/**/", "--note:'/* not a comment'", "--note:func([{}]);/* done */"]) {
    for (const rootStyle of [false, true]) {
      for (const isolated of [false, true]) {
        const style = `fill:red;fill-opacity:.5;${extra}`;
        const logoSvg = `<svg viewBox="0 0 10 10"${rootStyle ? ` style="${style}"` : ""}>` +
          `${isolated ? "<style/>" : ""}<rect width="10" height="10"${rootStyle ? "" : ` style="${style}"`}/></svg>`;
        const renderer = new QrRenderer({
          foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoSvg,
          svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1,
        });
        const pixels = rasterizeSvg(renderer.svg("test", { size: 290 }));
        const center = (145 * 290 + 145) * 4;
        assert.deepEqual([...pixels.data.slice(center, center + 4)], [128, 0, 127, 255]);
      }
    }
  }
});

test("SVG outlines override explicit strokes without repainting translucent fills", { skip: !hasSvgRasterizer }, () => {
  for (const attrs of ['fill="red" fill-opacity="0.5" stroke="none"', 'style="fill:red;fill-opacity:.5;stroke:none!important"']) {
    const logoSvg = `<svg viewBox="0 0 10 10" ${attrs}><rect x="1" y="1" width="8" height="8" ${attrs}/></svg>`;
    const base = { foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3, logoSvg };
    const plain = rasterizeSvg(new QrRenderer(base).svg("test", { size: 290 }));
    const renderer = new QrRenderer({ ...base, svgLogoOutlineColor: "#ffffff", svgLogoOutlineWidth: 1 });
    const outlined = rasterizeSvg(renderer.svg("test", { size: 290 }));
    const center = (145 * 290 + 145) * 4;
    assert.deepEqual([...outlined.data.slice(center, center + 4)], [...plain.data.slice(center, center + 4)]);
    const stroke = (145 * 290 + 108) * 4;
    assert.deepEqual([...outlined.data.slice(stroke, stroke + 4)], [255, 255, 255, 255]);
    const output = renderer.svg("test");
    if (attrs.startsWith("style=")) {
      assert.match(embeddedSvgs(output).join(""), /fill:none!important;stroke:#ffffff!important/u);
    } else {
      assert.match(output, /fill="none" stroke="#ffffff"/u);
      assert.doesNotMatch(output, /\sstyle=/u);
    }
  }
});

test("inline SVG logos and outlines need no CSP style or data-image allowances", () => {
  const logoSvg = '<svg viewBox="8 16 24 12" fill="red" overflow="hidden">' +
    '<defs><linearGradient id="paint"><stop stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>' +
    '<clipPath id="clip"><rect x="8" y="16" width="24" height="12" fill="white"/></clipPath></defs>' +
    '<path d="M10 18h20v8H10z" fill="url(#paint)" stroke="none" stroke-width="99" stroke-opacity="0" ' +
    'vector-effect="none" stroke-linejoin="bevel" stroke-linecap="butt" clip-path="url(#clip)"/>' +
    '<circle cx="28" cy="24" r="2" fill="pink"/></svg>';
  for (const moduleStyle of ["square", "rounded"]) {
    for (const svgLogoOutlineWidth of [0, 2]) {
      const output = new QrRenderer({
        logoSvg, moduleStyle, svgLogoOutlineWidth, svgLogoOutlineColor: "#1e1e2e",
      }).svg("https://example.com/csp", { size: 512 });
      assert.doesNotMatch(output, /\sstyle=|<style\b|<image\b/u);
      assert.match(output, /fill="url\(&quot;#qr-logo-main-/u);
      if (svgLogoOutlineWidth) {
        assert.match(output, /fill="none" stroke="#1e1e2e" stroke-opacity="1" stroke-width="[\d.]+"/u);
        assert.match(output, /vector-effect="none" stroke-linejoin="round" stroke-linecap="round"/u);
        assert.match(output, /<symbol[^>]+overflow="visible"/u);
        const outline = output.match(/<path[^>]+fill="none"[^>]+>/u)[0];
        for (const attribute of ["fill", "stroke", "stroke-width", "stroke-opacity", "vector-effect"]) {
          assert.equal([...outline.matchAll(new RegExp(`\\s${attribute}=`, "gu"))].length, 1, attribute);
        }
      }
    }
  }
});

test("source inline CSS stays isolated rather than leaking into the host CSP", () => {
  for (const logoSvg of [
    '<svg viewBox="0 0 10 10" style="fill:red"><rect width="10" height="10"/></svg>',
    '<svg viewBox="0 0 10 10"><rect width="10" height="10" style="fill:red"/></svg>',
  ]) {
    const output = new QrRenderer({ logoSvg, svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1 }).svg("css");
    assert.doesNotMatch(output, /\sstyle=|<style\b/u);
    assert.equal(embeddedSvgs(output).length, 2);
    assert.match(embeddedSvgs(output)[1], /style="[^" ]*fill:red/u);
  }
});

test("SVG logo outlines scale with the displayed QR without changing logo size", { skip: !hasSvgRasterizer }, () => {
  for (const body of [
    '<rect x="1" y="1" width="8" height="8" fill="red"/>',
    '<g transform="translate(5 5) scale(.2 .1)"><circle r="20" fill="red"/></g>',
    '<svg width="10" height="10" viewBox="0 0 100 100"><rect x="10" y="10" width="80" height="80" fill="red"/></svg>',
    '<style/><rect x="1" y="1" width="8" height="8" fill="red"/>',
  ]) {
    const renderer = new QrRenderer({
      foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
      logoSvg: `<svg viewBox="0 0 10 10">${body}</svg>`,
      svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1,
    });
    const intrinsic = renderer.svg("test", { size: 400 });
    for (const size of [200, 400, 800]) {
      const resized = rasterizeSvg(intrinsic.replace('width="400" height="400"', `width="${size}" height="${size}"`));
      const direct = rasterizeSvg(renderer.svg("test", { size }));
      // The filled logo covers the inner half of the stroke. Its remaining
      // white outline is 0.5 logo units, scaled by the fitted logo width.
      const expectedWhite = size * 0.015;
      const expectedRed = size * 0.24;
      for (const pixels of [resized, direct]) {
        let whiteCoverage = 0;
        let red = 0;
        for (let x = 0; x < size; x++) {
          const offset = ((size / 2) * size + x) * 4;
          const [r, g, b] = pixels.data.slice(offset, offset + 3);
          // Include antialiased edge coverage: an isolated SVG image can be
          // cached at its nominal resolution by the independent rasterizer.
          if (x < size / 2) whiteCoverage += g / 255;
          if (r >= 250 && g <= 5 && b <= 5) red++;
        }
        assert.ok(Math.abs(whiteCoverage - expectedWhite) <= 1, `outline at ${size}px: ${whiteCoverage}, expected ${expectedWhite}`);
        assert.ok(Math.abs(red - expectedRed) <= 2, `logo at ${size}px: ${red}, expected ${expectedRed}`);
      }
      if (!body.includes("transform") && !body.includes("style") && !body.includes("<svg")) {
        assert.deepEqual(resized.data, direct.data, "ordinary vector logos remain crisp when resized");
      }
    }
  }
});

test("SVG outline viewport includes strokes outside the source viewBox", { skip: !hasSvgRasterizer }, () => {
  const renderer = new QrRenderer({
    foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
    logoSvg: '<svg viewBox="10 20 10 10" overflow="hidden"><circle cx="15" cy="25" r="5" fill="red" stroke="none"/></svg>',
    svgLogoOutlineColor: "#ffffff", svgLogoOutlineWidth: 1,
  });
  const png = rasterizeSvg(renderer.svg("test", { size: 290 }));
  const offset = (145 * 290 + 99) * 4; // Left of the original 101px logo boundary.
  assert.deepEqual([...png.data.slice(offset, offset + 4)], [255, 255, 255, 255]);
});

test("host SVG overflow rules do not crop inline logo outlines", { skip: !hasSvgRasterizer }, () => {
  for (const logoSvg of [
    '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="red"/></svg>',
    '<svg id="viewport" viewBox="10 20 10 10"><circle cx="15" cy="25" r="5" fill="red"/></svg>',
    '<svg viewBox="0 0 10 10"><rect width="100%" height="100%" fill="red"/></svg>',
  ]) {
    const renderer = new QrRenderer({
      foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
      logoSvg, svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1,
    });
    for (const size of [200, 400, 800]) {
      const svg = renderer.svg("test", { size });
      const hosted = svg.replace("><path", "><style>svg{overflow:hidden}</style><path");
      assert.deepEqual(rasterizeSvg(hosted).data, rasterizeSvg(svg).data, `outline at ${size}px`);
    }
  }
});

test("inline logo outlines preserve root presentation effects", { skip: !hasSvgRasterizer }, () => {
  const body = '<defs><clipPath id="clip"><circle cx="5" cy="5" r="4"/></clipPath>' +
    '<mask id="mask"><rect width="10" height="10" fill="white"/></mask>' +
    '<filter id="filter"><feComponentTransfer><feFuncA type="linear" slope=".5"/></feComponentTransfer></filter>' +
    '</defs><rect width="100%" height="100%" fill="red"/>';
  for (const effects of ['opacity=".5"', 'clip-path="url(#clip)"', 'mask="url(#mask)"', 'filter="url(#filter)"', 'display="none"']) {
    const render = (rootEffects, groupEffects, size, hosted) => {
      const svg = new QrRenderer({
        foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
        logoSvg: `<svg id="viewport" viewBox="0 0 10 10" ${rootEffects}><g ${groupEffects}>${body}</g></svg>`,
        svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1,
      }).svg("test", { size });
      assert.doesNotMatch(svg, /\sstyle=|<style\b|<image\b/u);
      return rasterizeSvg(hosted ? svg.replace("><path", "><style>svg{overflow:hidden}</style><path") : svg).data;
    };
    for (const size of [200, 400]) {
      const expected = render("", effects, size, false);
      for (const hosted of [false, true]) {
        assert.deepEqual(render(effects, "", size, hosted), expected, `${effects} at ${size}px, hosted ${hosted}`);
      }
    }
  }
});

test("inline logo and outline definitions have independent IDs and references", { skip: !hasSvgRasterizer }, () => {
  const renderer = new QrRenderer({
    foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
    logoSvg: '<svg viewBox="0 0 10 10"><defs><rect id="shape" x="1" y="1" width="8" height="8" fill="red"/>' +
      '<clipPath id="clip"><rect width="10" height="10"/></clipPath></defs>' +
      '<use href="#shape" clip-path="url( #clip )"/></svg>',
    svgLogoOutlineColor: "#ffffff", svgLogoOutlineWidth: 1,
  });
  const svg = renderer.svg("test", { size: 290 });
  assert.doesNotMatch(svg, /<image /u);
  for (const prefix of ["qr-logo-main-", "qr-logo-outline-"]) {
    const scoped = svg.match(new RegExp(`id="(${prefix}[^" ]*-)shape"`, "u"))?.[1];
    assert.ok(scoped);
    assert.ok(svg.includes(`href="#${scoped}shape"`));
    assert.ok(svg.includes(`url(&quot;#${scoped}clip&quot;)`));
  }
  const ids = [...svg.matchAll(/\bid="([^"]+)"/gu)].map((match) => match[1]);
  assert.equal(ids.length, new Set(ids).size);
  const png = rasterizeSvg(svg);
  const center = (145 * 290 + 145) * 4;
  assert.deepEqual([...png.data.slice(center, center + 4)], [255, 0, 0, 255]);
  const stroke = (145 * 290 + 108) * 4;
  assert.deepEqual([...png.data.slice(stroke, stroke + 4)], [255, 255, 255, 255]);
});

test("different inline QR logos and outline sizes do not share definitions", { skip: !hasSvgRasterizer }, () => {
  const base = { foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3 };
  const draw = (color, width, size = 290) => new QrRenderer({
    ...base, svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: width,
    logoSvg: `<svg viewBox="0 0 10 10"><defs><linearGradient id="paint"><stop stop-color="${color}"/></linearGradient>` +
      '<rect id="shape" x="1" y="1" width="8" height="8" fill="url(#paint)"/></defs><use href="#shape"/></svg>',
  }).svg("test", { size });
  for (const [left, right, size] of [
    [draw("red", 1), draw("lime", 1), 290],
    [draw("red", 1), draw("red", 2), 290],
    [draw("red", 1), draw("red", 1, 580), 580],
  ]) {
    const combined = rasterizeSvg(`<svg xmlns="http://www.w3.org/2000/svg" width="${290 + size}" height="${size}">` +
      left + right.replace("<svg ", '<svg x="290" ') + "</svg>");
    const expected = rasterizeSvg(right);
    for (let y = 0; y < size; y++) {
      assert.deepEqual(combined.data.slice((y * combined.width + 290) * 4, (y * combined.width + 290 + size) * 4),
        expected.data.slice(y * size * 4, (y + 1) * size * 4));
    }
  }
  assert.equal(draw("red", 1), draw("red", 1), "content-derived IDs remain deterministic");
});

test("subnormal SVG outline widths never repaint the normal logo", { skip: !hasSvgRasterizer }, () => {
  for (const isolated of [false, true]) {
    const render = (svgLogoOutlineWidth) => rasterizeSvg(new QrRenderer({
      foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
      logoSvg: `<svg viewBox="0 0 10 10">${isolated ? "<style/>" : ""}<rect width="10" height="10" fill="red" fill-opacity=".5"/></svg>`,
      svgLogoOutlineColor: "#fff", svgLogoOutlineWidth,
    }).svg("test", { size: 290 })).data;
    assert.deepEqual(render(Number.MIN_VALUE), render(0));
  }
});

test("CSS-escaped fragment references keep their original logo paint", { skip: !hasSvgRasterizer }, () => {
  for (const [id, url] of [
    ["paint", String.raw`url(#\70 aint)`],
    ["paint", String.raw`url('#\000070aint')`],
    ["paint", String.raw`url(&quot;\23 paint&quot;)`],
    ["a)b", String.raw`url(#a\)b)`],
  ]) {
    for (const attr of [`fill="${url}"`, `style="fill:${url}"`]) {
      const svg = new QrRenderer({
        foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
        logoSvg: `<svg viewBox="0 0 10 10"><defs><linearGradient id="${id}"><stop stop-color="red"/></linearGradient></defs>` +
          `<rect width="10" height="10" ${attr}/></svg>`,
      }).svg("test", { size: 290 });
      const pixels = rasterizeSvg(svg);
      const center = (145 * 290 + 145) * 4;
      assert.deepEqual([...pixels.data.slice(center, center + 4)], [255, 0, 0, 255], attr);
    }
  }
});

test("CSS comments after quoted URLs preserve SVG logo paint", { skip: !hasSvgRasterizer }, () => {
  for (const url of [
    "url('#paint'/**/)",
    "url( '#paint' /* ) ' url(#other) */ /**/ )",
    String.raw`url('#\70 aint'/* first *//* second */)`,
  ]) {
    for (const attribute of [`fill="${url}"`, `style="fill:${url}"`]) {
      const logoSvg = '<svg xmlns="http://www.w3.org/2000/svg" width="290" height="290" viewBox="0 0 10 10">' +
        '<defs><linearGradient id="paint"><stop stop-color="red"/></linearGradient></defs>' +
        `<rect width="10" height="10" ${attribute}/></svg>`;
      const generated = new QrRenderer({
        foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3, logoSvg,
      }).svg("test", { size: 290 });
      const center = (145 * 290 + 145) * 4;
      for (const svg of [logoSvg, generated]) {
        const pixels = rasterizeSvg(svg);
        assert.deepEqual([...pixels.data.slice(center, center + 4)], [255, 0, 0, 255], attribute);
      }
    }
  }
});

test("PNG and SVG use identical pixel-aligned square logo backings", { skip: !hasSvgRasterizer }, () => {
  const { header, end } = tinyLogoChunks(6);
  const logoPng = Buffer.concat([PNG_SIGNATURE, header, pngChunk("IDAT", deflateSync(Buffer.from([0, 0, 0, 0, 0]))), end]);
  for (const size of [290, 291, 512]) {
    for (const logoPadding of [0, 0.35, 4]) {
      const renderer = new QrRenderer({
        foreground: "#0000ff", background: "#0000ff", logoBackground: "#ffffff", logoScale: 0.3,
        logoPadding, logoPng, logoSvg: '<svg viewBox="0 0 1 1"><path fill="none"/></svg>',
      });
      const png = decodeRgbaPng(renderer.png("test", { size }));
      const svg = rasterizeSvg(renderer.svg("test", { size }));
      assert.deepEqual(svg.data, png.data, `size ${size}, padding ${logoPadding}`);
    }
  }
});

test("root CSS sizing cannot escape the configured logo bounds", { skip: !hasSvgRasterizer }, () => {
  const render = (style, stylesheet, size) => rasterizeSvg(new QrRenderer({
    foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
    logoSvg: `<svg viewBox="0 0 10 10" style="${style}">${stylesheet}<rect width="10" height="10" fill="red"/></svg>`,
  }).svg("test", { size })).data;
  for (const size of [290, 512]) {
    const expected = render("", "", size);
    for (const style of [
      "width:100px;height:100px",
      "width:100%;height:100%",
      "width:100px!important;height:100px!important;min-width:200px;max-height:1px",
      "x:100px;y:100px;min-height:1000px!important;max-width:1px!important",
    ]) {
      assert.deepEqual(render(style, "", size), expected, style);
    }
    assert.deepEqual(render("", "<style>svg {width:100px!important;height:100px!important}</style>", size), render("", "<style/>", size));
  }
});

test("equivalent transformed shapes retain the same logo-unit outline width", { skip: !hasSvgRasterizer }, () => {
  const render = (body, size, isolated) => rasterizeSvg(new QrRenderer({
    foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3,
    logoSvg: `<svg viewBox="0 0 10 10">${isolated ? "<style/>" : ""}${body}</svg>`,
    svgLogoOutlineColor: "#fff", svgLogoOutlineWidth: 1,
  }).svg("test", { size })).data;
  const shapes = [
    ['<circle cx="5" cy="5" r="4" fill="red"/>', '<g transform="scale(.1)"><circle cx="50" cy="50" r="40" fill="red"/></g>'],
    ['<ellipse cx="5" cy="5" rx="4" ry="2" fill="red"/>', '<g transform="translate(5 5) scale(.2 .1)"><circle r="20" fill="red"/></g>'],
    ['<circle cx="5" cy="5" r="4" fill="red"/>', '<g transform="translate(10 10)"><g transform="scale(-.1)"><circle cx="50" cy="50" r="40" fill="red"/></g></g>'],
  ];
  // Integral fitted image dimensions avoid intermediary image-surface
  // antialiasing differences when comparing inline and isolated outlines.
  for (const [size, isolated] of [[290, true], [580, true], [400, false], [800, false]]) {
    for (const [plain, transformed] of shapes) {
      const expected = render(plain, size, isolated);
      const actual = render(transformed, size, isolated);
      for (let i = 0; i < actual.length; i++) {
        assert.ok(Math.abs(actual[i] - expected[i]) <= 2, `pixel byte ${i}, size ${size}, isolated ${isolated}`);
      }
    }
  }
});

test("XML references resolve before IDs and fragment URLs are rewritten", { skip: !hasSvgRasterizer }, () => {
  const draw = (id, fill) => `<svg viewBox="0 0 10 10"><defs><linearGradient id="${id}"><stop stop-color="red"/></linearGradient></defs><rect width="10" height="10" fill="${fill}"/></svg>`;
  const render = (logoSvg) => rasterizeSvg(new QrRenderer({
    foreground: "#0000ff", background: "#0000ff", logoBackground: "#0000", logoScale: 0.3, logoSvg,
  }).svg("test", { size: 290 })).data;
  const expected = render(draw("grad", "url(#grad)"));
  for (const logo of [
    draw("grad", "url(&#35;grad)"),
    draw("gr&#97;d", "url(&quot;&#x23;grad&quot;)"),
    draw("gr&#x61;d", "url(&apos;#gr&#97;d&apos;)"),
    draw("gr&amp;ad", "url(&quot;#gr&amp;ad&quot;)"),
    '<svg viewBox="0&#32;0&#x20;10 10"><defs><rect id="sh&#97;pe" width="10" height="10" fill="red"/></defs><use href="&#35;shape"/></svg>',
  ]) {
    assert.deepEqual(render(logo), expected);
  }
  for (const invalid of ["&#0;", "&#xD800;", "&#1114112;", "&unknown;"]) {
    assert.throws(() => new QrRenderer({ logoSvg: draw("grad", `url(${invalid}grad)`) }), /invalid SVG logo/u);
  }
});

test("invalid public options fail before allocating output", () => {
  assert.throws(() => new QrRenderer({ foreground: "not-a-color" }), /invalid color/u);
  assert.throws(() => new QrRenderer({ margin: 33 }), /margin/u);
  assert.throws(() => new QrRenderer({ margin: 1.5 }), /integer/u);
  assert.throws(() => new QrRenderer({ svgLogoOutlineWidth: -1 }), /svgLogoOutlineWidth/u);
  const renderer = new QrRenderer();
  assert.throws(() => renderer.png("test", { size: 20 }), /size must be between/u);
  assert.throws(() => renderer.png("test", { size: 512.5 }), /integer/u);
  assert.throws(
    () => new QrRenderer({ errorCorrection: "high" }).png("x".repeat(1_000), { size: 21 }),
    /too small for this symbol/u,
  );
  assert.throws(() => renderer.svg("x".repeat(3_000)), /too long/u);
});
