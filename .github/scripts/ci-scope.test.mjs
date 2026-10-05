// Tests for ci-scope.mjs: `node --test .github/scripts/*.test.mjs`.
import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { ALL_OSES, isDoc, isTest, LINUX_ONLY, scopeOf } from "./ci-scope.mjs";

describe("what counts as a test", () => {
  it("takes test folders, test files and a crate's test modules", () => {
    for (const path of [
      "crates/pagelamp-app/tests/auto_sync_api.rs",
      "crates/pagelamp-llm/tests/fixtures/ollama/README.md",
      "crates/pagelamp-canvas/src/tests_sync.rs",
      "crates/pagelamp-ffi/src/tests.rs",
      "apps/desktop/src-tauri/tests/fixtures/ipc-calls.json",
      "apps/desktop/src/features/sources/useAutoSync.test.tsx",
      "apps/desktop/src/test/setup.ts",
      "apps/macos/Tests/PageLampTests/SyncTests.swift",
      "apps/macos/scripts/test/strings.test.mjs",
      "design/tokens/test/tokens.test.mjs",
      ".github/scripts/release-config.test.mjs",
    ]) {
      assert.equal(isTest(path), true, path);
    }
  });

  it("takes no source file for a test", () => {
    for (const path of [
      "crates/pagelamp-canvas/src/sync.rs",
      "crates/pagelamp-core/src/contests.rs",
      "apps/desktop/src/api/mock/index.ts",
      "apps/desktop/src/lib/latest.ts",
      "apps/macos/Sources/PageLamp/Attestation.swift",
      ".github/workflows/ci.yml",
      "README.md",
    ]) {
      assert.equal(isTest(path), false, path);
    }
  });
});

describe("what counts as documentation", () => {
  it("takes Markdown, the docs folder and the licence", () => {
    for (const path of [
      "README.md",
      "CHANGELOG.md",
      "PRIVACY.md",
      "apps/desktop/README.md",
      "docs/ARCHITECTURE.md",
      "docs/release-notes/v0.3.0-alpha.1.md",
      "docs/design/diagram.svg",
      "LICENSE",
    ]) {
      assert.equal(isDoc(path), true, path);
    }
  });

  it("takes no fixture, config or source for documentation", () => {
    for (const path of [
      "crates/pagelamp-llm/tests/fixtures/ollama/README.md",
      "apps/desktop/src/i18n/locales/en/common.json",
      "Cargo.toml",
      ".github/workflows/ci.yml",
      "design/tokens/tokens.json",
    ]) {
      assert.equal(isDoc(path), false, path);
    }
  });
});

describe("the scope of a pull request", () => {
  it("skips the builds for documentation alone", () => {
    assert.deepEqual(scopeOf(["README.md", "docs/ARCHITECTURE.md"]), {
      scope: "docs",
      oses: [],
      macosApp: false,
    });
  });

  it("runs tests on Linux alone when only tests changed, with or without documentation", () => {
    assert.deepEqual(scopeOf(["crates/pagelamp-app/tests/auto_sync_api.rs"]), {
      scope: "tests",
      oses: LINUX_ONLY,
      macosApp: false,
    });
    assert.deepEqual(
      scopeOf(["apps/desktop/src/features/sources/useAutoSync.test.tsx", "CHANGELOG.md"]),
      { scope: "tests", oses: LINUX_ONLY, macosApp: false },
    );
  });

  it("runs the macOS app job for its own tests", () => {
    assert.equal(scopeOf(["apps/macos/Tests/PageLampTests/SyncTests.swift"]).macosApp, true);
    assert.equal(scopeOf(["design/tokens/test/tokens.test.mjs"]).macosApp, true);
  });

  it("runs everything as soon as one file is neither", () => {
    for (const files of [
      ["README.md", "crates/pagelamp-core/src/views.rs"],
      ["crates/pagelamp-app/tests/auto_sync_api.rs", "Cargo.lock"],
      [".github/workflows/ci.yml"],
      ["apps/desktop/src/api/mock/index.ts"],
    ]) {
      assert.deepEqual(scopeOf(files), { scope: "full", oses: ALL_OSES, macosApp: true }, files[0]);
    }
  });

  it("runs everything when the files aren't known", () => {
    const full = { scope: "full", oses: ALL_OSES, macosApp: true };
    assert.deepEqual(scopeOf(null), full);
    assert.deepEqual(scopeOf([]), full);
  });
});
