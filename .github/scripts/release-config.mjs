#!/usr/bin/env node
// Release configuration checks for the `release-config` job of .github/workflows/ci.yml and the
// `preflight` job of .github/workflows/release.yml. Node 24, no dependencies. Tests:
// release-config.test.mjs (the updater-manifest.mjs tests run next to it in both jobs).
//
//   node .github/scripts/release-config.mjs [--strict] [--root <repo>]
//
// Always errors (CI and release):
// - a secret used anywhere but a step's `env:` or `with:` in any workflow (job- or workflow-level
//   `env`, `run` scripts, `if`, `secrets: inherit`, …): build scripts, proc macros and package
//   lifecycle hooks can read everything in a job's environment (docs/design/v0.3-plan.md M0.6);
// - an action not pinned to a full commit SHA (the tj-actions compromise);
// - macOS entitlements in any Tauri config (the app and its sidecar ship with none, M0.1);
// - `createUpdaterArtifacts: "v1Compatible"` (only for apps migrating from Tauri v1).
// Warnings in CI, errors with --strict (release.yml, tags only; rehearsals run without it):
// - the updater is switched on (`bundle.createUpdaterArtifacts: true`) but
//   `plugins.updater.pubkey` isn't a real updater public key;
// - `tauri.conf.json`'s version is neither the Cargo workspace version nor its numeric part (the
//   check of release.yml's create step, earlier);
// - `bundle.targets` is "all" (the default) or has "msi" (NSIS only, D4). Strict only once the
//   updater is switched on: a release made without the updater still builds what v0.1.0 did.
//
// Writes `updater=true|false` (updater artifacts will be built and published: switched on with a
// real key), `version=<Cargo workspace version>` and `test_overlay=true|false`
// (src-tauri/tauri.rehearsal.conf.json exists; rehearsals build with it) to $GITHUB_OUTPUT.

import { appendFileSync, existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { isRealPublicKey } from "./updater-manifest.mjs";

const TAURI_DIR = "apps/desktop/src-tauri";
const WORKFLOWS_DIR = ".github/workflows";
export const TEST_OVERLAY = `${TAURI_DIR}/tauri.rehearsal.conf.json`;

// ---- workflows ----------------------------------------------------------------------------------

const indentOf = (line) => /^ */.exec(line)[0].length;

function stripComment(value) {
  let quote = null;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
    } else if (c === "#" && (i === 0 || /\s/.test(value[i - 1]))) {
      return value.slice(0, i).trimEnd();
    }
  }
  return value.trimEnd();
}

const unquote = (s) => (/^(".*"|'.*')$/.test(s) ? s.slice(1, -1) : s);

/**
 * A structural scan of a block-style YAML file (GitHub workflow files), enough to tell where each
 * value sits: one entry per mapping key (`{ line, path, key, value }`) and per other line
 * (`{ line, path, text }`: block-scalar content such as `run: |` scripts, plain list items).
 * `path` is the list of keys from the top, with "[]" for a sequence item.
 */
export function scanYaml(text) {
  const entries = [];
  const stack = []; // { indent, key }
  let block = null; // the key that owns the block scalar being read: { indent, path }
  const lines = text.split(/\r?\n/);
  lines.forEach((raw, index) => {
    const line = index + 1;
    if (block) {
      if (raw.trim() === "" || indentOf(raw) > block.indent) {
        if (raw.trim() !== "") entries.push({ line, path: block.path, text: raw.trim() });
        return;
      }
      block = null;
    }
    const trimmed = raw.trim();
    if (trimmed === "" || trimmed.startsWith("#") || trimmed === "---" || trimmed === "...") return;
    let indent = indentOf(raw);
    let rest = raw.slice(indent);
    // Sequence items ("- x", also "- - x"): a "[]" level at the dash, the content after it.
    let dash;
    while ((dash = /^-( +|$)/.exec(rest))) {
      while (stack.length && (stack.at(-1).indent > indent || (stack.at(-1).indent === indent && stack.at(-1).key === "[]"))) {
        stack.pop();
      }
      stack.push({ indent, key: "[]" });
      indent += dash[0].length;
      rest = rest.slice(dash[0].length);
      if (rest === "") return;
    }
    const match = /^("[^"]*"|'[^']*'|[^\s"'#][^:#]*?)\s*:(?:\s+(.*)|)$/.exec(rest);
    if (!match) {
      entries.push({ line, path: stack.map((e) => e.key), text: stripComment(rest) });
      return;
    }
    while (stack.length && stack.at(-1).indent >= indent) stack.pop();
    const key = unquote(match[1]);
    const value = stripComment(match[2] ?? "");
    const path = [...stack.map((e) => e.key), key];
    entries.push({ line, path, key, value });
    if (/^[|>][0-9+-]*$/.test(value)) block = { indent, path };
    stack.push({ indent, key });
  });
  return entries;
}

const EXPRESSION_SECRET = /\$\{\{[^}]*\bsecrets\b[^}]*\}\}/;
const BARE_SECRET = /\bsecrets\s*[.[]/;

/** Where a secret may appear: a step's env or with (`jobs.<id>.steps[].env.<NAME>`). */
function allowedSecretPath(path) {
  return path.length === 6 && path[0] === "jobs" && path[2] === "steps" && path[3] === "[]" && (path[4] === "env" || path[4] === "with");
}

/** Every use of a secret outside a step's `env:`/`with:` (and `secrets:` on a job). */
export function secretProblems(file, text) {
  const problems = [];
  for (const entry of scanYaml(text)) {
    const value = entry.value ?? entry.text ?? "";
    if (entry.key === "secrets" && entry.path.length === 3 && entry.path[0] === "jobs") {
      problems.push(`${file}:${entry.line}: jobs.${entry.path[1]} passes secrets to a reusable workflow (secrets: ${value || "…"}); pass each secret to the step that needs it`);
      continue;
    }
    const used = EXPRESSION_SECRET.test(value) || (entry.key === "if" && BARE_SECRET.test(value));
    if (used && !allowedSecretPath(entry.path)) {
      problems.push(`${file}:${entry.line}: a secret is used in ${entry.path.join(".").replaceAll(".[]", "[]")}; secrets may only be given to the step that needs them, in its env: or with:`);
    }
  }
  return problems;
}

/** Every `uses:` that isn't a local action or pinned to a full commit SHA (or image digest). */
export function pinningProblems(file, text) {
  const problems = [];
  for (const entry of scanYaml(text)) {
    if (entry.key !== "uses") continue;
    const ref = unquote(entry.value);
    if (ref.startsWith("./")) continue;
    if (ref.startsWith("docker://") ? /@sha256:[0-9a-f]{64}$/.test(ref) : /^[^@\s]+@[0-9a-f]{40}$/.test(ref)) continue;
    problems.push(`${file}:${entry.line}: '${ref}' isn't pinned to a full commit SHA (add the version as a comment)`);
  }
  return problems;
}

// ---- Tauri configuration ------------------------------------------------------------------------

/** The `[workspace.package] version` of the root Cargo.toml. */
export function cargoWorkspaceVersion(cargoToml) {
  let inSection = false;
  for (const line of cargoToml.split(/\r?\n/)) {
    const header = /^\s*\[([^\]]+)\]\s*(#.*)?$/.exec(line);
    if (header) {
      inSection = header[1].trim() === "workspace.package";
      continue;
    }
    const version = inSection && /^\s*version\s*=\s*"([^"]+)"/.exec(line);
    if (version) return version[1];
  }
  return undefined;
}

/**
 * The Tauri checks. `configs` maps each file name in src-tauri (`tauri.conf.json`,
 * `tauri.windows.conf.json`, overlays, …) to its parsed content. Returns `{ errors, warnings,
 * updater }`.
 */
export function tauriFindings({ configs, cargoVersion, strict }) {
  const errors = [];
  const warnings = [];
  const base = configs["tauri.conf.json"];
  if (!base) return { errors: [`${TAURI_DIR}/tauri.conf.json is missing`], warnings, updater: false };

  for (const [name, config] of Object.entries(configs)) {
    const entitlements = config?.bundle?.macOS?.entitlements;
    if (entitlements !== undefined && entitlements !== null && entitlements !== "") {
      errors.push(`${TAURI_DIR}/${name}: bundle.macOS.entitlements is set; PageLamp and its sidecar are signed with no entitlements (a file here would reach both)`);
    }
  }

  const artifacts = base.bundle?.createUpdaterArtifacts;
  if (typeof artifacts === "string") {
    errors.push(`${TAURI_DIR}/tauri.conf.json: bundle.createUpdaterArtifacts is "${artifacts}"; use true (v1Compatible is only for apps migrating from Tauri v1, and the manifest expects v2 artifacts)`);
  }
  for (const [name, config] of Object.entries(configs)) {
    if (name !== "tauri.conf.json" && config?.bundle?.createUpdaterArtifacts !== undefined) {
      errors.push(`${TAURI_DIR}/${name}: sets bundle.createUpdaterArtifacts; switch the updater on or off in tauri.conf.json only`);
    }
  }
  const switchedOn = artifacts === true;
  const pubkey = base.plugins?.updater?.pubkey;
  const realKey = isRealPublicKey(pubkey);
  const release = (message) => (strict ? errors : warnings).push(message);
  // D4 only blocks a release once the updater is on (see the header).
  const d4 = (message) => (strict && switchedOn ? errors : warnings).push(message);

  if (switchedOn && !realKey) {
    release(`${TAURI_DIR}/tauri.conf.json: the updater is on (createUpdaterArtifacts) but plugins.updater.pubkey isn't a real updater public key (the content of the .key.pub file from \`tauri signer generate\`); the release builds no updater artifacts until it is`);
  }
  // As release.yml's create step: the full Cargo version, its numeric part, or none (Cargo's).
  if (base.version !== undefined && cargoVersion && base.version !== cargoVersion && base.version !== cargoVersion.split("-")[0]) {
    release(`${TAURI_DIR}/tauri.conf.json: version ${base.version} is neither the Cargo workspace version ${cargoVersion} nor its numeric part`);
  }

  // What Windows builds: tauri.windows.conf.json's targets replace the base ones there.
  const windowsTargets = configs["tauri.windows.conf.json"]?.bundle?.targets;
  const name = windowsTargets === undefined ? "tauri.conf.json" : "tauri.windows.conf.json";
  const targets = windowsTargets ?? base.bundle?.targets ?? "all";
  const list = Array.isArray(targets) ? targets : [targets];
  if (list.includes("all")) {
    d4(`${TAURI_DIR}/${name}: bundle.targets is ${base.bundle?.targets === undefined && windowsTargets === undefined ? "not set (all)" : '"all"'}, which builds an MSI too; list the targets without "msi" (NSIS only on Windows, D4)`);
  } else if (list.includes("msi")) {
    d4(`${TAURI_DIR}/${name}: bundle.targets has "msi"; ship NSIS only on Windows (D4)`);
  }

  return { errors, warnings, updater: switchedOn && realKey };
}

// ---- main ---------------------------------------------------------------------------------------

function readJson(path, errors) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    errors.push(`${path}: ${err.message}`);
    return undefined;
  }
}

export function run({ root = ".", strict = false, env = process.env, log = console.log } = {}) {
  const errors = [];
  const warnings = [];

  const workflowDir = join(root, WORKFLOWS_DIR);
  const workflows = readdirSync(workflowDir).filter((name) => /\.ya?ml$/.test(name)).sort();
  for (const name of workflows) {
    const text = readFileSync(join(workflowDir, name), "utf8");
    const file = `${WORKFLOWS_DIR}/${name}`;
    errors.push(...secretProblems(file, text), ...pinningProblems(file, text));
  }

  const cargoVersion = cargoWorkspaceVersion(readFileSync(join(root, "Cargo.toml"), "utf8"));
  if (!cargoVersion) errors.push("Cargo.toml has no [workspace.package] version");

  const configs = {};
  const tauriDir = join(root, TAURI_DIR);
  for (const name of readdirSync(tauriDir).filter((n) => /^tauri(\.[\w-]+)*\.conf\.json$/.test(n)).sort()) {
    const config = readJson(join(tauriDir, name), errors);
    if (config !== undefined) configs[name] = config;
  }
  const tauri = tauriFindings({ configs, cargoVersion, strict });
  errors.push(...tauri.errors);
  warnings.push(...tauri.warnings);
  const testOverlay = existsSync(join(root, TEST_OVERLAY));

  const annotate = (level, message) => log(env.GITHUB_ACTIONS ? `::${level}::${message}` : `${level}: ${message}`);
  for (const message of warnings) annotate("warning", message);
  for (const message of errors) annotate("error", message);
  log(
    `Checked ${workflows.length} workflows and ${Object.keys(configs).length} Tauri config file(s)${strict ? " (strict: release)" : ""}: ` +
      `${errors.length} error(s), ${warnings.length} warning(s). Updater artifacts: ${tauri.updater ? "on" : "off"}; ` +
      `version ${cargoVersion ?? "?"}; rehearsal overlay: ${testOverlay ? "yes" : "no"}.`,
  );
  if (env.GITHUB_OUTPUT) {
    appendFileSync(env.GITHUB_OUTPUT, `updater=${tauri.updater}\nversion=${cargoVersion ?? ""}\ntest_overlay=${testOverlay}\n`);
  }
  return { errors, warnings, updater: tauri.updater, version: cargoVersion, testOverlay };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  let root = ".";
  let strict = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--strict") strict = true;
    else if (args[i] === "--root" && i + 1 < args.length) root = args[++i];
    else {
      console.error(`usage: release-config.mjs [--strict] [--root <repo>] (unknown argument ${args[i]})`);
      process.exit(2);
    }
  }
  process.exitCode = run({ root, strict }).errors.length ? 1 : 0;
}
