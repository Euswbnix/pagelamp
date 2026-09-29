#!/usr/bin/env node
// The Tauri updater manifest (`latest.json`) for .github/workflows/release.yml and
// .github/workflows/channels.yml. Node 24, no dependencies. Tests: updater-manifest.test.mjs.
//
//   node updater-manifest.mjs build --dir <dir> --version <semver> (--tag <tag> [--repo <o/r>] | --base-url <url>)
//        [--platforms <k,k>] [--notes <text> | --notes-file <file>] [--pub-date <rfc3339>]
//        [--pubkey <base64> | --pubkey-config <tauri.conf.json>] [--out <file>]
//   node updater-manifest.mjs check <latest.json> [--version <semver>] (--tag <tag> [--repo <o/r>] | --base-url <url>)
//        [--platforms <k,k>] [--assets <file with one asset name per line>] [--sums <SHA256SUMS>]
//   node updater-manifest.mjs verify <latest.json> --dir <dir> (--pubkey <base64> | --pubkey-config <tauri.conf.json>)
//   node updater-manifest.mjs newer <semver a> <semver b>     prints true if a is newer than b (SemVer precedence)
//   node updater-manifest.mjs enabled <tauri.conf.json>      prints true if that config builds updater artifacts
//
// The contract with the desktop app (apps/desktop, docs/design/v0.3-plan.md M0.4):
// - exactly these platform keys: darwin-aarch64-app and darwin-x86_64-app (both the one universal
//   .app.tar.gz), windows-x86_64-nsis, linux-x86_64-appimage. No generic `{os}-{arch}` key (the
//   updater would fall back to it and, on a .deb/.rpm install, run `dpkg -i` on an AppImage) and
//   no deb/rpm key (those installs get a download link instead, D7);
// - plain https://github.com/<repo>/releases/download/<tag>/<asset> URLs, not API asset URLs
//   (unauthenticated API calls are limited per IP), or `--base-url` + asset for the test channel;
// - `version` is the Cargo workspace version including any pre-release part (the tag without
//   its `v`), and `signature` is the content of the artifact's `.sig` file.
//
// `build` fails if a platform, an artifact's signature or its version binding is missing. Tauri's
// CLI signs updater artifacts with the app version in the signature's trusted comment
// (`version:<tauri.conf.json version>`), and tauri-plugin-updater (2.12+) refuses an update whose
// manifest announces a different version than the signature was made for, so a mismatch fails
// here instead of in every installed app. With a public key, `build` and `verify` also check each
// signature against the artifact's bytes (minisign: Ed25519 over the BLAKE2b-512 hash).

import { createHash, createPublicKey, verify as ed25519Verify } from "node:crypto";
import { closeSync, existsSync, openSync, readdirSync, readFileSync, readSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

/** The manifest's platform keys, in order, and the artifact each one points at. */
export const PLATFORMS = [
  { key: "darwin-aarch64-app", suffix: ".app.tar.gz", label: "macOS updater archive (.app.tar.gz)" },
  { key: "darwin-x86_64-app", suffix: ".app.tar.gz", label: "macOS updater archive (.app.tar.gz)" },
  { key: "windows-x86_64-nsis", suffix: "-setup.exe", label: "Windows NSIS installer (-setup.exe)" },
  { key: "linux-x86_64-appimage", suffix: ".AppImage", label: "Linux AppImage (.AppImage)" },
];
export const PLATFORM_KEYS = PLATFORMS.map((p) => p.key);
const MANIFEST_KEYS = ["notes", "platforms", "pub_date", "version"];

// SemVer 2.0.0 (semver.org), no leading "v".
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

export class ManifestError extends Error {}
const fail = (message) => {
  throw new ManifestError(message);
};

// ---- minisign (the format of Tauri's updater keys and signatures) ------------------------------

function base64(text, what) {
  const value = typeof text === "string" ? text.trim() : "";
  if (!value || value.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value)) {
    fail(`${what} isn't base64`);
  }
  return Buffer.from(value, "base64");
}

/**
 * A Tauri updater public key (`plugins.updater.pubkey`, the content of the `.key.pub` file made by
 * `tauri signer generate`): base64 of a minisign public key file. Throws if it isn't one.
 */
export function decodePublicKey(pubkey) {
  const lines = base64(pubkey, "The updater public key").toString("utf8").split("\n");
  if (!lines[0]?.startsWith("untrusted comment:")) {
    fail("The updater public key isn't a minisign public key (no 'untrusted comment:' line)");
  }
  const bin = base64(lines[1], "The updater public key's key line");
  const algorithm = bin.subarray(0, 2).toString("latin1");
  if (bin.length !== 42 || (algorithm !== "Ed" && algorithm !== "ED")) {
    fail("The updater public key isn't an Ed25519 minisign key");
  }
  return { keyId: bin.subarray(2, 10), key: bin.subarray(10, 42) };
}

/** True when `pubkey` is a real updater public key, not a placeholder or a file path. */
export function isRealPublicKey(pubkey) {
  try {
    decodePublicKey(pubkey);
    return true;
  } catch {
    return false;
  }
}

/**
 * True when a Tauri config builds updater artifacts that release.yml publishes: the updater is
 * switched on (`bundle.createUpdaterArtifacts: true`) and `plugins.updater.pubkey` is a real key.
 */
export function updaterEnabled(config) {
  return config?.bundle?.createUpdaterArtifacts === true && isRealPublicKey(config?.plugins?.updater?.pubkey);
}

/** A Tauri `.sig` file's content: base64 of a minisign signature file. Throws if it isn't one. */
export function decodeSignature(signature, what = "The signature") {
  const lines = base64(signature, what)
    .toString("utf8")
    .split("\n")
    .map((line) => line.replace(/\r$/, ""));
  if (!lines[0]?.startsWith("untrusted comment:")) fail(`${what} isn't a minisign signature`);
  const bin = base64(lines[1], `${what}'s signature line`);
  const algorithm = bin.subarray(0, 2).toString("latin1");
  if (bin.length !== 74 || (algorithm !== "Ed" && algorithm !== "ED")) {
    fail(`${what} isn't an Ed25519 minisign signature`);
  }
  if (!lines[2]?.startsWith("trusted comment: ")) fail(`${what} has no trusted comment`);
  const globalSignature = base64(lines[3], `${what}'s global signature`);
  if (globalSignature.length !== 64) fail(`${what}'s global signature has the wrong length`);
  return {
    prehashed: algorithm === "ED",
    keyId: bin.subarray(2, 10),
    signature: bin.subarray(10, 74),
    trustedComment: lines[2].slice("trusted comment: ".length),
    globalSignature,
  };
}

/** The `version:` field of a trusted comment (`timestamp:…\tfile:…\tversion:…`), if any. */
export function signedVersion(trustedComment) {
  const field = trustedComment.split("\t").find((part) => part.startsWith("version:"));
  return field === undefined ? undefined : field.slice("version:".length);
}

function blake2b512File(path) {
  const hash = createHash("blake2b512");
  const fd = openSync(path, "r");
  try {
    const buffer = Buffer.alloc(1 << 20);
    let read;
    while ((read = readSync(fd, buffer, 0, buffer.length, null)) > 0) {
      hash.update(buffer.subarray(0, read));
    }
  } finally {
    closeSync(fd);
  }
  return hash.digest();
}

/**
 * Checks `signature` (a `.sig` file's content) over `data` (a Buffer, or `{ path }` to hash a file
 * in chunks) with `pubkey`, as tauri-plugin-updater does (minisign-verify). Throws if it fails.
 */
export function verifySignature(data, signature, pubkey, what = "the artifact") {
  const key = decodePublicKey(pubkey);
  const sig = decodeSignature(signature, `The signature of ${what}`);
  if (!sig.keyId.equals(key.keyId)) {
    fail(`The signature of ${what} was made with another key than plugins.updater.pubkey`);
  }
  let message;
  if (sig.prehashed) {
    message = Buffer.isBuffer(data) ? createHash("blake2b512").update(data).digest() : blake2b512File(data.path);
  } else {
    message = Buffer.isBuffer(data) ? data : readFileSync(data.path);
  }
  const publicKey = createPublicKey({
    key: { kty: "OKP", crv: "Ed25519", x: key.key.toString("base64url") },
    format: "jwk",
  });
  if (!ed25519Verify(null, message, publicKey, sig.signature)) {
    fail(`The signature of ${what} doesn't match its content`);
  }
  const global = Buffer.concat([sig.signature, Buffer.from(sig.trustedComment, "utf8")]);
  if (!ed25519Verify(null, global, publicKey, sig.globalSignature)) {
    fail(`The trusted comment in the signature of ${what} was changed after signing`);
  }
}

/** The updater public key from a Tauri config file (`plugins.updater.pubkey`). */
export function publicKeyFromConfig(path) {
  let config;
  try {
    config = JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    fail(`Can't read ${path}: ${err.message}`);
  }
  const pubkey = config?.plugins?.updater?.pubkey;
  if (typeof pubkey !== "string" || !pubkey.trim()) fail(`${path} has no plugins.updater.pubkey`);
  return pubkey;
}

// ---- the manifest -------------------------------------------------------------------------------

export function checkVersion(version) {
  if (typeof version !== "string" || !SEMVER.test(version)) {
    fail(`'${version}' isn't a semantic version (e.g. 0.3.0 or 0.3.0-beta.1, without a leading v)`);
  }
}

/**
 * SemVer 2.0.0 precedence (build metadata ignored): negative if a < b, 0 if equal, positive if
 * a > b. A pre-release is older than its release (0.3.0-beta.2 < 0.3.0).
 */
export function compareVersions(a, b) {
  checkVersion(a);
  checkVersion(b);
  const parse = (v) => {
    const [, major, minor, patch, pre] = SEMVER.exec(v);
    return { core: [major, minor, patch].map(Number), pre: pre === undefined ? [] : pre.split(".") };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) {
    if (x.core[i] !== y.core[i]) return x.core[i] - y.core[i];
  }
  if (!x.pre.length || !y.pre.length) return (x.pre.length ? -1 : 0) + (y.pre.length ? 1 : 0);
  const numeric = /^\d+$/;
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const p = x.pre[i];
    const q = y.pre[i];
    if (p === undefined) return -1;
    if (q === undefined) return 1;
    if (p === q) continue;
    const pn = numeric.test(p);
    const qn = numeric.test(q);
    if (pn && qn) return Number(p) - Number(q);
    if (pn !== qn) return pn ? -1 : 1;
    return p < q ? -1 : 1;
  }
  return 0;
}

/** True if `version` has a pre-release part (0.3.1-beta.1): never for the stable channel. */
export function isPrerelease(version) {
  checkVersion(version);
  return SEMVER.exec(version)[4] !== undefined;
}

/** `<sha256>  <name>` lines of a SHA256SUMS file (sha256sum's text or binary mode) as a Map. */
export function parseSums(text) {
  const sums = new Map();
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const match = /^([0-9a-f]{64}) [ *](.+)$/.exec(line);
    if (!match) fail(`SHA256SUMS has a line that isn't '<sha256>  <file name>': ${line}`);
    sums.set(match[2], match[1]);
  }
  return sums;
}

/**
 * The release's SHA256SUMS (written by release.yml's `checksums` job only after every check of the
 * run passed, and attested) must list the manifest itself with exactly these bytes, and every
 * asset the manifest points at.
 */
export function checkSums(manifestBytes, sumsText, assets, manifestName = "latest.json") {
  const sums = parseSums(sumsText);
  const expected = sums.get(manifestName);
  if (!expected) fail(`SHA256SUMS doesn't list ${manifestName}`);
  const actual = createHash("sha256").update(manifestBytes).digest("hex");
  if (actual !== expected) fail(`${manifestName} isn't the file SHA256SUMS lists (sha256 ${actual}, listed ${expected})`);
  for (const asset of new Set(assets)) {
    if (!sums.has(asset)) fail(`SHA256SUMS doesn't list ${asset}, which the manifest points at`);
  }
}

function platformList(platforms) {
  const list = platforms ?? PLATFORM_KEYS;
  if (!Array.isArray(list) || list.length === 0) fail("No platforms asked for");
  for (const key of list) {
    if (!PLATFORM_KEYS.includes(key)) {
      fail(`Unknown platform key '${key}' (the manifest has only ${PLATFORM_KEYS.join(", ")})`);
    }
  }
  return PLATFORM_KEYS.filter((key) => list.includes(key));
}

/** Where assets are downloaded from: the release's download URL, or `baseUrl` (test channel). */
function urlPrefix({ tag, repo, baseUrl }) {
  if (baseUrl) {
    if (!/^https:\/\/[^\s/]+(\/[^\s]*)?$/.test(baseUrl)) fail(`--base-url must be an https URL, not '${baseUrl}'`);
    return baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
  }
  if (!tag) fail("Give --tag (release URLs) or --base-url (test channel)");
  if (typeof repo !== "string" || !REPO.test(repo)) fail(`'${repo}' isn't an owner/name repository`);
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/`;
}

/**
 * Which artifact each platform uses, from the file names in the artifact folder (artifacts and
 * their `.sig` files). Throws if a platform's artifact is missing or ambiguous, or has no `.sig`.
 */
export function findArtifacts(names, platforms) {
  const keys = platformList(platforms);
  const all = new Set(names);
  const found = {};
  for (const key of keys) {
    const { suffix, label } = PLATFORMS.find((p) => p.key === key);
    const candidates = new Set();
    for (const name of all) {
      if (name.endsWith(suffix)) candidates.add(name);
      if (name.endsWith(`${suffix}.sig`)) candidates.add(name.slice(0, -".sig".length));
    }
    if (candidates.size === 0) fail(`${key}: no ${label} among the artifacts`);
    if (candidates.size > 1) fail(`${key}: more than one ${label}: ${[...candidates].sort().join(", ")}`);
    const [asset] = candidates;
    if (!all.has(`${asset}.sig`)) fail(`${key}: the signature ${asset}.sig is missing`);
    found[key] = asset;
  }
  return found;
}

/** A `.sig` file's content as the manifest's `signature`, checked for the version binding. */
export function manifestSignature(content, asset, version) {
  const signature = typeof content === "string" ? content.trim() : "";
  if (!signature) fail(`The signature of ${asset} is empty`);
  const { trustedComment } = decodeSignature(signature, `The signature of ${asset}`);
  const signed = signedVersion(trustedComment);
  if (signed === undefined) {
    fail(
      `The signature of ${asset} isn't bound to a version (no 'version:' in its trusted comment). Sign it with the Tauri CLI from the lockfile (2.11.5+), which records the app version.`,
    );
  }
  if (signed.replace(/^v/, "") !== version) {
    fail(
      `The signature of ${asset} was made for version ${signed}, but the manifest announces ${version}; tauri-plugin-updater refuses that update (SignedVersionMismatch). \`tauri bundle\` binds its signatures to tauri.conf.json's version, so release.yml signs every updater file again with \`tauri signer sign --app-version ${version}\`.`,
    );
  }
  return signature;
}

function rfc3339(date) {
  return date.toISOString().replace(/\.\d{3}Z$/, "Z");
}

/**
 * The manifest object. `artifacts` maps each platform key to its asset name (`findArtifacts`),
 * `signatures` maps each asset name to its `.sig` file's content.
 */
export function buildManifest({ artifacts, signatures, version, tag, repo, baseUrl, platforms, notes = "", pubDate }) {
  checkVersion(version);
  const keys = platformList(platforms);
  if (!baseUrl && tag !== `v${version}`) fail(`The tag ${tag} doesn't match the version ${version} (expected v${version})`);
  const prefix = urlPrefix({ tag, repo, baseUrl });
  const date = pubDate ?? rfc3339(new Date());
  if (!RFC3339.test(date) || Number.isNaN(Date.parse(date))) fail(`'${date}' isn't an RFC 3339 date`);
  if (typeof notes !== "string") fail("The notes must be text");
  const entries = {};
  for (const key of keys) {
    const asset = artifacts[key];
    if (!asset) fail(`${key}: no artifact`);
    entries[key] = {
      signature: manifestSignature(signatures[asset], asset, version),
      url: `${prefix}${encodeURIComponent(asset)}`,
    };
  }
  return { version, notes, pub_date: date, platforms: entries };
}

const sameKeys = (object, keys) =>
  object !== null && typeof object === "object" && !Array.isArray(object) &&
  JSON.stringify(Object.keys(object).sort()) === JSON.stringify([...keys].sort());

/**
 * Checks a manifest against the contract. Returns the asset names it points at, per platform.
 * `assets` (optional) is the list of files the release (or test folder) really has.
 */
export function checkManifest(manifest, { version, tag, repo, baseUrl, platforms, assets } = {}) {
  if (!sameKeys(manifest, MANIFEST_KEYS)) {
    fail(`The manifest must have exactly the fields ${MANIFEST_KEYS.join(", ")}`);
  }
  checkVersion(manifest.version);
  if (version !== undefined && manifest.version !== version) {
    fail(`The manifest announces ${manifest.version}, expected ${version}`);
  }
  if (tag !== undefined && tag !== `v${manifest.version}`) {
    fail(`The manifest announces ${manifest.version}, which doesn't match the tag ${tag}`);
  }
  if (typeof manifest.notes !== "string") fail("The manifest's notes must be text");
  if (typeof manifest.pub_date !== "string" || !RFC3339.test(manifest.pub_date) || Number.isNaN(Date.parse(manifest.pub_date))) {
    fail(`The manifest's pub_date '${manifest.pub_date}' isn't an RFC 3339 date`);
  }
  const keys = platformList(platforms);
  if (!sameKeys(manifest.platforms, keys)) {
    fail(`The manifest's platforms must be exactly ${keys.join(", ")}; it has ${Object.keys(manifest.platforms ?? {}).join(", ") || "none"}`);
  }
  const prefix = urlPrefix({ tag, repo, baseUrl });
  const known = assets ? new Set(assets) : undefined;
  const found = {};
  for (const key of keys) {
    const entry = manifest.platforms[key];
    if (!sameKeys(entry, ["signature", "url"])) fail(`${key} must have exactly url and signature`);
    if (typeof entry.url !== "string" || !entry.url.startsWith(prefix)) fail(`${key}: the URL must start with ${prefix}`);
    const rest = entry.url.slice(prefix.length);
    if (!rest || rest.includes("/")) fail(`${key}: '${entry.url}' doesn't name one file under ${prefix}`);
    const asset = decodeURIComponent(rest);
    if (encodeURIComponent(asset) !== rest) fail(`${key}: '${entry.url}' isn't a plainly encoded file name`);
    const { suffix, label } = PLATFORMS.find((p) => p.key === key);
    if (!asset.endsWith(suffix)) fail(`${key}: ${asset} isn't a ${label}`);
    if (known && !known.has(asset)) fail(`${key}: ${asset} isn't among the published files`);
    manifestSignature(entry.signature, asset, manifest.version);
    found[key] = asset;
  }
  const mac = keys.filter((key) => key.startsWith("darwin-"));
  if (mac.length === 2) {
    const [a, b] = mac.map((key) => manifest.platforms[key]);
    if (a.url !== b.url || a.signature !== b.signature) {
      fail("darwin-aarch64-app and darwin-x86_64-app must point at the same universal .app.tar.gz");
    }
  }
  return found;
}

/** Checks every signature in the manifest against the file of that name in `dir`. */
export function verifyManifestFiles(manifest, dir, pubkey) {
  const done = new Set();
  for (const [key, entry] of Object.entries(manifest.platforms ?? {})) {
    const asset = decodeURIComponent(new URL(entry.url).pathname.split("/").pop());
    if (done.has(asset)) continue;
    const path = join(dir, asset);
    if (!existsSync(path)) fail(`${key}: ${asset} isn't in ${dir}, so its signature can't be checked`);
    verifySignature({ path }, entry.signature, pubkey, asset);
    done.add(asset);
  }
  return [...done];
}

// ---- command line -------------------------------------------------------------------------------

function parseArgs(argv) {
  const options = {};
  const positionals = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      positionals.push(arg);
      continue;
    }
    const eq = arg.indexOf("=");
    const name = arg.slice(2, eq === -1 ? undefined : eq);
    let value;
    if (eq !== -1) value = arg.slice(eq + 1);
    else if (i + 1 < argv.length && !argv[i + 1].startsWith("--")) value = argv[++i];
    else fail(`--${name} needs a value`);
    options[name] = value;
  }
  return { options, positionals };
}

function allow(options, names) {
  for (const name of Object.keys(options)) {
    if (!names.includes(name)) fail(`Unknown option --${name}`);
  }
}

function pubkeyFrom(options) {
  if (options.pubkey && options["pubkey-config"]) fail("Give --pubkey or --pubkey-config, not both");
  if (options["pubkey-config"]) return publicKeyFromConfig(options["pubkey-config"]);
  return options.pubkey;
}

const splitList = (value) => (value === undefined ? undefined : value.split(",").map((s) => s.trim()).filter(Boolean));

function readManifest(path) {
  if (!path) fail("Name the manifest file");
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    fail(`Can't read ${path}: ${err.message}`);
  }
}

export function main(argv, env = process.env) {
  const [first, ...rest] = argv;
  const command = first && !first.startsWith("--") ? first : "build";
  const { options, positionals } = parseArgs(command === first ? rest : argv);
  const repo = options.repo ?? env.GITHUB_REPOSITORY;

  if (command === "build") {
    allow(options, ["dir", "version", "tag", "repo", "base-url", "platforms", "notes", "notes-file", "pub-date", "pubkey", "pubkey-config", "out"]);
    if (positionals.length) fail(`Unexpected argument ${positionals[0]}`);
    if (!options.dir) fail("--dir is required (the folder with the updater artifacts and their .sig files)");
    if (!options.version) fail("--version is required");
    if (options.notes !== undefined && options["notes-file"]) fail("Give --notes or --notes-file, not both");
    const names = readdirSync(options.dir);
    const platforms = splitList(options.platforms);
    const artifacts = findArtifacts(names, platforms);
    const signatures = {};
    for (const asset of Object.values(artifacts)) {
      signatures[asset] = readFileSync(join(options.dir, `${asset}.sig`), "utf8");
    }
    const notes = options["notes-file"] ? readFileSync(options["notes-file"], "utf8").trim() : (options.notes ?? "");
    const manifest = buildManifest({
      artifacts,
      signatures,
      version: options.version,
      tag: options.tag,
      repo,
      baseUrl: options["base-url"],
      platforms,
      notes,
      pubDate: options["pub-date"],
    });
    const pubkey = pubkeyFrom(options);
    if (pubkey !== undefined) verifyManifestFiles(manifest, options.dir, pubkey);
    const json = `${JSON.stringify(manifest, null, 2)}\n`;
    if (options.out) writeFileSync(options.out, json);
    else process.stdout.write(json);
    const lines = Object.entries(manifest.platforms).map(([key, entry]) => `  ${key}: ${entry.url}`);
    process.stderr.write(`Manifest for ${manifest.version}${pubkey !== undefined ? " (signatures checked)" : ""}:\n${lines.join("\n")}\n`);
    return 0;
  }

  if (command === "check") {
    allow(options, ["version", "tag", "repo", "base-url", "platforms", "assets", "sums"]);
    const manifest = readManifest(positionals[0]);
    const assets = options.assets
      ? readFileSync(options.assets, "utf8").split(/\r?\n/).map((s) => s.trim()).filter(Boolean)
      : undefined;
    const found = checkManifest(manifest, {
      version: options.version,
      tag: options.tag,
      repo,
      baseUrl: options["base-url"],
      platforms: splitList(options.platforms),
      assets,
    });
    if (options.sums) {
      checkSums(readFileSync(positionals[0]), readFileSync(options.sums, "utf8"), Object.values(found));
    }
    process.stderr.write(`${positionals[0]}: ${manifest.version}, ${Object.keys(found).join(", ")}${options.sums ? ", in SHA256SUMS" : ""}: OK\n`);
    return 0;
  }

  if (command === "newer") {
    allow(options, []);
    if (positionals.length !== 2) fail("newer takes two versions");
    process.stdout.write(`${compareVersions(positionals[0], positionals[1]) > 0}\n`);
    return 0;
  }

  if (command === "enabled") {
    allow(options, []);
    if (positionals.length !== 1) fail("enabled takes the path of a tauri.conf.json");
    let config;
    try {
      config = JSON.parse(readFileSync(positionals[0], "utf8"));
    } catch (err) {
      fail(`Can't read ${positionals[0]}: ${err.message}`);
    }
    process.stdout.write(`${updaterEnabled(config)}\n`);
    return 0;
  }

  if (command === "verify") {
    allow(options, ["dir", "pubkey", "pubkey-config"]);
    const manifest = readManifest(positionals[0]);
    if (!options.dir) fail("--dir is required");
    const pubkey = pubkeyFrom(options);
    if (pubkey === undefined) fail("--pubkey or --pubkey-config is required");
    const files = verifyManifestFiles(manifest, options.dir, pubkey);
    process.stderr.write(`Signatures match: ${files.join(", ")}\n`);
    return 0;
  }

  fail(`Unknown command '${command}' (build, check, verify, newer or enabled)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    process.exitCode = main(process.argv.slice(2));
  } catch (err) {
    // Contract failures and file errors (ENOENT, …) get a one-line message; bugs keep their stack.
    if (!(err instanceof ManifestError) && !err.code) throw err;
    console.error(process.env.GITHUB_ACTIONS ? `::error::${err.message}` : `error: ${err.message}`);
    process.exitCode = 1;
  }
}
