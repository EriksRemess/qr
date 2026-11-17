import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { copyNative } from "../scripts/copy-native.js";

const filename = process.platform === "darwin"
  ? "qr-native-darwin-arm64.node" : "qr-native-linux-x64-gnu.node";
const sourceName = process.platform === "darwin" ? "libqr_native.dylib" : "libqr_native.so";

async function fixture(run) {
  const root = mkdtempSync(join(tmpdir(), "qr-copy-test-"));
  try {
    for (const directory of ["scripts", "native", "target/release"]) {
      mkdirSync(join(root, directory), { recursive: true });
    }
    writeFileSync(join(root, "package.json"), JSON.stringify({ type: "module" }));
    const script = join(root, "scripts/copy-native.js");
    copyFileSync(new URL("../scripts/copy-native.js", import.meta.url), script);
    const destination = join(root, "native", filename);
    const source = join(root, "target/release", sourceName);
    copyFileSync(new URL(`../native/${filename}`, import.meta.url), source);
    await run({ root, script, source, destination });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("native rebuild atomically replaces the file without invalidating loaded code", () => {
  return fixture(({ root, script, source, destination }) => {
    copyFileSync(source, destination);
    const before = statSync(destination).ino;
    // Load only a disposable addon in a child process. A regression must fail
    // the test, not crash the test runner or overwrite the package's addon.
    const result = spawnSync(process.execPath, ["-e", `
      (async () => {
      const assert = require('node:assert/strict');
      const { pathToFileURL } = require('node:url');
      const { QrRenderer } = require(${JSON.stringify(destination)});
      const renderer = new QrRenderer();
      const expected = renderer.svg('still loaded');
      const { copyNative } = await import(pathToFileURL(${JSON.stringify(script)}));
      await copyNative(${JSON.stringify(source)}, ${JSON.stringify(destination)});
      assert.equal(renderer.svg('still loaded'), expected);
      assert.ok(renderer.png('still loaded').length > 0);
      })().catch(error => { console.error(error); process.exitCode = 1; });
    `], { timeout: 15_000 });
    assert.equal(result.signal, null, `addon process received ${result.signal}`);
    assert.equal(result.status, 0, result.stderr?.toString());
    assert.notEqual(statSync(destination).ino, before);
    assert.deepEqual(readFileSync(destination), readFileSync(source));
    assert.deepEqual(readdirSync(join(root, "native")), [filename]);
  });
});

test("failed native copy preserves the installed binary and cleans staging files", () => {
  return fixture(async ({ root, source, destination }) => {
    copyFileSync(source, destination);
    const before = statSync(destination).ino;
    const bytes = readFileSync(destination);
    rmSync(source);
    await assert.rejects(copyNative(source, destination), { code: "ENOENT" });
    assert.equal(statSync(destination).ino, before);
    assert.deepEqual(readFileSync(destination), bytes);
    assert.deepEqual(readdirSync(join(root, "native")), [filename]);
  });
});

test("first native copy installs a binary and cleans staging files", () => {
  return fixture(async ({ root, source, destination }) => {
    assert.equal(existsSync(destination), false);
    await copyNative(source, destination);
    assert.deepEqual(readFileSync(destination), readFileSync(source));
    assert.deepEqual(readdirSync(join(root, "native")), [filename]);
  });
});

test("native build installs the library Cargo produced in a custom target directory", () => {
  return fixture(({ root, destination }) => {
    copyFileSync(new URL("../package.json", import.meta.url), join(root, "package.json"));
    writeFileSync(join(root, "Cargo.toml"), `[package]
name = "qr_native"
version = "0.1.0"
edition = "2024"
[lib]
path = "lib.rs"
crate-type = ["cdylib"]
`);
    writeFileSync(join(root, "lib.rs"), "#[unsafe(no_mangle)] pub extern \"C\" fn fixture() -> u32 { 42 }");
    const lock = spawnSync("cargo", ["generate-lockfile", "--offline"], { cwd: root });
    assert.equal(lock.status, 0, lock.stderr?.toString());
    const target = join(root, "custom target");
    const build = spawnSync("npm", ["run", "build:native"], {
      cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target }, timeout: 30_000,
    });
    assert.equal(build.status, 0, build.stderr?.toString());
    const library = readdirSync(target, { recursive: true })
      .find(path => path.endsWith(join("release", sourceName)));
    assert.ok(library, "Cargo emitted the release library in the configured directory");
    assert.ok(readFileSync(destination).equals(readFileSync(join(target, library))),
      "installed addon must match the fresh build even when target/release contains an older library");
  });
});
