#!/usr/bin/env node
// How much of .github/workflows/ci.yml a pull request needs, from the files it changes (the
// `changes` job). Node 24, no dependencies. Tests: `node --test .github/scripts/*.test.mjs`.
//
//   docs   only documentation: no build and no test reads these files, so the Rust, desktop
//          and macOS app jobs are skipped.
//   tests  only tests (and documentation): the Rust and desktop jobs run on Linux alone; the
//          macOS app job runs when something of its own changed.
//   full   anything else, and every run that isn't for a pull request (a push to main, a manual
//          run): every job on every OS.
//
// Unsure is "full": no list of files, an empty one, a failed git call.
//
// A change to tests that depends on the OS can be run in full before it is merged:
//   gh workflow run ci.yml --ref <branch>

import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const ALL_OSES = ["ubuntu-22.04", "macos-latest", "windows-latest"];
export const LINUX_ONLY = ["ubuntu-22.04"];

/** A test, a test's fixture or a test's helper. Checked before `isDoc`: a fixture can be Markdown. */
export function isTest(path) {
  return (
    // crates/*/tests/, src-tauri/tests/, apps/macos/Tests/, design/tokens/test/, src/test/ …
    /(^|\/)[Tt]ests?\//.test(path) ||
    /\.test\.(ts|tsx|js|mjs)$/.test(path) ||
    // A crate's own test modules: src/tests.rs, src/tests_sync.rs.
    /(^|\/)tests(_[a-z0-9_]+)?\.rs$/.test(path)
  );
}

/** Documentation that nothing builds from and no test reads. */
export function isDoc(path) {
  return !isTest(path) && (/\.md$/i.test(path) || path.startsWith("docs/") || path === "LICENSE");
}

/** What the macOS app job builds or tests from, besides the Rust core. */
function isMacosApp(path) {
  return path.startsWith("apps/macos/") || path.startsWith("design/tokens/");
}

/**
 * `files`: the paths a pull request changes, or null when that isn't known.
 * Returns the scope, the OSes the Rust and desktop jobs run on, and whether the macOS app job runs.
 */
export function scopeOf(files) {
  if (!files || files.length === 0) {
    return { scope: "full", oses: ALL_OSES, macosApp: true };
  }
  if (files.every(isDoc)) {
    return { scope: "docs", oses: [], macosApp: false };
  }
  if (files.every((path) => isDoc(path) || isTest(path))) {
    return { scope: "tests", oses: LINUX_ONLY, macosApp: files.some(isMacosApp) };
  }
  return { scope: "full", oses: ALL_OSES, macosApp: true };
}

/** The files this pull request changes: the merge commit against the base it was merged into. */
function changedFiles() {
  if (process.env.GITHUB_EVENT_NAME !== "pull_request") return null;
  try {
    const out = execFileSync(
      "git",
      ["diff", "--name-only", "--no-renames", "-z", "HEAD^1", "HEAD"],
      { encoding: "utf8" },
    );
    return out.split("\0").filter(Boolean);
  } catch {
    return null;
  }
}

function main() {
  const files = changedFiles();
  const { scope, oses, macosApp } = scopeOf(files);
  const lines = [`scope=${scope}`, `oses=${JSON.stringify(oses)}`, `macos_app=${macosApp}`];
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${lines.join("\n")}\n`);
  console.log(
    files
      ? `${files.length} changed file(s): ${scope}`
      : `not a pull request, or its files aren't known: ${scope}`,
  );
  for (const line of lines) console.log(`  ${line}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main();
