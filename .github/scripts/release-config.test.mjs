// Tests for release-config.mjs: `node --test .github/scripts/*.test.mjs`.
import assert from "node:assert/strict";
import { generateKeyPairSync, randomBytes } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, describe, it } from "node:test";

import { cargoWorkspaceVersion, pinningProblems, run, scanYaml, secretProblems, tauriFindings } from "./release-config.mjs";

function realPubkey() {
  const { publicKey } = generateKeyPairSync("ed25519");
  const raw = Buffer.from(publicKey.export({ format: "jwk" }).x, "base64url");
  const line = Buffer.concat([Buffer.from("Ed"), randomBytes(8), raw]).toString("base64");
  return Buffer.from(`untrusted comment: minisign public key 0000\n${line}\n`).toString("base64");
}
const PUBKEY = realPubkey();
const SHA = "3d3c42e5aac5ba805825da76410c181273ba90b1";

const workflow = String.raw`name: Test
on:
  push:
env:
  CARGO_TERM_COLOR: always
jobs:
  build:
    runs-on: ubuntu-22.04
    environment: release
    steps:
      - uses: actions/checkout@${SHA} # v7.0.1
      - name: Check
        env:
          HAS_KEY: ${"${{ secrets.KEY != '' }}"}
          GH_TOKEN: ${"${{ secrets.GITHUB_TOKEN }}"}
        run: |
          echo "key: value lines in a script are not keys"
          gh release view
      - uses: some/action@${SHA}
        with:
          token: ${"${{ secrets.TOKEN }}"}
`;

describe("scanYaml", () => {
  it("tracks where each key sits, including list items and block scalars", () => {
    const entries = scanYaml(workflow);
    const env = entries.find((e) => e.key === "HAS_KEY");
    assert.deepEqual(env.path, ["jobs", "build", "steps", "[]", "env", "HAS_KEY"]);
    const token = entries.find((e) => e.key === "token");
    assert.deepEqual(token.path, ["jobs", "build", "steps", "[]", "with", "token"]);
    const script = entries.find((e) => e.text?.startsWith("echo"));
    assert.deepEqual(script.path, ["jobs", "build", "steps", "[]", "run"]);
    assert.equal(entries.find((e) => e.key === "value lines in a script are not keys"), undefined);
  });

  it("handles sequences at the parent's indentation", () => {
    const entries = scanYaml("jobs:\n  a:\n    steps:\n    - name: x\n      env:\n        K: v\n    env:\n      J: w\n");
    assert.deepEqual(entries.find((e) => e.key === "K").path, ["jobs", "a", "steps", "[]", "env", "K"]);
    assert.deepEqual(entries.find((e) => e.key === "J").path, ["jobs", "a", "env", "J"]);
  });
});

describe("secrets", () => {
  it("allows secrets in a step's env and with", () => {
    assert.deepEqual(secretProblems("w.yml", workflow), []);
  });

  it("flags job-level and workflow-level env", () => {
    const job = workflow.replace("    environment: release\n", "    environment: release\n    env:\n      KEY: ${{ secrets.KEY }}\n");
    assert.match(secretProblems("w.yml", job).join("\n"), /w\.yml:\d+: a secret is used in jobs\.build\.env\.KEY/);
    const top = workflow.replace("  CARGO_TERM_COLOR: always\n", "  CARGO_TERM_COLOR: always\n  KEY: ${{ secrets.KEY }}\n");
    assert.match(secretProblems("w.yml", top).join("\n"), /a secret is used in env\.KEY/);
  });

  it("flags secrets in run scripts, conditions and reusable-workflow calls", () => {
    const script = workflow.replace("gh release view", "curl -H \"x: ${{ secrets.KEY }}\" https://example.com");
    assert.match(secretProblems("w.yml", script).join("\n"), /jobs\.build\.steps\[\]\.run/);
    const condition = workflow.replace("      - name: Check\n", "      - name: Check\n        if: secrets.KEY != ''\n");
    assert.match(secretProblems("w.yml", condition).join("\n"), /jobs\.build\.steps\[\]\.if/);
    const inherit = "jobs:\n  call:\n    uses: ./.github/workflows/other.yml\n    secrets: inherit\n";
    assert.match(secretProblems("w.yml", inherit).join("\n"), /passes secrets to a reusable workflow/);
  });
});

describe("pinning", () => {
  it("accepts full SHAs and local actions only", () => {
    assert.deepEqual(pinningProblems("w.yml", workflow), []);
    assert.deepEqual(pinningProblems("w.yml", "jobs:\n  a:\n    steps:\n      - uses: ./local-action\n"), []);
    const tag = pinningProblems("w.yml", "jobs:\n  a:\n    steps:\n      - uses: actions/checkout@v7\n      - uses: 'dtolnay/rust-toolchain@stable'\n");
    assert.equal(tag.length, 2);
    assert.match(tag[0], /w\.yml:4: 'actions\/checkout@v7' isn't pinned/);
    assert.match(pinningProblems("w.yml", "jobs:\n  a:\n    uses: org/repo/.github/workflows/x.yml@main\n")[0], /isn't pinned/);
  });
});

describe("Tauri config", () => {
  const today = { version: "0.1.0", bundle: { targets: "all", macOS: { signingIdentity: "-" } } };
  const updater = (extra = {}) => ({
    version: "0.3.0-alpha.1",
    bundle: { targets: ["app", "dmg", "nsis", "appimage", "deb", "rpm"], createUpdaterArtifacts: true },
    plugins: { updater: { pubkey: PUBKEY } },
    ...extra,
  });
  const check = (base, { strict = false, cargoVersion = "0.3.0-alpha.1", more = {} } = {}) =>
    tauriFindings({ configs: { "tauri.conf.json": base, ...more }, cargoVersion, strict });

  it("keeps today's config releasable: MSI only warns while the updater is off", () => {
    for (const strict of [false, true]) {
      const result = check(today, { strict, cargoVersion: "0.1.0" });
      assert.deepEqual(result.errors, []);
      assert.equal(result.updater, false);
      assert.match(result.warnings.join("\n"), /bundle\.targets is "all"/);
    }
  });

  it("turns the updater on only with a real public key", () => {
    const result = check(updater(), { strict: true });
    assert.deepEqual(result, { errors: [], warnings: [], updater: true });
    const placeholder = updater({ plugins: { updater: { pubkey: "REPLACE_WITH_OWNER_KEY" } } });
    const ci = check(placeholder);
    assert.equal(ci.updater, false);
    assert.deepEqual(ci.errors, []);
    assert.match(ci.warnings.join("\n"), /isn't a real updater public key/);
    assert.match(check(placeholder, { strict: true }).errors.join("\n"), /isn't a real updater public key/);
  });

  it("requires NSIS only once the updater is on (strict)", () => {
    const msi = updater({ bundle: { targets: ["app", "dmg", "msi", "nsis"], createUpdaterArtifacts: true } });
    assert.match(check(msi).warnings.join("\n"), /has "msi"/);
    assert.match(check(msi, { strict: true }).errors.join("\n"), /has "msi"/);
    const all = updater({ bundle: { createUpdaterArtifacts: true } });
    assert.match(check(all, { strict: true }).errors.join("\n"), /not set \(all\)/);
    const windowsOverride = { "tauri.windows.conf.json": { bundle: { targets: ["nsis"] } } };
    const allButNsisOnWindows = updater({ bundle: { targets: "all", createUpdaterArtifacts: true } });
    assert.deepEqual(check(allButNsisOnWindows, { strict: true, more: windowsOverride }).errors, []);
  });

  it("accepts the Cargo version, its numeric part or no version in tauri.conf.json", () => {
    // release.yml signs updater files with the Cargo version, whatever tauri.conf.json says.
    assert.deepEqual(check(updater({ version: "0.3.0" }), { strict: true }).errors, []);
    const { version: _omitted, ...inherits } = updater();
    assert.deepEqual(check(inherits, { strict: true }).errors, []);
    const other = updater({ version: "0.2.0" });
    assert.match(check(other, { strict: true }).errors.join("\n"), /version 0\.2\.0 is neither the Cargo workspace version 0\.3\.0-alpha\.1 nor its numeric part/);
    assert.match(check(other).warnings.join("\n"), /is neither the Cargo workspace version/);
  });

  it("always rejects entitlements, v1Compatible and overlays that switch the updater", () => {
    const entitled = { ...today, bundle: { ...today.bundle, macOS: { entitlements: "Entitlements.plist" } } };
    assert.match(check(entitled).errors.join("\n"), /bundle\.macOS\.entitlements is set/);
    const overlayEntitled = check(today, { more: { "tauri.macos.conf.json": { bundle: { macOS: { entitlements: "x.plist" } } } } });
    assert.match(overlayEntitled.errors.join("\n"), /tauri\.macos\.conf\.json: bundle\.macOS\.entitlements/);
    const v1 = updater({ bundle: { targets: ["nsis"], createUpdaterArtifacts: "v1Compatible" } });
    assert.match(check(v1).errors.join("\n"), /v1Compatible/);
    const overlay = check(today, { more: { "tauri.rehearsal.conf.json": { bundle: { createUpdaterArtifacts: true } } } });
    assert.match(overlay.errors.join("\n"), /tauri\.rehearsal\.conf\.json: sets bundle\.createUpdaterArtifacts/);
  });

  it("reads the Cargo workspace version", () => {
    const toml = '[workspace]\nmembers = []\n\n[workspace.package]\nversion = "0.3.0-alpha.1"\nedition = "2024"\n\n[profile.release]\nversion = "no"\n';
    assert.equal(cargoWorkspaceVersion(toml), "0.3.0-alpha.1");
    assert.equal(cargoWorkspaceVersion('[package]\nversion = "1.0.0"\n'), undefined);
  });
});

describe("run", () => {
  const roots = [];
  after(() => {
    for (const root of roots) rmSync(root, { recursive: true, force: true });
  });
  function repo({ config, workflows = { "ci.yml": workflow }, overlay = false }) {
    const root = mkdtempSync(join(tmpdir(), "release-config-"));
    roots.push(root);
    mkdirSync(join(root, ".github/workflows"), { recursive: true });
    mkdirSync(join(root, "apps/desktop/src-tauri"), { recursive: true });
    for (const [name, text] of Object.entries(workflows)) writeFileSync(join(root, ".github/workflows", name), text);
    writeFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "0.3.0-alpha.1"\n');
    writeFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), JSON.stringify(config));
    if (overlay) writeFileSync(join(root, "apps/desktop/src-tauri/tauri.rehearsal.conf.json"), "{}");
    return root;
  }

  it("writes the updater switch, the version and the overlay to GITHUB_OUTPUT", () => {
    const root = repo({
      config: { bundle: { targets: ["app", "dmg", "nsis", "appimage", "deb", "rpm"], createUpdaterArtifacts: true }, plugins: { updater: { pubkey: PUBKEY } } },
      overlay: true,
    });
    const output = join(root, "out.txt");
    const lines = [];
    const result = run({ root, strict: true, env: { GITHUB_OUTPUT: output }, log: (l) => lines.push(l) });
    assert.deepEqual(result.errors, []);
    assert.equal(readFileSync(output, "utf8"), "updater=true\nversion=0.3.0-alpha.1\ntest_overlay=true\n");
    assert.match(lines.at(-1), /Updater artifacts: on/);
  });

  it("fails on an unpinned action or a job-level secret in any workflow", () => {
    const bad = "jobs:\n  a:\n    env:\n      T: ${{ secrets.T }}\n    steps:\n      - uses: actions/checkout@v7\n";
    const root = repo({ config: { version: "0.3.0-alpha.1", bundle: { targets: "all" } }, workflows: { "ci.yml": workflow, "bad.yaml": bad } });
    const lines = [];
    const result = run({ root, env: { GITHUB_ACTIONS: "true" }, log: (l) => lines.push(l) });
    assert.equal(result.errors.length, 2);
    assert.ok(lines.some((l) => l.startsWith("::error::.github/workflows/bad.yaml:4:")));
    assert.ok(lines.some((l) => l.startsWith("::warning::")), "D4 is a warning outside a release");
  });
});
