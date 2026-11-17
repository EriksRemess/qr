import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

for (const [kind, createArtifact] of [
  ["an empty file", path => writeFileSync(path, "")],
  ["a directory", path => mkdirSync(path)],
  ["a symbolic link", (path, source) => symlinkSync(source, path)],
]) {
  test(`release preflight rejects ${kind} in place of a native addon`, t => {
    const root = mkdtempSync(join(tmpdir(), "qr-release-test-"));
    t.after(() => rmSync(root, { recursive: true, force: true }));
    mkdirSync(join(root, "scripts"));
    mkdirSync(join(root, "native"));
    const script = join(root, "scripts", "check-release.js");
    copyFileSync(new URL("../scripts/check-release.js", import.meta.url), script);
    writeFileSync(join(root, "package.json"), JSON.stringify({ type: "module", version: "1.0.0" }));
    const other = join(root, "native", "qr-native-linux-x64-gnu.node");
    writeFileSync(other, "fixture");
    const filename = "qr-native-darwin-arm64.node";
    createArtifact(join(root, "native", filename), other);
    const result = spawnSync(process.execPath, [script], {
      encoding: "utf8", env: { ...process.env, GITHUB_EVENT_NAME: "workflow_dispatch" },
    });
    assert.equal(result.status, 1, result.stderr);
    assert.ok(result.stderr.includes(filename), result.stderr);
  });
}
