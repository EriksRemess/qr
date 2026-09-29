import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const packageDirectory = dirname(fileURLToPath(import.meta.url));

function nativeFilename() {
  if (process.platform === "linux" && process.arch === "x64") {
    const glibc = process.report?.getReport()?.header?.glibcVersionRuntime;
    if (!glibc) {
      throw new Error("@eriksremess/qr supports Linux x64 with glibc; musl is not supported");
    }
    return "qr-native-linux-x64-gnu.node";
  }

  if (process.platform === "darwin" && process.arch === "arm64") {
    return "qr-native-darwin-arm64.node";
  }

  throw new Error(
    `Unsupported @eriksremess/qr platform: ${process.platform}-${process.arch}`,
  );
}

const native = require(join(packageDirectory, "native", nativeFilename()));

export const { QrRenderer } = native;
export default native;
