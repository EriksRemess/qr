import { mkdir, writeFile } from "node:fs/promises";
import { crc32, deflateSync } from "node:zlib";

import { QrRenderer } from "../index.js";

const text = process.argv[2] ?? "https://example.com/qr-demo";
const outputDirectory = new URL("./output/generic/", import.meta.url);
const logoGlyphs = [
  { x: 8, rows: ["01110", "10001", "10001", "10001", "10101", "10010", "01101"] },
  { x: 28, rows: ["11110", "10001", "10001", "11110", "10100", "10010", "10001"] },
];
const logoGlyphPath = logoGlyphs.flatMap(({ x, rows }) => rows.flatMap((row, y) =>
  [...row].flatMap((value, column) => value === "1"
    ? [`M${x + column * 2} ${3 + y * 2}h2v2h-2z`]
    : []))).join("");
const logoSvg = `
  <svg viewBox="0 0 48 20">
    <rect width="48" height="20" rx="4" fill="#2563eb"/>
    <path d="${logoGlyphPath}" fill="#fff" shape-rendering="crispEdges"/>
  </svg>
`;

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const chunk = Buffer.allocUnsafe(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
}

function createLogoPng() {
  const scale = 8;
  const width = 48 * scale;
  const height = 20 * scale;
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
      const logoX = (x + 0.5) / scale;
      const logoY = (y + 0.5) / scale;
      const cornerX = Math.min(44, Math.max(4, logoX));
      const cornerY = Math.min(16, Math.max(4, logoY));
      const inside = Math.hypot(logoX - cornerX, logoY - cornerY) <= 4;
      const glyph = logoGlyphs.some(({ x: glyphX, rows }) => {
        const column = Math.floor((logoX - glyphX) / 2);
        const glyphRow = Math.floor((logoY - 3) / 2);
        return column >= 0 && column < 5 && glyphRow >= 0 && glyphRow < 7
          && rows[glyphRow][column] === "1";
      });
      scanlines[pixel] = glyph ? 255 : 37;
      scanlines[pixel + 1] = glyph ? 255 : 99;
      scanlines[pixel + 2] = glyph ? 255 : 235;
      scanlines[pixel + 3] = inside ? 255 : 0;
    }
  }

  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  return Buffer.concat([
    signature,
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(scanlines)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

const renderer = new QrRenderer({
  background: "#eff6ffff",
  errorCorrection: "high",
  foreground: "#172554ff",
  logoBackground: "#ffffff00",
  logoPadding: 0.25,
  logoPng: createLogoPng(),
  logoScale: 0.22,
  logoSvg,
  margin: 2,
});

await mkdir(outputDirectory, { recursive: true });
await Promise.all([
  writeFile(new URL("qr.svg", outputDirectory), renderer.svg(text, { size: 512 }), "utf8"),
  writeFile(new URL("qr.png", outputDirectory), renderer.png(text, { size: 1_024 })),
]);

console.log(`Generated ${text} as example/output/generic/qr.svg and qr.png`);
