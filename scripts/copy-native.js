import { execFileSync } from "node:child_process";
import { copyFile, mkdir, mkdtemp, rename, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function hostBuild() {
  if (process.platform === "linux" && process.arch === "x64") {
    const glibc = process.report?.getReport()?.header?.glibcVersionRuntime;
    if (!glibc) {
      throw new Error("Linux native builds currently require x64 glibc");
    }
    return { target: "x86_64-unknown-linux-gnu", library: "libqr_native.so", addon: "qr-native-linux-x64-gnu.node" };
  }
  if (process.platform === "darwin" && process.arch === "arm64") {
    return { target: "aarch64-apple-darwin", library: "libqr_native.dylib", addon: "qr-native-darwin-arm64.node" };
  }
  throw new Error(`Unsupported native build host: ${process.platform}-${process.arch}`);
}

function buildLibrary({ target, library }) {
  const messages = execFileSync("cargo", [
    "build", "--locked", "--release", "--lib", "--target", target,
    "--message-format=json-render-diagnostics",
  ], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
  for (const line of messages.trimEnd().split("\n")) {
    const artifact = JSON.parse(line);
    if (artifact.reason === "compiler-artifact" && artifact.manifest_path === join(root, "Cargo.toml")) {
      const path = artifact.filenames.find(file => file.endsWith(`/${library}`));
      if (path) return path;
    }
  }
  throw new Error(`Cargo did not report the built ${library}`);
}

export async function copyNative(source, destination) {
  const nativeDirectory = dirname(destination);
  await mkdir(nativeDirectory, { recursive: true });
  // Replace the directory entry atomically so loaded processes retain their mapped library.
  const stagingDirectory = await mkdtemp(join(nativeDirectory, ".qr-build-"));
  try {
    const staged = join(stagingDirectory, "addon.node");
    await copyFile(source, staged);
    await rename(staged, destination);
  } finally {
    await rm(stagingDirectory, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  const build = hostBuild();
  await copyNative(buildLibrary(build), join(root, "native", build.addon));
}
