#!/usr/bin/env node
// Writes THIRD-PARTY-NOTICES.txt: the licences of what PageLamp's installers and command-line
// archives contain that isn't PageLamp's own. Node 24, no dependencies; it uses the tools the
// builds already use (cargo and pnpm). Tests: third-party-notices.test.mjs.
//
//   node .github/scripts/third-party-notices.mjs [--check] [--root <repo>]
//
// What it lists:
// - the Rust crates the released programs are built from: every package the `ROOTS` reach
//   through normal and build dependencies (not dev-dependencies), for each released target
//   (`cargo metadata --locked --filter-platform`);
// - the npm packages the desktop app's frontend is built from: its production dependencies
//   (`pnpm licenses list --prod`), the bundled Inter font among them;
// - what `EXTRAS` names: data compiled into the programs that comes from another project.
//
// Each package's own licence files are printed, each distinct text once. A package that carries
// none gets the files of a package from the same repository, else a standard text of its
// licence taken from the other packages here (`standardTexts`), else it is listed without one.
//
// `--check` writes nothing and fails when the file in the repository differs (CI). The file
// has to be regenerated whenever Cargo.lock or the desktop's pnpm-lock.yaml changes.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

export const OUTPUT = "THIRD-PARTY-NOTICES.txt";
/// The workspace packages that are released: the desktop app (with its sidecar) and the CLI.
export const ROOTS = ["pagelamp-desktop", "pagelamp-cli"];
/// The targets release.yml builds for.
export const TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
  "x86_64-unknown-linux-gnu",
];
const DESKTOP_DIR = "apps/desktop";
/// Data from another project that is compiled into the programs.
export const EXTRAS = [
  {
    name: "models.dev price table",
    version: "",
    license: "MIT",
    repository: "https://github.com/anomalyco/models.dev",
    note: "trimmed into crates/pagelamp-llm/data/prices.json",
    files: ["crates/pagelamp-llm/data/models-dev.LICENSE"],
  },
];

/// A licence, notice or copyright file at a package's top level.
export const isLicenceFile = (name) =>
  /^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICEN[CS]E|PATENTS)([-._ ].*)?$/i.test(name) &&
  !/\.(rs|js|mjs|cjs|ts|json|toml|ya?ml|png|svg)$/i.test(name);

/// A text as it is printed: LF line ends, no trailing spaces, no blank lines at either end.
export const tidy = (text) =>
  text
    .replace(/^﻿/, "")
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.replace(/[ \t]+$/, ""))
    .join("\n")
    .replace(/^\n+|\n+$/g, "");

/// What makes two texts the same text: their words, whatever the line breaks and spacing.
export const fingerprint = (text) =>
  createHash("sha256").update(tidy(text).replace(/\s+/g, " ").toLowerCase()).digest("hex");

/// The licence files of the package in `dir`, by name: `[{ file, text }]`.
export function licenceFiles(dir, alsoFile = null) {
  if (!existsSync(dir)) return [];
  const names = readdirSync(dir).filter((name) => isLicenceFile(name) && statSync(join(dir, name)).isFile());
  if (alsoFile && existsSync(join(dir, alsoFile)) && !names.includes(alsoFile)) names.push(alsoFile);
  return names
    .sort(plainOrder)
    .map((file) => ({ file, text: tidy(readFileSync(join(dir, file), "utf8")) }))
    .filter((entry) => entry.text.length > 0);
}

// ---- Rust ---------------------------------------------------------------------------------------

/// The third-party packages `roots` reach in one `cargo metadata` answer, through normal and
/// build dependencies: `[{ id, name, version, license, repository, dir, licenseFile, authors }]`.
export function reachable(metadata, roots = ROOTS) {
  const packages = new Map(metadata.packages.map((p) => [p.id, p]));
  const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
  const workspace = new Set(metadata.workspace_members);
  const start = [...workspace].filter((id) => roots.includes(packages.get(id).name));
  const missing = roots.filter((name) => !start.some((id) => packages.get(id).name === name));
  if (missing.length) throw new Error(`no workspace package named ${missing.join(", ")}`);
  const seen = new Set();
  const stack = [...start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // (A dependency used only for tests and examples isn't in what is built for release.)
      if (dep.dep_kinds.some((kind) => kind.kind !== "dev")) stack.push(dep.pkg);
    }
  }
  return [...seen]
    .filter((id) => !workspace.has(id))
    .map((id) => {
      const p = packages.get(id);
      return {
        id,
        name: p.name,
        version: p.version,
        license: p.license ?? "",
        repository: p.repository ?? "",
        dir: dirname(p.manifest_path),
        licenseFile: p.license_file ?? null,
        authors: p.authors ?? [],
      };
    });
}

function rustPackages(root) {
  const byId = new Map();
  for (const target of TARGETS) {
    const out = execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--locked", "--filter-platform", target],
      { cwd: root, encoding: "utf8", maxBuffer: 256 * 1024 * 1024, stdio: ["ignore", "pipe", "inherit"] },
    );
    for (const p of reachable(JSON.parse(out))) byId.set(`${p.name} ${p.version}`, p);
  }
  return [...byId.values()].map((p) => ({
    kind: "crate",
    name: p.name,
    version: p.version,
    license: p.license,
    repository: p.repository,
    authors: p.authors,
    files: licenceFiles(p.dir, p.licenseFile),
  }));
}

// ---- npm ----------------------------------------------------------------------------------------

/// `pnpm licenses list --json` (licence → packages) as a flat list, one entry per version.
export function npmEntries(listing) {
  const entries = [];
  for (const [license, packages] of Object.entries(listing)) {
    for (const p of packages) {
      const versions = p.versions ?? [p.version];
      const paths = p.paths ?? [p.path];
      versions.forEach((version, i) => {
        entries.push({
          name: p.name,
          version,
          license: p.license ?? license,
          repository: p.homepage ?? "",
          authors: p.author ? [p.author] : [],
          dir: paths[i] ?? paths[0],
        });
      });
    }
  }
  return entries;
}

function npmPackages(root) {
  const out = execFileSync("pnpm", ["licenses", "list", "--prod", "--json"], {
    cwd: join(root, DESKTOP_DIR),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
  return npmEntries(JSON.parse(out)).map((p) => ({
    kind: "npm",
    name: p.name,
    version: p.version,
    license: p.license,
    repository: p.repository,
    authors: p.authors,
    files: licenceFiles(p.dir),
  }));
}

function extraPackages(root) {
  return EXTRAS.map((extra) => ({
    kind: "other",
    name: extra.name,
    version: extra.version,
    license: extra.license,
    repository: extra.repository,
    note: extra.note,
    authors: [],
    files: extra.files.map((file) => ({ file: file.split("/").pop(), text: tidy(readFileSync(join(root, file), "utf8")) })),
  }));
}

// ---- order ------------------------------------------------------------------------------------

const label = (p) => (p.version ? `${p.name} ${p.version}` : p.name);

/// An order that is the same on every machine: by code unit, never by the locale's rules.
export const plainOrder = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

/// Versions by their numbers ("1.10.0" after "1.9.0"), then as written.
export function versionOrder(a, b) {
  const parts = (v) => v.split(/[.+-]/);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const [m, n] = [x[i] ?? "", y[i] ?? ""];
    const numeric = /^\d+$/.test(m) && /^\d+$/.test(n);
    const order = numeric ? Number(m) - Number(n) : plainOrder(m, n);
    if (order !== 0) return order < 0 ? -1 : 1;
  }
  return 0;
}

const byName = (a, b) => plainOrder(a.name, b.name) || versionOrder(a.version, b.version);

// ---- packages without a licence file ------------------------------------------------------------

/// The licence ids an expression names, in its order ("MIT OR Apache-2.0", "MIT/Apache-2.0").
export const licenceIds = (expression) =>
  expression
    .replace(/[()]/g, " ")
    .split(/\s+(?:OR|AND|WITH)\s+|\//i)
    .map((id) => id.trim())
    .filter(Boolean);

const repositoryKey = (url) =>
  url
    .toLowerCase()
    .replace(/^git\+/, "")
    .replace(/\.git$/, "")
    .replace(/\/+$/, "")
    .replace(/\/tree\/.*$/, "");

/// A licence's text without the lines that name a copyright holder.
export const withoutHolders = (text) =>
  tidy(
    text
      .split("\n")
      .filter((line) => !/^\s*(copyright\b|\(c\)|©)/i.test(line) && !/^\s*all rights reserved\.?\s*$/i.test(line))
      .join("\n")
      .replace(/\n{3,}/g, "\n\n"),
  );

/// The licences that name their holders in the text itself (the others' texts are the same for
/// every project).
const namesHolders = (id) => /^(MIT|BSD-|ISC|0BSD|Zlib)/i.test(id);

/// The ids of an expression in the order a text is looked for: MIT, then Apache-2.0, then
/// the rest as written. (A package under "A OR B" may be used under either.)
const byPreference = (ids) => [
  ...ids.filter((id) => id === "MIT"),
  ...ids.filter((id) => id === "Apache-2.0"),
  ...ids.filter((id) => id !== "MIT" && id !== "Apache-2.0"),
];

/// A standard text per licence id, taken from the packages themselves: of the packages whose
/// licence is exactly that id and that carry one licence file, the text most of them carry
/// (for MIT and the BSD family without its copyright lines, which name each project's own
/// holders). Ids no such package has get none.
export function standardTexts(packages) {
  const tally = new Map();
  for (const p of packages) {
    const ids = licenceIds(p.license);
    if (ids.length !== 1 || p.files.length !== 1) continue;
    const id = ids[0];
    const text = namesHolders(id) ? withoutHolders(p.files[0].text) : p.files[0].text;
    const key = fingerprint(text);
    const forId = tally.get(id) ?? new Map();
    const seen = forId.get(key) ?? { text, count: 0 };
    seen.count += 1;
    forId.set(key, seen);
    tally.set(id, forId);
  }
  // (Files named for one licence of a dual-licensed package count for that licence.)
  for (const p of packages) {
    for (const { file, text } of p.files) {
      const id = /APACHE/i.test(file) ? "Apache-2.0" : null;
      if (!id) continue;
      const key = fingerprint(text);
      const forId = tally.get(id) ?? new Map();
      const seen = forId.get(key) ?? { text, count: 0 };
      seen.count += 1;
      forId.set(key, seen);
      tally.set(id, forId);
    }
  }
  const standard = new Map();
  for (const [id, texts] of [...tally].sort(([a], [b]) => plainOrder(a, b))) {
    const best = [...texts.entries()].sort(([ka, a], [kb, b]) => b.count - a.count || plainOrder(ka, kb))[0][1];
    standard.set(id, best.text);
  }
  return standard;
}

/// Gives every package without a licence file one: the files of a package from the same
/// repository under the same licences; else the standard text of a licence its expression
/// names (MIT first, then Apache-2.0, then as written), with the package's authors as the
/// holders where the text names them. `borrowed` says where a text came from. A package
/// nothing fits keeps no files.
export function fillMissing(packages) {
  const standard = standardTexts(packages);
  // A repository can hold packages under different licences: only one with the same ones
  // lends its files.
  const siblingKey = (p) => `${repositoryKey(p.repository)} ${licenceIds(p.license).sort(plainOrder).join(" ")}`;
  const bySibling = new Map();
  for (const p of packages) {
    if (!p.files.length || !p.repository) continue;
    const key = siblingKey(p);
    // The first by name, so the choice doesn't depend on the order packages were read in.
    const known = bySibling.get(key);
    if (!known || byName(p, known) < 0) bySibling.set(key, p);
  }
  return packages.map((p) => {
    if (p.files.length) return p;
    const sibling = p.repository ? bySibling.get(siblingKey(p)) : null;
    if (sibling) {
      return { ...p, files: sibling.files, borrowed: `the files of ${sibling.name} ${sibling.version}, from the same repository` };
    }
    for (const id of byPreference(licenceIds(p.license))) {
      const text = standard.get(id);
      if (!text) continue;
      const holders = p.authors.length ? p.authors.join(", ") : `the ${p.name} authors`;
      const named = namesHolders(id) ? `Copyright (c) ${holders}\n\n${text}` : text;
      return { ...p, files: [{ file: id, text: named }], borrowed: `the standard text of ${id}; the package carries no licence file` };
    }
    return p;
  });
}

// ---- the file -----------------------------------------------------------------------------------

/// The file's text from the packages (crates, npm packages, extras), whatever order they come in.
export function render(packages) {
  const sorted = [...packages].sort(byName);
  const section = (kind) => sorted.filter((p) => p.kind === kind);
  const line = (p) => {
    const parts = [label(p), p.license || "licence not stated"];
    if (p.repository) parts.push(p.repository);
    if (p.note) parts.push(p.note);
    return `  ${parts.join(" | ")}`;
  };
  const out = [
    "THIRD-PARTY NOTICES",
    "",
    "PageLamp's installers and command-line archives contain software and data from other",
    "projects. This file names them and gives their licences. PageLamp's own licence is in the",
    "file LICENSE.",
    "",
    "This file is generated from the build's lock files; don't edit it by hand:",
    "  node .github/scripts/third-party-notices.mjs",
    "",
    "Part 1 lists what is included. Part 2 gives the licence texts, each text once, with the",
    "packages it belongs to.",
    "",
    "=".repeat(78),
    "PART 1. WHAT IS INCLUDED",
    "=".repeat(78),
    "",
  ];
  const groups = [
    ["crate", "Rust crates"],
    ["npm", "npm packages in the desktop app (the Inter font among them)"],
    ["other", "Other"],
  ];
  for (const [kind, title] of groups) {
    const entries = section(kind);
    if (!entries.length) continue;
    out.push(`${title} (${entries.length})`, "", ...entries.map(line), "");
  }

  // Each distinct text once, in the order of the first package that has it.
  const texts = new Map();
  const without = [];
  for (const p of sorted) {
    if (!p.files.length) {
      without.push(p);
      continue;
    }
    for (const { file, text } of p.files) {
      const key = fingerprint(text);
      const entry = texts.get(key) ?? { text, users: [] };
      entry.users.push(p.borrowed ? `${label(p)} (${p.borrowed})` : `${label(p)} (${file})`);
      texts.set(key, entry);
    }
  }
  out.push("=".repeat(78), "PART 2. LICENCE TEXTS", "=".repeat(78), "");
  let n = 0;
  for (const { text, users } of texts.values()) {
    n += 1;
    out.push("-".repeat(78), `[${n}] ${users.length === 1 ? "Belongs to" : `Belongs to ${users.length} packages`}:`);
    out.push(...users.map((user) => `  ${user}`), "-".repeat(78), "", text, "");
  }
  if (without.length) {
    out.push(
      "-".repeat(78),
      "Packages that carry no licence file, and for whose licence no text is given above:",
      ...without.map(line),
      "-".repeat(78),
      "",
    );
  }
  return `${out.join("\n").replace(/\n+$/, "")}\n`;
}

/// Everything the file is made from, read with cargo and pnpm in the repository at `root`.
export function collect(root) {
  return fillMissing([...rustPackages(root), ...npmPackages(root), ...extraPackages(root)]);
}

export function run({ root = ".", check = false, log = console.log } = {}) {
  const text = render(collect(root));
  const path = join(root, OUTPUT);
  if (check) {
    const current = existsSync(path) ? readFileSync(path, "utf8") : null;
    if (current !== text) {
      log(
        `${OUTPUT} is ${current === null ? "missing" : "out of date"}: a dependency changed. ` +
          "Run `node .github/scripts/third-party-notices.mjs` and commit the result.",
      );
      return { ok: false, text };
    }
    log(`${OUTPUT} is current.`);
    return { ok: true, text };
  }
  writeFileSync(path, text);
  log(`wrote ${OUTPUT} (${text.length} characters)`);
  return { ok: true, text };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  let root = ".";
  let check = false;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--check") check = true;
    else if (args[i] === "--root" && i + 1 < args.length) root = args[++i];
    else {
      console.error(`usage: third-party-notices.mjs [--check] [--root <repo>] (unknown argument ${args[i]})`);
      process.exit(2);
    }
  }
  process.exitCode = run({ root, check }).ok ? 0 : 1;
}
