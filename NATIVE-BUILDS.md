# Native builds

## Supported targets

| Platform | Artifact |
| --- | --- |
| Linux x86-64 with glibc | `native/qr-native-linux-x64-gnu.node` |
| Apple Silicon macOS | `native/qr-native-darwin-arm64.node` |

Node-API provides compatibility across supported Node.js versions, but each
OS/architecture needs its own binary. Linux musl, Linux ARM64, Intel macOS,
and Windows are not supported.

## Build

Run on the target platform with Node.js 26+, Rust 1.88+, and a native linker:

```sh
npm install
npm run build:native
npm test
```

The build script selects the artifact name from the host platform and copies
Cargo's release library into `native/`. It does not configure cross-compilation.
Linux binaries must be built against a glibc version compatible with the machine
that will load them.

The copy is staged on the same filesystem and installed with an atomic rename.
Existing Node processes keep their loaded binary; restart them to use a new
build. Copy failures leave the previous artifact intact.

Package installation does not compile or download an addon. The matching native
artifact must already be present.
