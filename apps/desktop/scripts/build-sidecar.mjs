#!/usr/bin/env node
// Build the `weekmark` CLI (crates owned by the backend) and put it where Tauri's
// `bundle.externalBin: ["binaries/weekmark"]` expects it:
//
//   src-tauri/binaries/weekmark-<target-triple>[.exe]
//
// Tauri copies it next to the app executable (macOS: Weekmark.app/Contents/MacOS/weekmark),
// and the "Connect your AI app" snippets point there (src-tauri/src/backend.rs). The app never
// runs it itself: AI apps start `weekmark mcp`.
//
//   pnpm run build:sidecar                 # release build for TAURI_ENV_TARGET_TRIPLE or the host
//   node scripts/build-sidecar.mjs --dev   # quick debug build for the host (`tauri dev`)
//
// Wired into src-tauri/tauri.conf.json: beforeBuildCommand (release, so `tauri build --target …`
// gets the matching sidecar) and beforeDevCommand (--dev). tauri-build refuses to compile the
// desktop crate while the file is missing, so on a fresh checkout run it once before
// `cargo clippy/test -p weekmark-desktop` (CI does: .github/workflows/ci.yml).

import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(desktopDir, "../..");
const outDir = join(desktopDir, "src-tauri/binaries");
// Respect a custom cargo target dir (CI caches may set it).
const targetDir = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : join(repoRoot, "target");
const dev = process.argv.includes("--dev");
const env = { ...process.env, PATH: `${join(homedir(), ".cargo/bin")}:${process.env.PATH}` };

function run(cmd, args) {
  execFileSync(cmd, args, { cwd: repoRoot, env, stdio: ["ignore", "inherit", "inherit"] });
}

function hostTriple() {
  const info = execFileSync("rustc", ["-vV"], { env, encoding: "utf8" });
  const host = /^host: (\S+)$/m.exec(info)?.[1];
  if (!host) throw new Error("could not read the host target triple from `rustc -vV`");
  return host;
}

const triple = process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
const exe = triple.includes("windows") ? ".exe" : "";
const dest = join(outDir, `weekmark-${triple}${exe}`);
mkdirSync(outDir, { recursive: true });

/** Build for one real target triple; returns the path of the built binary. */
function build(target) {
  if (dev) {
    // Dev builds are for the host only and reuse the workspace's debug build.
    run("cargo", ["build", "-p", "weekmark-cli"]);
    return join(targetDir, "debug", `weekmark${exe}`);
  }
  run("cargo", ["build", "--release", "-p", "weekmark-cli", "--target", target]);
  return join(targetDir, target, "release", `weekmark${exe}`);
}

if (triple === "universal-apple-darwin" && !dev) {
  // A universal macOS app needs a universal sidecar: build both and merge them.
  const parts = ["aarch64-apple-darwin", "x86_64-apple-darwin"].map(build);
  run("lipo", ["-create", "-output", dest, ...parts]);
} else {
  copyFileSync(build(dev ? hostTriple() : triple), dest);
}
chmodSync(dest, 0o755);
console.log(`sidecar ready: ${dest}`);
