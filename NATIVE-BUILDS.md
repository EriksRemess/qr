# Native builds

## Supported targets

The JavaScript loader currently recognizes exactly two artifacts:

| Host | Node platform/architecture | Artifact |
| --- | --- | --- |
| Production Linux | `linux-x64` with glibc | `native/qr-native-linux-x64-gnu.node` |
| Apple Silicon macOS | `darwin-arm64` | `native/qr-native-darwin-arm64.node` |

Node-API keeps the addon ABI stable across compatible Node releases. It does
not make machine code portable across operating systems, architectures, or C
libraries, so each table row requires its own build.

## Build on each host

With Node 26, Rust, and a native linker installed:

```sh
npm install
npm run build:native
npm test
```

`scripts/copy-native.js` detects the build host and copies Cargo's release
library to the artifact name expected by `index.js`. On the Linux server, run
this on an x86-64 glibc host compatible with the deployment environment. On an
Apple Silicon development machine, the same command creates the Darwin ARM64
artifact.

Cross-compilation is deliberately not hidden in the first implementation. It
adds linker/SDK complexity and provides less confidence than running tests on
the target operating system.

## Assemble a two-platform package

Build and test on both hosts, collect both `.node` files into `native/`, then
inspect the package before publishing:

```sh
npm pack --dry-run
```

The package has no install-time compilation and no download script. Consumers
therefore get deterministic installation with no network or Rust requirement,
provided the matching artifact was included. Missing and unsupported targets
fail immediately with a platform-specific loader error.

The single package intentionally omits npm's `os` and `cpu` fields because
those fields cannot express the allowed pairings without also claiming support
for Linux ARM64 and Intel macOS. If more targets are added, platform-specific
optional packages are preferable to an install script.

## Release checklist

1. Run `npm run check` and `npm test` on Linux x64.
2. Run the same commands on Apple Silicon macOS.
3. Run `npm run bench` on the production-class Linux host and retain the raw
   results with the Node/Rust versions.
4. Confirm both native artifact filenames are present.
5. Run `npm pack --dry-run` and inspect the file list and unpacked size.
6. Install the tarball into a clean fixture on both platforms.
7. Decode at least one styled SVG/PNG result independently on each platform.

Publishing is intentionally outside the build scripts.

## GitHub Packages release workflow

`.github/workflows/publish.yml` runs when a non-prerelease GitHub Release is
released, or when explicitly started with `workflow_dispatch`. It:

1. builds and tests `linux-x64-gnu` on `ubuntu-24.04`;
2. builds and tests `darwin-arm64` on the Apple Silicon `macos-15` runner;
3. uploads each native binary as a short-lived workflow artifact;
4. downloads both binaries into a clean publish job;
5. verifies that a release tag is exactly `v` plus the `package.json` version;
6. inspects the npm package contents; and
7. publishes the public scoped package to `https://npm.pkg.github.com`.

The publish job grants `packages: write` only to itself and authenticates npm
with the automatically generated `GITHUB_TOKEN`. Build jobs retain read-only
repository permissions. Third-party registry secrets are not used.
