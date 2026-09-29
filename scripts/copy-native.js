import { copyFile, mkdir, mkdtemp, rename, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function outputName() {
  if (process.platform === "linux" && process.arch === "x64") {
    const glibc = process.report?.getReport()?.header?.glibcVersionRuntime;
    if (!glibc) {
      throw new Error("Linux native builds currently require x64 glibc");
    }
    return "qr-native-linux-x64-gnu.node";
  }
  if (process.platform === "darwin" && process.arch === "arm64") {
    return "qr-native-darwin-arm64.node";
  }
  throw new Error(`Unsupported native build host: ${process.platform}-${process.arch}`);
}

const sourceName = process.platform === "win32"
  ? "qr_native.dll"
  : process.platform === "darwin"
    ? "libqr_native.dylib"
    : "libqr_native.so";

const nativeDirectory = join(root, "native");
await mkdir(nativeDirectory, { recursive: true });
const destination = join(nativeDirectory, outputName());
// Never truncate a library that another Node process may have memory-mapped.
// Stage on the same filesystem, then replace the directory entry atomically.
// Existing processes retain the old inode; new processes load the new binary.
const stagingDirectory = await mkdtemp(join(nativeDirectory, ".qr-build-"));
try {
  const staged = join(stagingDirectory, "addon.node");
  await copyFile(join(root, "target", "release", sourceName), staged);
  await rename(staged, destination);
} finally {
  await rm(stagingDirectory, { recursive: true, force: true });
}
