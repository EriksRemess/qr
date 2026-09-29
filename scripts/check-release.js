import { access, readFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const packageJson = JSON.parse(await readFile(new URL("package.json", root), "utf8"));
const expectedTag = `v${packageJson.version}`;

if (process.env.GITHUB_EVENT_NAME === "release" && process.env.GITHUB_REF_NAME !== expectedTag) {
  throw new Error(
    `Release tag ${JSON.stringify(process.env.GITHUB_REF_NAME)} must match ${expectedTag}`,
  );
}

const nativeArtifacts = [
  "qr-native-linux-x64-gnu.node",
  "qr-native-darwin-arm64.node",
];
await Promise.all(nativeArtifacts.map((filename) => access(new URL(`native/${filename}`, root))));

console.log(`Ready to publish ${packageJson.name}@${packageJson.version}`);
console.log(`Native artifacts: ${nativeArtifacts.join(", ")}`);
