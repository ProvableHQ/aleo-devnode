#!/usr/bin/env node
"use strict";

// Thin launcher for the aleo-devnode binary. The actual platform-specific
// binary ships in an optional dependency (e.g. @provablehq/aleo-devnode-darwin-arm64);
// npm installs only the one matching the host's os/cpu. This shim resolves that
// package and execs the binary, forwarding argv, stdio, and the exit code.

const { spawnSync } = require("node:child_process");

// suffix -> [process.platform, process.arch]
const PLATFORMS = {
  "darwin-arm64": ["darwin", "arm64"],
  "darwin-x64": ["darwin", "x64"],
  "linux-x64": ["linux", "x64"],
  "linux-arm64": ["linux", "arm64"],
  "win32-x64": ["win32", "x64"],
};

function resolveBinary() {
  const suffix = Object.keys(PLATFORMS).find(
    (k) => PLATFORMS[k][0] === process.platform && PLATFORMS[k][1] === process.arch,
  );
  if (!suffix) {
    return { error: `unsupported platform: ${process.platform}-${process.arch}` };
  }
  const pkg = `@provablehq/aleo-devnode-${suffix}`;
  const binName = process.platform === "win32" ? "aleo-devnode.exe" : "aleo-devnode";
  try {
    return { path: require.resolve(`${pkg}/bin/${binName}`) };
  } catch {
    return {
      error:
        `the platform package "${pkg}" is not installed.\n` +
        `It is normally installed automatically as an optional dependency; a\n` +
        `"--no-optional" / "--omit=optional" install will skip it. Reinstall\n` +
        `without that flag, or add the package directly:\n` +
        `  npm install ${pkg}`,
    };
  }
}

const resolved = resolveBinary();
if (resolved.error) {
  console.error(`aleo-devnode: ${resolved.error}`);
  process.exit(1);
}

const result = spawnSync(resolved.path, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`aleo-devnode: failed to launch binary: ${result.error.message}`);
  process.exit(1);
}
// Propagate the child's exit code; if it was killed by a signal, exit non-zero.
process.exit(typeof result.status === "number" ? result.status : 1);
