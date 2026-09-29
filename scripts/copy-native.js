import { copyFile, mkdir } from "node:fs/promises";
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
await copyFile(join(root, "target", "release", sourceName), join(nativeDirectory, outputName()));
