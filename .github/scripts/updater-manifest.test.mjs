// Tests for updater-manifest.mjs: `node --test .github/scripts/*.test.mjs`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, generateKeyPairSync, randomBytes, sign } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import {
  buildManifest,
  checkManifest,
  checkSums,
  compareVersions,
  decodeSignature,
  findArtifacts,
  isPrerelease,
  isRealPublicKey,
  PLATFORM_KEYS,
  signedVersion,
  updaterEnabled,
  verifyManifestFiles,
  verifySignature,
} from "./updater-manifest.mjs";

const SCRIPT = fileURLToPath(new URL("./updater-manifest.mjs", import.meta.url));
const REPO = "Euswbnix/pagelamp";
const VERSION = "0.3.0-beta.1";
const TAG = `v${VERSION}`;
const KEYS = ["darwin-aarch64-app", "darwin-x86_64-app", "windows-x86_64-nsis", "linux-x86_64-appimage"];
const MAC = `PageLamp_${VERSION}_universal.app.tar.gz`;
const WIN = `PageLamp_${VERSION}_x64-setup.exe`;
const LINUX = `PageLamp_${VERSION}_amd64.AppImage`;

/** An updater key pair in Tauri's formats (base64 of minisign files), made with node:crypto. */
function makeKey() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const raw = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const keyId = randomBytes(8);
  const keyLine = Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64");
  const pubkey = Buffer.from(
    `untrusted comment: minisign public key ${keyId.toString("hex").toUpperCase()}\n${keyLine}\n`,
  ).toString("base64");
  /** What `tauri signer sign --app-version <version>` writes to <file>.sig (prehashed). */
  const signData = (data, file, version) => {
    const comment = `timestamp:1790000000\tfile:${file}${version === undefined ? "" : `\tversion:${version}`}`;
    const signature = sign(null, createHash("blake2b512").update(data).digest(), privateKey);
    const global = sign(null, Buffer.concat([signature, Buffer.from(comment)]), privateKey);
    const text = [
      "untrusted comment: signature from tauri secret key",
      Buffer.concat([Buffer.from("ED"), keyId, signature]).toString("base64"),
      `trusted comment: ${comment}`,
      global.toString("base64"),
      "",
    ].join("\n");
    return Buffer.from(text).toString("base64");
  };
  return { pubkey, signData };
}

const key = makeKey();
const otherKey = makeKey();
const content = { [MAC]: "mac archive", [WIN]: "windows installer", [LINUX]: "linux appimage" };
const signatures = Object.fromEntries(
  Object.entries(content).map(([name, data]) => [name, key.signData(Buffer.from(data), name, VERSION)]),
);
// Everything a release folder holds; the manifest must ignore the .dmg, .deb and their .sig files.
const names = [
  ...Object.keys(content),
  ...Object.keys(content).map((name) => `${name}.sig`),
  `PageLamp_${VERSION}_universal.dmg`,
  `PageLamp_${VERSION}_amd64.deb`,
  `PageLamp_${VERSION}_amd64.deb.sig`,
];
const PUB_DATE = "2026-10-18T12:00:00Z";

function release(overrides = {}) {
  return buildManifest({
    artifacts: findArtifacts(names),
    signatures,
    version: VERSION,
    tag: TAG,
    repo: REPO,
    notes: "Release notes",
    pubDate: PUB_DATE,
    ...overrides,
  });
}

const tempDirs = [];
function folder(files) {
  const dir = mkdtempSync(join(tmpdir(), "updater-manifest-"));
  tempDirs.push(dir);
  for (const [name, data] of Object.entries(files)) writeFileSync(join(dir, name), data);
  return dir;
}
after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

describe("build", () => {
  it("writes exactly the four platform keys and the four manifest fields", () => {
    const manifest = release();
    assert.deepEqual(Object.keys(manifest).sort(), ["notes", "platforms", "pub_date", "version"]);
    assert.deepEqual(Object.keys(manifest.platforms), KEYS);
    assert.deepEqual(PLATFORM_KEYS, KEYS);
    for (const generic of ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "linux-x86_64"]) {
      assert.equal(manifest.platforms[generic], undefined, `no generic key ${generic}`);
    }
    assert.ok(!Object.keys(manifest.platforms).some((k) => /deb|rpm|msi/.test(k)), "no deb, rpm or msi key");
    for (const entry of Object.values(manifest.platforms)) assert.deepEqual(Object.keys(entry).sort(), ["signature", "url"]);
  });

  it("announces the Cargo version with its pre-release part, the notes and the date", () => {
    const manifest = release();
    assert.equal(manifest.version, "0.3.0-beta.1");
    assert.equal(manifest.notes, "Release notes");
    assert.equal(manifest.pub_date, PUB_DATE);
    assert.match(release({ pubDate: undefined }).pub_date, /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  });

  it("uses plain releases/download URLs, one universal archive for both Mac keys", () => {
    const { platforms } = release();
    const base = `https://github.com/${REPO}/releases/download/${TAG}/`;
    assert.equal(platforms["darwin-aarch64-app"].url, `${base}${MAC}`);
    assert.equal(platforms["darwin-x86_64-app"].url, `${base}${MAC}`);
    assert.equal(platforms["windows-x86_64-nsis"].url, `${base}${WIN}`);
    assert.equal(platforms["linux-x86_64-appimage"].url, `${base}${LINUX}`);
    assert.equal(platforms["darwin-aarch64-app"].signature, platforms["darwin-x86_64-app"].signature);
    for (const { url } of Object.values(platforms)) {
      assert.ok(!url.includes("api.github.com") && !url.includes("/assets/"), "no API asset URL");
    }
  });

  it("uses the .sig file's content as the signature", () => {
    const withNewline = { ...signatures, [WIN]: `${signatures[WIN]}\n` };
    const { platforms } = release({ signatures: withNewline });
    assert.equal(platforms["windows-x86_64-nsis"].signature, signatures[WIN]);
    assert.equal(platforms["linux-x86_64-appimage"].signature, signatures[LINUX]);
  });

  it("serves a test channel from --base-url with only the platforms asked for", () => {
    const platforms = ["darwin-aarch64-app", "darwin-x86_64-app", "windows-x86_64-nsis"];
    const onlyTested = names.filter((n) => !n.includes("AppImage"));
    const manifest = buildManifest({
      artifacts: findArtifacts(onlyTested, platforms),
      signatures,
      version: VERSION,
      baseUrl: "https://raw.githubusercontent.com/Euswbnix/pagelamp/gh-pages/updates/test/abc1234",
      platforms,
      pubDate: PUB_DATE,
    });
    assert.deepEqual(Object.keys(manifest.platforms), platforms);
    assert.equal(
      manifest.platforms["windows-x86_64-nsis"].url,
      `https://raw.githubusercontent.com/Euswbnix/pagelamp/gh-pages/updates/test/abc1234/${WIN}`,
    );
  });

  it("fails when a platform is missing", () => {
    assert.throws(() => findArtifacts(names.filter((n) => !n.includes("AppImage"))), /linux-x86_64-appimage: no Linux AppImage/);
    assert.throws(() => findArtifacts(names.filter((n) => !n.includes("setup.exe"))), /windows-x86_64-nsis: no Windows NSIS installer/);
    assert.throws(() => findArtifacts(names.filter((n) => !n.includes(".app.tar.gz"))), /darwin-aarch64-app: no macOS updater archive/);
  });

  it("fails when a signature is missing or empty", () => {
    assert.throws(() => findArtifacts(names.filter((n) => n !== `${WIN}.sig`)), new RegExp(`the signature ${WIN}\\.sig is missing`));
    // Only the .sig without its artifact still names the artifact (the draft's asset list).
    assert.equal(findArtifacts(names.filter((n) => n !== LINUX))["linux-x86_64-appimage"], LINUX);
    assert.throws(() => release({ signatures: { ...signatures, [LINUX]: "  \n" } }), /signature of .*AppImage is empty/);
    assert.throws(() => release({ signatures: { ...signatures, [MAC]: "not a signature" } }), /isn't base64/);
  });

  it("fails when a platform has more than one candidate", () => {
    const two = [...names, "Other_universal.app.tar.gz", "Other_universal.app.tar.gz.sig"];
    assert.throws(() => findArtifacts(two), /more than one macOS updater archive/);
  });

  it("fails when a signature was made for another version or none", () => {
    const numeric = { ...signatures, [WIN]: key.signData(Buffer.from(content[WIN]), WIN, "0.3.0") };
    assert.throws(() => release({ signatures: numeric }), /made for version 0\.3\.0, but the manifest announces 0\.3\.0-beta\.1/);
    const unbound = { ...signatures, [MAC]: key.signData(Buffer.from(content[MAC]), MAC, undefined) };
    assert.throws(() => release({ signatures: unbound }), /isn't bound to a version/);
  });

  it("fails on a bad version, a tag that doesn't match, or unknown platforms", () => {
    assert.throws(() => release({ version: "v0.3.0-beta.1", tag: "vv0.3.0-beta.1" }), /isn't a semantic version/);
    assert.throws(() => release({ tag: "v0.3.0" }), /doesn't match the version/);
    assert.throws(() => release({ repo: "not a repo" }), /isn't an owner\/name repository/);
    assert.throws(() => findArtifacts(names, ["linux-x86_64"]), /Unknown platform key 'linux-x86_64'/);
    assert.throws(() => findArtifacts(names, []), /No platforms/);
  });
});

describe("check", () => {
  const assets = names;
  it("accepts a manifest that follows the contract", () => {
    assert.deepEqual(checkManifest(release(), { tag: TAG, repo: REPO, assets }), {
      "darwin-aarch64-app": MAC,
      "darwin-x86_64-app": MAC,
      "windows-x86_64-nsis": WIN,
      "linux-x86_64-appimage": LINUX,
    });
  });

  it("rejects extra, missing or generic keys", () => {
    assert.throws(() => checkManifest({ ...release(), extra: 1 }, { tag: TAG, repo: REPO }), /exactly the fields/);
    const generic = release();
    generic.platforms["linux-x86_64"] = generic.platforms["linux-x86_64-appimage"];
    assert.throws(() => checkManifest(generic, { tag: TAG, repo: REPO }), /platforms must be exactly/);
    const missing = release();
    delete missing.platforms["windows-x86_64-nsis"];
    assert.throws(() => checkManifest(missing, { tag: TAG, repo: REPO }), /platforms must be exactly/);
  });

  it("rejects URLs that aren't this release's plain download URLs", () => {
    const api = release();
    api.platforms["windows-x86_64-nsis"].url = `https://api.github.com/repos/${REPO}/releases/assets/1`;
    assert.throws(() => checkManifest(api, { tag: TAG, repo: REPO }), /must start with/);
    const other = release({ tag: "v0.3.0-beta.1" });
    assert.throws(() => checkManifest(other, { tag: "v0.3.0-beta.2", repo: REPO }), /doesn't match the tag/);
    assert.throws(() => checkManifest(release(), { tag: TAG, repo: REPO, assets: [MAC, WIN] }), /isn't among the published files/);
  });

  it("rejects Mac keys that point at different archives", () => {
    const split = release();
    split.platforms["darwin-x86_64-app"] = { ...split.platforms["darwin-x86_64-app"], url: `${split.platforms["darwin-x86_64-app"].url.replace("universal", "x64")}` };
    assert.throws(() => checkManifest(split, { tag: TAG, repo: REPO }), /same universal \.app\.tar\.gz/);
  });
});

describe("signatures", () => {
  // Known-answer vectors from the minisign-verify crate's tests (MIT), which tauri-plugin-updater
  // uses to check updates: a real minisign key and signatures over the 4 bytes "test".
  const vectorKey = Buffer.from(
    "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n",
  ).toString("base64");
  const vector = (line, comment, global) =>
    Buffer.from(`untrusted comment: signature from minisign secret key\n${line}\ntrusted comment: ${comment}\n${global}`).toString("base64");
  const prehashed = vector(
    "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=",
    "timestamp:1556193335\tfile:test",
    "y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==",
  );
  const legacy = vector(
    "RWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=",
    "timestamp:1555779966\tfile:test",
    "QtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==",
  );

  it("verifies real minisign signatures (prehashed and legacy)", () => {
    verifySignature(Buffer.from("test"), prehashed, vectorKey);
    verifySignature(Buffer.from("test"), legacy, vectorKey);
    assert.throws(() => verifySignature(Buffer.from("Test"), prehashed, vectorKey), /doesn't match its content/);
    assert.throws(() => verifySignature(Buffer.from("Test"), legacy, vectorKey), /doesn't match its content/);
    assert.equal(decodeSignature(prehashed).prehashed, true);
    assert.equal(signedVersion(decodeSignature(prehashed).trustedComment), undefined);
  });

  it("verifies Tauri-style signatures and rejects tampering and other keys", () => {
    const data = Buffer.from("installer bytes");
    const signature = key.signData(data, WIN, VERSION);
    verifySignature(data, signature, key.pubkey);
    assert.equal(signedVersion(decodeSignature(signature).trustedComment), VERSION);
    assert.throws(() => verifySignature(Buffer.from("installer bytez"), signature, key.pubkey), /doesn't match its content/);
    assert.throws(() => verifySignature(data, signature, otherKey.pubkey), /another key/);
    // A version edited into the trusted comment breaks the global signature.
    const text = Buffer.from(signature, "base64").toString("utf8").replace(`version:${VERSION}`, "version:9.9.9");
    assert.throws(() => verifySignature(data, Buffer.from(text).toString("base64"), key.pubkey), /changed after signing/);
  });

  it("checks every file of a manifest in a folder", () => {
    const dir = folder(content);
    assert.deepEqual(verifyManifestFiles(release(), dir, key.pubkey).sort(), [LINUX, MAC, WIN].sort());
    writeFileSync(join(dir, WIN), "swapped installer");
    assert.throws(() => verifyManifestFiles(release(), dir, key.pubkey), /PageLamp_0\.3\.0-beta\.1_x64-setup\.exe doesn't match/);
  });

  it("tells a real public key from a placeholder", () => {
    assert.equal(isRealPublicKey(key.pubkey), true);
    assert.equal(isRealPublicKey(vectorKey), true);
    for (const placeholder of [undefined, "", "REPLACE_WITH_THE_OWNER_PUBKEY", "dW50cnVzdGVk", "~/.tauri/pagelamp.key.pub"]) {
      assert.equal(isRealPublicKey(placeholder), false, `${placeholder} is not a key`);
    }
  });
});

describe("SHA256SUMS", () => {
  const sha = (data) => createHash("sha256").update(data).digest("hex");
  const manifestBytes = Buffer.from(`${JSON.stringify(release(), null, 2)}\n`);
  const sums = (entries) => `${entries.map(([name, data]) => `${sha(data)}  ${name}`).join("\n")}\n`;
  const all = [["latest.json", manifestBytes], ...Object.entries(content), [`PageLamp_${VERSION}_universal.dmg`, "dmg"]];

  it("accepts a manifest listed with its exact bytes, whose assets are all listed", () => {
    assert.doesNotThrow(() => checkSums(manifestBytes, sums(all), [MAC, MAC, WIN, LINUX]));
    // sha256sum's binary-mode marker is fine too.
    assert.doesNotThrow(() => checkSums(manifestBytes, sums(all).replaceAll("  ", " *"), [MAC, WIN, LINUX]));
  });

  it("rejects a manifest that isn't listed, was changed, or points at unlisted assets", () => {
    assert.throws(() => checkSums(manifestBytes, sums(all.slice(1)), [MAC]), /SHA256SUMS doesn't list latest\.json/);
    assert.throws(() => checkSums(Buffer.from("{}"), sums(all), [MAC]), /latest\.json isn't the file SHA256SUMS lists/);
    assert.throws(() => checkSums(manifestBytes, sums(all.filter(([n]) => n !== WIN)), [MAC, WIN, LINUX]), /doesn't list PageLamp_0\.3\.0-beta\.1_x64-setup\.exe/);
    assert.throws(() => checkSums(manifestBytes, "not a sums file\n", [MAC]), /isn't '<sha256>  <file name>'/);
  });
});

describe("versions and channels", () => {
  it("orders versions by SemVer precedence", () => {
    assert.ok(compareVersions("0.3.0-beta.2", "0.3.0") < 0, "a pre-release is older than its release");
    assert.ok(compareVersions("0.4.0-alpha.1", "0.3.1") > 0);
    assert.ok(compareVersions("0.3.1", "0.3.0") > 0);
    assert.ok(compareVersions("0.3.0-alpha.0.1", "0.3.0-alpha.1") < 0, "the test channel's versions come before alpha.1");
    assert.ok(compareVersions("0.3.0-alpha.0.2", "0.3.0-alpha.0.1") > 0);
    assert.ok(compareVersions("0.3.0-beta.10", "0.3.0-beta.9") > 0, "numeric identifiers compare as numbers");
    assert.ok(compareVersions("0.3.0-beta", "0.3.0-beta.1") < 0, "fewer identifiers come first");
    assert.ok(compareVersions("0.3.0-alpha.1", "0.3.0-1") > 0, "text identifiers come after numeric ones");
    assert.ok(compareVersions("0.3.0-alpha.1", "0.3.0-beta.1") < 0);
    assert.equal(compareVersions("0.3.0+build.1", "0.3.0+build.2"), 0, "build metadata doesn't count");
    assert.equal(compareVersions("0.3.0-rc.1", "0.3.0-rc.1"), 0);
    assert.throws(() => compareVersions("v0.3.0", "0.3.0"), /isn't a semantic version/);
  });

  it("tells pre-release versions apart", () => {
    assert.equal(isPrerelease("0.3.1-beta.1"), true);
    assert.equal(isPrerelease("0.3.1"), false);
    assert.equal(isPrerelease("0.3.1+build.5"), false);
  });

  it("knows when a Tauri config builds updater artifacts", () => {
    const on = { bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: key.pubkey } } };
    assert.equal(updaterEnabled(on), true);
    assert.equal(updaterEnabled({ ...on, bundle: {} }), false);
    assert.equal(updaterEnabled({ ...on, bundle: { createUpdaterArtifacts: "v1Compatible" } }), false);
    assert.equal(updaterEnabled({ ...on, plugins: { updater: { pubkey: "PLACEHOLDER: the owner's key" } } }), false);
    assert.equal(updaterEnabled(undefined), false);
  });
});

describe("command line", () => {
  const run = (...args) =>
    spawnSync(process.execPath, [SCRIPT, ...args], { encoding: "utf8", env: { ...process.env, GITHUB_ACTIONS: "" } });
  const files = {
    ...content,
    ...Object.fromEntries(Object.entries(signatures).map(([name, sig]) => [`${name}.sig`, `${sig}\n`])),
  };

  it("builds, checks and verifies latest.json", () => {
    const dir = folder(files);
    const notes = join(dir, "notes.md");
    writeFileSync(notes, "## What's new\n\n- Updates\n");
    const out = join(dir, "latest.json");
    const built = run("build", "--dir", dir, "--version", VERSION, "--tag", TAG, "--repo", REPO, "--notes-file", notes, "--pubkey", key.pubkey, "--out", out);
    assert.equal(built.status, 0, built.stderr);
    const manifest = JSON.parse(readFileSync(out, "utf8"));
    assert.deepEqual(Object.keys(manifest.platforms), KEYS);
    assert.equal(manifest.notes, "## What's new\n\n- Updates");
    const list = join(dir, "assets.txt");
    writeFileSync(list, `${names.join("\n")}\nlatest.json\n`);
    const checked = run("check", out, "--tag", TAG, "--repo", REPO, "--assets", list);
    assert.equal(checked.status, 0, checked.stderr);
    const verified = run("verify", out, "--dir", dir, "--pubkey", key.pubkey);
    assert.equal(verified.status, 0, verified.stderr);
    const wrongKey = run("verify", out, "--dir", dir, "--pubkey", otherKey.pubkey);
    assert.equal(wrongKey.status, 1);
    assert.match(wrongKey.stderr, /another key/);
  });

  it("reads the public key from a Tauri config", () => {
    const dir = folder({ ...files, "tauri.conf.json": JSON.stringify({ plugins: { updater: { pubkey: key.pubkey } } }) });
    const built = run("build", "--dir", dir, "--version", VERSION, "--tag", TAG, "--repo", REPO, "--pubkey-config", join(dir, "tauri.conf.json"));
    assert.equal(built.status, 0, built.stderr);
    assert.deepEqual(Object.keys(JSON.parse(built.stdout).platforms), KEYS);
  });

  it("exits 1 with a message when a platform is missing", () => {
    const { [LINUX]: _gone, [`${LINUX}.sig`]: _alsoGone, ...partial } = files;
    const dir = folder(partial);
    const result = run("build", "--dir", dir, "--version", VERSION, "--tag", TAG, "--repo", REPO);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /linux-x86_64-appimage: no Linux AppImage/);
    assert.equal(result.stdout, "");
  });

  it("checks latest.json against the release's SHA256SUMS", () => {
    const dir = folder(files);
    const out = join(dir, "latest.json");
    assert.equal(run("build", "--dir", dir, "--version", VERSION, "--tag", TAG, "--repo", REPO, "--out", out).status, 0);
    const sha = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
    const listed = ["latest.json", MAC, WIN, LINUX].map((name) => `${sha(join(dir, name))}  ${name}`);
    const sums = join(dir, "SHA256SUMS");
    writeFileSync(sums, `${listed.join("\n")}\n`);
    const ok = run("check", out, "--tag", TAG, "--repo", REPO, "--sums", sums);
    assert.equal(ok.status, 0, ok.stderr);
    assert.match(ok.stderr, /in SHA256SUMS: OK/);
    writeFileSync(sums, `${listed.slice(1).join("\n")}\n`);
    const unlisted = run("check", out, "--tag", TAG, "--repo", REPO, "--sums", sums);
    assert.equal(unlisted.status, 1);
    assert.match(unlisted.stderr, /SHA256SUMS doesn't list latest\.json/);
  });

  it("compares versions and reads the updater switch", () => {
    assert.equal(run("newer", "0.4.0-alpha.1", "0.3.1").stdout, "true\n");
    assert.equal(run("newer", "0.3.0-beta.2", "0.3.0").stdout, "false\n");
    assert.equal(run("newer", "0.3.0", "0.3.0").stdout, "false\n");
    assert.equal(run("newer", "0.3.0", "latest").status, 1);
    const dir = folder({
      "on.json": JSON.stringify({ bundle: { createUpdaterArtifacts: true }, plugins: { updater: { pubkey: key.pubkey } } }),
      "off.json": JSON.stringify({ bundle: {}, plugins: { updater: { pubkey: "PLACEHOLDER" } } }),
    });
    assert.equal(run("enabled", join(dir, "on.json")).stdout, "true\n");
    assert.equal(run("enabled", join(dir, "off.json")).stdout, "false\n");
    assert.equal(run("enabled", join(dir, "missing.json")).status, 1);
  });

  it("exits 1 on unknown options and commands", () => {
    assert.equal(run("build", "--dir", ".", "--version", VERSION, "--frobnicate", "x").status, 1);
    assert.equal(run("publish").status, 1);
  });
});
