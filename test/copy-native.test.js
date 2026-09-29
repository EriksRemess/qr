import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const filename = process.platform === "darwin"
  ? "qr-native-darwin-arm64.node" : "qr-native-linux-x64-gnu.node";
const sourceName = process.platform === "darwin" ? "libqr_native.dylib" : "libqr_native.so";

function fixture(run) {
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
    run({ root, script, source, destination });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("native rebuild atomically replaces the file without invalidating loaded code", () => {
  fixture(({ root, script, source, destination }) => {
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
      await import(pathToFileURL(${JSON.stringify(script)}));
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
  fixture(({ root, script, source, destination }) => {
    copyFileSync(source, destination);
    const before = statSync(destination).ino;
    const bytes = readFileSync(destination);
    rmSync(source);
    const result = spawnSync(process.execPath, [script], { timeout: 15_000 });
    assert.notEqual(result.status, 0);
    assert.equal(statSync(destination).ino, before);
    assert.deepEqual(readFileSync(destination), bytes);
    assert.deepEqual(readdirSync(join(root, "native")), [filename]);
  });
});

test("first native copy installs a binary and cleans staging files", () => {
  fixture(({ root, script, source, destination }) => {
    assert.equal(existsSync(destination), false);
    const result = spawnSync(process.execPath, [script], { timeout: 15_000 });
    assert.equal(result.status, 0, result.stderr?.toString());
    assert.deepEqual(readFileSync(destination), readFileSync(source));
    assert.deepEqual(readdirSync(join(root, "native")), [filename]);
  });
});
