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
