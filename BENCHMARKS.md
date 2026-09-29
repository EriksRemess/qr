# Benchmarks

## Running

```sh
npm run bench
```

The command builds the release native addon and measures the public Node API.
Iteration counts can be adjusted without changing the benchmark source:

```sh
QR_BENCH_WARMUP=100 \
QR_BENCH_SVG_ITERATIONS=10000 \
QR_BENCH_PNG_ITERATIONS=500 \
npm run bench
```

## Current matrix

- Payloads: 32, 256, and 1024 UTF-8 bytes for SVG
- PNG dimensions: 256, 512, and 1024 pixels
- Module shape: square
- Error correction: H
- Reported values: operations/second, milliseconds/operation, output bytes

The benchmark retains each result long enough to observe its size, preventing
the call from becoming dead work. Warmup runs occur before measurement.

## Interpretation

Results are comparative, not universal. Record at least:

- Node and Rust versions;
- operating system, CPU architecture, and libc;
- release profile and CPU flags;
- payload, style, dimensions, and compression configuration;
- whether a logo is present;
- whether correctness tests passed for the tested output.

Do not compare cached HTTP route timings with uncached renderer timings. The
application cache primarily measures map lookup and response plumbing; renderer
benchmarks measure encoding and image generation.

## Planned additions

- Original `@eriksremess/qrcode` plus `pngjs` comparison
- Synthetic SVG and PNG logo fixtures
- Stage-level Rust timings for encoding, masks, rasterization, filtering,
  compression, and compositing
- p50/p95/p99 latency under concurrency 1, 4, and 16
- synchronous versus worker-pool PNG generation
- peak RSS and allocation counts
- output-size comparisons at multiple compression strategies
- optional end-to-end application benchmarks maintained by consumers

Any benchmark candidate that does not independently decode is a correctness
failure and must not be reported as a performance result.

## Recorded implementation decisions

The encoder always emits RGBA, including for fully opaque styles. An RGB fast
path was implemented and measured on Linux x86-64 with Node 26.7.0, but made
1024-pixel square PNG generation slower (approximately 5.6 ms instead of 3.8
ms). With the Sub filter, RGBA's constant alpha lane creates a more favorable
RLE stream. The RGB path was therefore removed rather than retained as
unmeasured complexity.

Square-module rasterization caches filtered rows by QR module row. At 1024px,
one module spans many identical pixel rows; regenerating those rows dominated
the initial native implementation. The cache is bypassed for rows intersecting
a PNG logo, where submodule pixel position changes coverage.

After the full-matrix differential tests were added, a Linux x86-64/Node 26.7.0
run measured the following public-API loop averages on 2026-09-29:

| Case | Operations/second | Milliseconds/operation | Output bytes |
| --- | ---: | ---: | ---: |
| SVG square, 32 B, 512px | 7,983.8 | 0.1253 | 4,027 |
| PNG square, 32 B, 1024px | 722.1 | 1.3849 | 50,343 |

These figures are development snapshots, not durable release claims. Rerun the
suite after toolchain, encoder, renderer, or compression changes.

## Square-only regression run (2026-09-29)

After removing alternate module shapes and fixing alpha compositing and logo
parsing, the default benchmark measured the following on an AMD Ryzen 9 5950X,
Linux x86-64/glibc 2.43, Node 26.10.0, and Rust 1.98.1. This used the standard
release profile (thin LTO, one codegen unit), H correction, no logo, and Sub/RLE
PNG compression. Each case had 50 warmups, then 2,000 SVG or 200 PNG iterations.
All 13 Rust and 12 Node tests passed, including independent SVG rasterization
and QR decoding checks. These are local loop averages, not server throughput.

| Case | Operations/second | Milliseconds/operation | Output bytes |
| --- | ---: | ---: | ---: |
| SVG square, 32 B, 512px | 8,234.9 | 0.1214 | 4,027 |
| SVG square, 256 B, 512px | 938.3 | 1.0657 | 25,689 |
| SVG square, 1024 B, 512px | 263.7 | 3.7924 | 98,078 |
| PNG square, 32 B, 256px | 2,789.9 | 0.3584 | 11,026 |
| PNG square, 32 B, 512px | 1,806.5 | 0.5535 | 22,907 |
| PNG square, 32 B, 1024px | 737.3 | 1.3562 | 50,343 |

After the SVG-logo isolation/outline fixes and shared backing layout, the same
machine and default benchmark settings measured 0.1207 ms for the 32-byte SVG
case and 1.3959 ms for the 1024px PNG case. Output byte counts were unchanged.
These unbranded cases do not measure logo compilation or SVG viewer rendering;
logo parsing, ID rewriting and isolated-image URI creation happen once in the
renderer constructor, not on each render call.

A follow-up run after viewport-CSS normalization, transform-independent outline
widths and XML-reference decoding measured 0.1287 ms for the 32-byte SVG case
and 1.3713 ms for 1024px PNG on the same host/settings. The 17 Rust and 21 Node
tests passed, including transformed-outline and viewport/reference raster checks.
These unbranded timings do not measure the small per-render substitution of the
outlined logo's output-pixel stroke width.
