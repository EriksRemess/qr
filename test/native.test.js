import assert from "node:assert/strict";
import { crc32, deflateSync, inflateSync } from "node:zlib";
import test from "node:test";

import jsQR from "jsqr";

import { QrRenderer } from "../index.js";

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
    assert.ok([0, 1].includes(filtered[sourceStart]), "expected None or Sub filtering");
    for (let x = 0; x < rowBytes; x += 1) {
      const left = x >= channels ? raw[y * rowBytes + x - channels] : 0;
      raw[y * rowBytes + x] = filtered[sourceStart] === 1
        ? (filtered[sourceStart + 1 + x] + left) & 0xff
        : filtered[sourceStart + 1 + x];
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

for (const moduleShape of ["square", "dot"]) {
  test(`native PNG round-trips UTF-8 payload with ${moduleShape} modules`, () => {
    const text = "https://example.com/ābols?日本語=✓";
    const renderer = new QrRenderer({
      background: "#ffffffff",
      errorCorrection: "high",
      foreground: "#172554ff",
      margin: 4,
      moduleShape,
    });
    const decodedPng = decodeRgbaPng(renderer.png(text, { size: 512 }));
    const decodedQr = jsQR(decodedPng.data, decodedPng.width, decodedPng.height, {
      inversionAttempts: "attemptBoth",
    });
    assert.ok(decodedQr, "jsQR should locate the generated symbol");
    assert.equal(decodedQr.data, text);
  });
}

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
  assert.match(svg, /fill="#abcdef" fill-opacity="0\.502"/u);
  assert.match(svg, /fill="#123456"/u);
});

test("invalid public options fail before allocating output", () => {
  assert.throws(() => new QrRenderer({ foreground: "not-a-color" }), /invalid color/u);
  assert.throws(() => new QrRenderer({ margin: 33 }), /margin/u);
  assert.throws(() => new QrRenderer({ margin: 1.5 }), /integer/u);
  assert.throws(() => new QrRenderer({ moduleShape: "triangle" }), /moduleShape/u);
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
