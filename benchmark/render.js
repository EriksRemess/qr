import { performance } from "node:perf_hooks";

import { QrRenderer } from "../index.js";

const warmup = positiveInteger(process.env.QR_BENCH_WARMUP, 50);
const svgIterations = positiveInteger(process.env.QR_BENCH_SVG_ITERATIONS, 2_000);
const pngIterations = positiveInteger(process.env.QR_BENCH_PNG_ITERATIONS, 200);

function positiveInteger(value, fallback) {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

function payload(bytes) {
  const seed = "https://example.com/path?q=qr-native-benchmark&";
  return seed.repeat(Math.ceil(bytes / seed.length)).slice(0, bytes);
}

function run({ iterations, name, render }) {
  let sample;
  for (let index = 0; index < warmup; index += 1) {
    sample = render();
  }

  const started = performance.now();
  for (let index = 0; index < iterations; index += 1) {
    sample = render();
  }
  const elapsedMs = performance.now() - started;
  const msPerOp = elapsedMs / iterations;
  const outputBytes = typeof sample === "string" ? Buffer.byteLength(sample) : sample.byteLength;
  return {
    name,
    iterations,
    msPerOp,
    opsPerSecond: 1_000 / msPerOp,
    outputBytes,
  };
}

const rows = [];
for (const moduleShape of ["square", "dot"]) {
  const renderer = new QrRenderer({
    errorCorrection: "high",
    foreground: "#172554",
    moduleShape,
  });
  for (const bytes of [32, 256, 1_024]) {
    const text = payload(bytes);
    rows.push(run({
      iterations: svgIterations,
      name: `SVG ${moduleShape} ${bytes}B 512px`,
      render: () => renderer.svg(text, { size: 512 }),
    }));
  }
  for (const size of [256, 512, 1_024]) {
    const text = payload(32);
    rows.push(run({
      iterations: pngIterations,
      name: `PNG ${moduleShape} 32B ${size}px`,
      render: () => renderer.png(text, { size }),
    }));
  }
}

const nameWidth = Math.max(...rows.map(({ name }) => name.length));
console.log(`Node ${process.version} ${process.platform}-${process.arch}`);
console.log(`warmup=${warmup} svgIterations=${svgIterations} pngIterations=${pngIterations}\n`);
console.log(
  `${"benchmark".padEnd(nameWidth)}  ${"ops/s".padStart(12)}  ${"ms/op".padStart(10)}  ${"bytes".padStart(10)}`,
);
console.log("-".repeat(nameWidth + 38));
for (const row of rows) {
  console.log(
    `${row.name.padEnd(nameWidth)}  ${row.opsPerSecond.toFixed(1).padStart(12)}  ` +
    `${row.msPerOp.toFixed(4).padStart(10)}  ${String(row.outputBytes).padStart(10)}`,
  );
}
