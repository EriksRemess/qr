# Benchmarks

## Run

```sh
npm run bench
```

This builds the release addon and measures the public Node.js API. Defaults are
50 warmups per case, 2,000 SVG iterations, and 200 PNG iterations. To override:

```sh
QR_BENCH_WARMUP=100 \
QR_BENCH_SVG_ITERATIONS=10000 \
QR_BENCH_PNG_ITERATIONS=500 \
npm run bench
```

## Cases

| Format | Style | Payload | Output size |
| --- | --- | --- | --- |
| SVG | square | 32, 256, 1024 bytes | 512px |
| PNG | square | 32 bytes | 256, 512, 1024px |
| SVG | rounded | 32 bytes | 512px |
| PNG | rounded | 32 bytes | 1024px |

All cases use high error correction, the default four-module
margin, and no logo. Results report operations/second, mean milliseconds per
operation, and output bytes.

Timing includes encoding and rendering. It excludes renderer construction,
logo preparation, SVG viewer rendering, filesystem access, and HTTP handling.
These synchronous loop averages do not measure concurrency or tail latency.

Run `npm test` before measuring. Record the CPU, OS/libc, Node/Rust versions,
build flags, and benchmark settings when comparing results.

## Renderer comparison — 2026-11-24

Median throughput from three runs of the default benchmark per build, alternating
build order on one core of an AMD Ryzen 9 5950X. Both builds used Linux
x86-64/glibc 2.44, Node 26.13.1, Rust 1.100.0, the same updated dependencies
and the release profile. The baseline used the renderer from `2cd54e2`.

| Case | Before (ops/s) | After (ops/s) | Speedup | Before (bytes) | After (bytes) |
| --- | ---: | ---: | ---: | ---: | ---: |
| SVG square 32B 512px | 7,505 | 27,534 | 3.67× | 4,027 | 4,027 |
| SVG square 256B 512px | 921 | 4,326 | 4.70× | 25,689 | 25,689 |
| SVG square 1024B 512px | 258 | 1,210 | 4.68× | 98,078 | 98,078 |
| PNG square 32B 256px | 3,099 | 9,568 | 3.09× | 11,026 | 2,726 |
| PNG square 32B 512px | 1,577 | 2,749 | 1.74× | 22,907 | 4,196 |
| PNG square 32B 1024px | 692 | 1,338 | 1.93× | 50,343 | 8,233 |
| SVG rounded 32B 512px | 3,184 | 6,777 | 2.13× | 23,853 | 23,853 |
| PNG rounded 32B 1024px | 183 | 301 | 1.65× | 136,036 | 99,375 |

A second comparison varied the URL on every timed call, using the same inputs
for both builds. Across five alternating runs, square SVG throughput improved
3.94–4.65× across the three payload lengths. The 512px square PNG improved
2.72× and the 1024px rounded PNG improved 1.66×.

## Recorded sample — 2026-10-01

AMD Ryzen 9 5950X; Linux x86-64/glibc 2.43; Node 26.10.0; Rust 1.98.1.
Release profile with thin LTO and one codegen unit; default iteration counts.
PNG uses RGBA, Sub filtering, and RLE compression.

| Case | Operations/second | Milliseconds/operation | Output bytes |
| --- | ---: | ---: | ---: |
| SVG square, 32 B, 512px | 8,041.5 | 0.1244 | 4,027 |
| SVG square, 256 B, 512px | 997.4 | 1.0026 | 25,689 |
| SVG square, 1024 B, 512px | 278.1 | 3.5961 | 98,078 |
| PNG square, 32 B, 256px | 2,770.0 | 0.3610 | 11,026 |
| PNG square, 32 B, 512px | 1,762.0 | 0.5675 | 22,907 |
| PNG square, 32 B, 1024px | 738.6 | 1.3539 | 50,343 |
| SVG rounded, 32 B, 512px | 3,339.8 | 0.2994 | 23,853 |
| PNG rounded, 32 B, 1024px | 190.6 | 5.2462 | 136,036 |

Rerun on the target machine after code or toolchain changes.
