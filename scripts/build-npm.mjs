#!/usr/bin/env node
// Assembles the npm packages for release into <out>/:
//   - <out>/<suffix>/       one per platform, each carrying the compiled binary
//   - <out>/main/           the @provablehq/aleo-devnode launcher package
//
// Inputs are the release binaries the GitHub Actions build matrix produced,
// laid out as <artifacts>/<rust-target>/aleo-devnode[.exe].
//
// Usage:
//   node scripts/build-npm.mjs --version 0.2.0 --artifacts bins --out dist-npm
//
// The publish step (release.yml) publishes every platform package first, then
// main last, so main's exact-pinned optionalDependencies always resolve.

import { existsSync, mkdirSync, copyFileSync, writeFileSync, chmodSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCOPE = "@provablehq";
const BASE = "aleo-devnode";
const LICENSE = "Apache-2.0";
const REPO = "https://github.com/ProvableHQ/aleo-devnode";

// Single source of truth for the target matrix. `target` must match the
// directory names produced by the release build matrix (the Rust triples).
const PLATFORMS = [
  { suffix: "darwin-arm64", target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { suffix: "darwin-x64", target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { suffix: "linux-x64", target: "x86_64-unknown-linux-gnu", os: "linux", cpu: "x64" },
  { suffix: "linux-arm64", target: "aarch64-unknown-linux-gnu", os: "linux", cpu: "arm64" },
  { suffix: "win32-x64", target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64" },
];

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    if (!key?.startsWith("--")) throw new Error(`unexpected argument: ${key}`);
    args[key.slice(2)] = argv[i + 1];
  }
  return args;
}

const { version, artifacts, out } = parseArgs(process.argv.slice(2));
if (!version || !artifacts || !out) {
  console.error("usage: build-npm.mjs --version <v> --artifacts <dir> --out <dir>");
  process.exit(1);
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const shimSrc = resolve(scriptDir, "..", "npm", "run.js");
const readmeSrc = resolve(scriptDir, "..", "npm", "README.md");
const artifactsDir = resolve(artifacts);
const outDir = resolve(out);

const writeJson = (path, obj) => writeFileSync(path, JSON.stringify(obj, null, 2) + "\n");

// --- platform packages -----------------------------------------------------
for (const p of PLATFORMS) {
  const binName = p.os === "win32" ? "aleo-devnode.exe" : "aleo-devnode";
  const binSrc = join(artifactsDir, p.target, binName);
  if (!existsSync(binSrc)) {
    throw new Error(`missing binary for ${p.target}: expected ${binSrc}`);
  }

  const pkgDir = join(outDir, p.suffix);
  const binDir = join(pkgDir, "bin");
  mkdirSync(binDir, { recursive: true });

  copyFileSync(binSrc, join(binDir, binName));
  if (p.os !== "win32") chmodSync(join(binDir, binName), 0o755);

  writeJson(join(pkgDir, "package.json"), {
    name: `${SCOPE}/${BASE}-${p.suffix}`,
    version,
    description: `Prebuilt aleo-devnode binary for ${p.suffix}`,
    license: LICENSE,
    repository: { type: "git", url: `git+${REPO}.git` },
    os: [p.os],
    cpu: [p.cpu],
    files: ["bin/"],
  });
  console.log(`built ${SCOPE}/${BASE}-${p.suffix}`);
}

// --- main launcher package -------------------------------------------------
const mainDir = join(outDir, "main");
mkdirSync(mainDir, { recursive: true });
copyFileSync(shimSrc, join(mainDir, "run.js"));
copyFileSync(readmeSrc, join(mainDir, "README.md"));

writeJson(join(mainDir, "package.json"), {
  name: `${SCOPE}/${BASE}`,
  version,
  description: "Local Aleo development node (prebuilt binary distribution)",
  license: LICENSE,
  repository: { type: "git", url: `git+${REPO}.git` },
  bin: { "aleo-devnode": "run.js" },
  files: ["run.js", "README.md"],
  engines: { node: ">=18" },
  optionalDependencies: Object.fromEntries(
    PLATFORMS.map((p) => [`${SCOPE}/${BASE}-${p.suffix}`, version]),
  ),
});
console.log(`built ${SCOPE}/${BASE} (main)`);

// Sanity check: the shim's platform table must match this script's matrix,
// otherwise a published platform package would never be resolved at runtime.
const shim = readFileSync(shimSrc, "utf8");
for (const p of PLATFORMS) {
  if (!shim.includes(`"${p.suffix}"`)) {
    throw new Error(`npm/run.js is missing platform "${p.suffix}" — matrix drift`);
  }
}
