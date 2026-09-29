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

| Format | Payload | Output size |
| --- | --- | --- |
| SVG | 32, 256, 1024 bytes | 512px |
| PNG | 32 bytes | 256, 512, 1024px |

All cases use square modules, high error correction, the default four-module
margin, and no logo. Results report operations/second, mean milliseconds per
operation, and output bytes.

Timing includes encoding and rendering. It excludes renderer construction,
logo preparation, SVG viewer rendering, filesystem access, and HTTP handling.
These synchronous loop averages do not measure concurrency or tail latency.

Run `npm test` before measuring. Record the CPU, OS/libc, Node/Rust versions,
build flags, and benchmark settings when comparing results.

## Recorded sample — 2026-09-29

AMD Ryzen 9 5950X; Linux x86-64/glibc 2.43; Node 26.10.0; Rust 1.98.1.
Release profile with thin LTO and one codegen unit; default iteration counts.
PNG uses RGBA, Sub filtering, and RLE compression.

| Case | Operations/second | Milliseconds/operation | Output bytes |
| --- | ---: | ---: | ---: |
| SVG, 32 B, 512px | 8,234.9 | 0.1214 | 4,027 |
| SVG, 256 B, 512px | 938.3 | 1.0657 | 25,689 |
| SVG, 1024 B, 512px | 263.7 | 3.7924 | 98,078 |
| PNG, 32 B, 256px | 2,789.9 | 0.3584 | 11,026 |
| PNG, 32 B, 512px | 1,806.5 | 0.5535 | 22,907 |
| PNG, 32 B, 1024px | 737.3 | 1.3562 | 50,343 |

This is a retained measurement, not a fresh run or a throughput guarantee.
Rerun on the target machine after code or toolchain changes.
