#!/usr/bin/env node
// Checks the Swift sources against the generated string table (spec §7.3):
//   1. every string key written as a literal exists in L10nKeys.all (a literal that starts
//      with an i18n namespace, e.g. "mac.nav.thisWeek" or "common.sync.done"); a key built by
//      interpolation ("common.policy.\(x)") must at least be the prefix of some key;
//   2. no user-visible English (or Chinese) is hard-coded in the view layer: SwiftUI initializers
//      and modifiers that show text must not get a string literal with letters (Text(verbatim:)
//      is the explicit escape hatch for non-localizable text).
// Zero dependencies (Node ≥ 24). Exit 1 with one line per problem.
//
//   node apps/macos/scripts/check-swift-strings.mjs [--root <repo>]

import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const rootIndex = args.indexOf("--root");
const ROOT = rootIndex >= 0 ? resolve(args[rootIndex + 1]) : resolve(SCRIPT_DIR, "..", "..", "..");
const MACOS = join(ROOT, "apps/macos");
const KEYS_FILE = join(MACOS, "Sources/PageLamp/Generated/L10nKeys.swift");

/** The `all` set of L10nKeys.swift. */
function loadKeys() {
  const text = readFileSync(KEYS_FILE, "utf8");
  const start = text.indexOf("static let all: Set<String> = [");
  const end = text.indexOf("]", start);
  if (start < 0 || end < 0) throw new Error(`${KEYS_FILE}: no "static let all" set`);
  return new Set([...text.slice(start, end).matchAll(/"([^"]+)"/g)].map((m) => m[1]));
}

function swiftFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (name === "Generated" || name === ".build") continue;
      out.push(...swiftFiles(path));
    } else if (name.endsWith(".swift")) {
      out.push(path);
    }
  }
  return out;
}

/** Source without comments (keeps line breaks, so line numbers stay right). */
function stripComments(source) {
  let out = "";
  let i = 0;
  let inString = false;
  while (i < source.length) {
    const c = source[i];
    const next = source[i + 1];
    if (inString) {
      out += c;
      if (c === "\\") {
        out += next ?? "";
        i += 2;
        continue;
      }
      if (c === '"' || c === "\n") inString = false;
      i += 1;
    } else if (c === '"') {
      inString = true;
      out += c;
      i += 1;
    } else if (c === "/" && next === "/") {
      while (i < source.length && source[i] !== "\n") i += 1;
    } else if (c === "/" && next === "*") {
      i += 2;
      while (i < source.length && !(source[i] === "*" && source[i + 1] === "/")) {
        if (source[i] === "\n") out += "\n";
        i += 1;
      }
      i += 2;
    } else {
      out += c;
      i += 1;
    }
  }
  return out;
}

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

const keys = loadKeys();
const namespaces = [...new Set([...keys].map((k) => k.split(".")[0]))];
const keyLiteral = new RegExp(`"((?:${namespaces.join("|")})\\.[A-Za-z0-9_.\\-]+)"`, "g");
const keyPrefix = new RegExp(`"((?:${namespaces.join("|")})\\.[A-Za-z0-9_.\\-]*)\\\\\\(`, "g");

// Initializers/modifiers whose string-literal argument would be shown as-is (LocalizedStringKey
// from the main bundle, which has no strings: English on screen in every language).
const TEXT_INITS = [
  "Text", "Button", "Label", "Toggle", "Picker", "Section", "Menu", "CommandMenu", "Tab",
  "LabeledContent", "Window", "WindowGroup", "ContentUnavailableView", "Link", "TextField",
  "SecureField", "DisclosureGroup", "GroupBox", "Stepper", "DatePicker",
];
const TEXT_MODIFIERS = [
  "navigationTitle", "navigationSubtitle", "help", "accessibilityLabel", "accessibilityHint",
  "accessibilityValue", "alert", "confirmationDialog", "badge",
];
const hardCoded = new RegExp(
  `(?:(?<![\\w.])(?:${TEXT_INITS.join("|")})|\\.(?:${TEXT_MODIFIERS.join("|")}))\\(\\s*"([^"]*[A-Za-z\\u3400-\\u9fff][^"]*)"`,
  "g",
);
// UI code: everything under Sources except the facade bindings and the model's synthetic data.
const UI_DIRS = ["Sources/PageLamp", "Sources/PageLampApp", "Sources/PageLampSnapshots"];

const problems = [];
let checkedLiterals = 0;
for (const file of swiftFiles(join(MACOS, "Sources"))) {
  const rel = relative(ROOT, file);
  if (rel.startsWith("apps/macos/Sources/PageLampKit/")) continue;
  const source = stripComments(readFileSync(file, "utf8"));
  for (const m of source.matchAll(keyLiteral)) {
    checkedLiterals += 1;
    if (!keys.has(m[1])) problems.push(`${rel}:${lineOf(source, m.index)}: unknown string key "${m[1]}"`);
  }
  for (const m of source.matchAll(keyPrefix)) {
    checkedLiterals += 1;
    if (![...keys].some((k) => k.startsWith(m[1]))) {
      problems.push(`${rel}:${lineOf(source, m.index)}: no string key starts with "${m[1]}"`);
    }
  }
  if (UI_DIRS.some((dir) => rel.startsWith(`apps/macos/${dir}/`))) {
    for (const m of source.matchAll(hardCoded)) {
      problems.push(`${rel}:${lineOf(source, m.index)}: hard-coded text "${m[1]}" (use l10n("…") or Text(verbatim:))`);
    }
  }
}

if (problems.length > 0) {
  for (const p of problems) console.error(p);
  console.error(`check-swift-strings: ${problems.length} problem(s)`);
  process.exit(1);
}
console.log(`check-swift-strings: ${checkedLiterals} key literals OK, no hard-coded UI text (${keys.size} keys)`);
