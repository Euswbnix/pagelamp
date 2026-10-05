// Tests for third-party-notices.mjs: `node --test .github/scripts/*.test.mjs`. They use made-up
// packages: what cargo and pnpm answer is their own business.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, describe, it } from "node:test";

import {
  fillMissing,
  fingerprint,
  isLicenceFile,
  licenceFiles,
  licenceIds,
  npmEntries,
  plainOrder,
  reachable,
  render,
  standardTexts,
  tidy,
  versionOrder,
  withoutHolders,
} from "./third-party-notices.mjs";

const MIT_BODY = "Permission is hereby granted, free of charge, to any person obtaining a copy\nof this software.";
const mit = (holder) => `MIT License\n\nCopyright (c) ${holder}\n\n${MIT_BODY}`;
const APACHE = "Apache License\nVersion 2.0, January 2004\n\nTERMS AND CONDITIONS";

const pkg = (name, extra = {}) => ({
  kind: "crate",
  name,
  version: "1.0.0",
  license: "MIT",
  repository: `https://example.org/${name}`,
  authors: [],
  files: [],
  ...extra,
});

describe("licence files", () => {
  const dir = mkdtempSync(join(tmpdir(), "notices-"));
  after(() => rmSync(dir, { recursive: true, force: true }));

  it("knows a licence file by its name", () => {
    for (const name of ["LICENSE", "LICENSE-MIT", "LICENSE.md", "license.txt", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "LICENSE_APACHE"]) {
      assert.equal(isLicenceFile(name), true, name);
    }
    for (const name of ["README.md", "Cargo.toml", "license.rs", "licenses.json", "CHANGELOG.md", "notices"]) {
      assert.equal(isLicenceFile(name), false, name);
    }
  });

  it("reads a package's files in name order, tidied, and skips empty ones and folders", () => {
    writeFileSync(join(dir, "LICENSE-MIT"), "﻿line one  \r\nline two\r\n\r\n");
    writeFileSync(join(dir, "LICENSE-APACHE"), "apache\n");
    writeFileSync(join(dir, "NOTICE"), "   \n");
    writeFileSync(join(dir, "README.md"), "not a licence");
    mkdirSync(join(dir, "LICENSES"));
    assert.deepEqual(licenceFiles(dir), [
      { file: "LICENSE-APACHE", text: "apache" },
      { file: "LICENSE-MIT", text: "line one\nline two" },
    ]);
    // A file the manifest names is read even when its name says nothing.
    writeFileSync(join(dir, "terms.txt"), "terms");
    assert.deepEqual(
      licenceFiles(dir, "terms.txt").map((entry) => entry.file),
      ["LICENSE-APACHE", "LICENSE-MIT", "terms.txt"],
    );
    assert.deepEqual(licenceFiles(join(dir, "nowhere")), []);
  });

  it("takes two texts for the same when only their line breaks and spacing differ", () => {
    assert.equal(fingerprint("The  Software\nis provided"), fingerprint("the software is\r\nprovided\n"));
    assert.notEqual(fingerprint(mit("A")), fingerprint(mit("B")));
    assert.equal(tidy("\n\n a \n\n\n b  \n\n"), " a\n\n\n b");
  });
});

describe("what is listed", () => {
  const node = (id, deps) => ({ id, deps: deps.map(([pkgId, kind]) => ({ pkg: pkgId, dep_kinds: [{ kind }] })) });
  const metadata = {
    packages: ["app", "cli", "tool", "used", "build-only", "test-only", "deep", "unused"].map((name) => ({
      id: name,
      name,
      version: "1.0.0",
      license: "MIT",
      repository: null,
      manifest_path: `/registry/${name}/Cargo.toml`,
      authors: [],
    })),
    workspace_members: ["app", "cli", "tool"],
    resolve: {
      nodes: [
        node("app", [["used", null], ["build-only", "build"], ["test-only", "dev"], ["cli", null]]),
        node("cli", []),
        node("tool", [["unused", null]]),
        node("used", [["deep", null]]),
        node("build-only", []),
        node("test-only", []),
        node("deep", []),
        node("unused", []),
      ],
    },
  };

  it("follows normal and build dependencies from the released packages, never dev ones", () => {
    const names = reachable(metadata, ["app", "cli"]).map((p) => p.name).sort();
    // Not the workspace's own packages, not what only a test uses, not what only an
    // unreleased package uses.
    assert.deepEqual(names, ["build-only", "deep", "used"]);
    assert.equal(reachable(metadata, ["app"])[0].dir.startsWith("/registry/"), true);
    assert.throws(() => reachable(metadata, ["app", "gone"]), /no workspace package named gone/);
  });

  it("flattens pnpm's listing, one entry per version", () => {
    const entries = npmEntries({
      MIT: [{ name: "left", versions: ["1.0.0", "2.0.0"], paths: ["/m/left1", "/m/left2"], license: "MIT", author: "A", homepage: "https://left" }],
      "OFL-1.1": [{ name: "font", versions: ["5.3.0"], paths: ["/m/font"], license: "OFL-1.1" }],
    });
    assert.deepEqual(
      entries.map((e) => [e.name, e.version, e.license, e.dir]),
      [
        ["left", "1.0.0", "MIT", "/m/left1"],
        ["left", "2.0.0", "MIT", "/m/left2"],
        ["font", "5.3.0", "OFL-1.1", "/m/font"],
      ],
    );
    assert.deepEqual(entries[0].authors, ["A"]);
  });

  it("reads the ids of a licence expression", () => {
    assert.deepEqual(licenceIds("MIT OR Apache-2.0"), ["MIT", "Apache-2.0"]);
    assert.deepEqual(licenceIds("MIT/Apache-2.0"), ["MIT", "Apache-2.0"]);
    assert.deepEqual(licenceIds("(Apache-2.0 WITH LLVM-exception) OR MIT"), ["Apache-2.0", "LLVM-exception", "MIT"]);
    assert.deepEqual(licenceIds(""), []);
  });
});

describe("a package without a licence file", () => {
  const withFiles = [
    pkg("a", { files: [{ file: "LICENSE", text: mit("Ann") }] }),
    pkg("b", { files: [{ file: "LICENSE", text: mit("Bob") }] }),
    pkg("odd", { files: [{ file: "LICENSE", text: `Copyright (c) Odd\n\nAnother wording of MIT.` }] }),
    pkg("ap", { license: "Apache-2.0", files: [{ file: "LICENSE", text: APACHE }] }),
    pkg("dual", { license: "MIT OR Apache-2.0", files: [{ file: "LICENSE-APACHE", text: APACHE }, { file: "LICENSE-MIT", text: mit("Dee") }] }),
  ];

  it("gets the text most packages of that licence carry, without their holders", () => {
    const standard = standardTexts(withFiles);
    assert.equal(standard.get("MIT"), `MIT License\n\n${MIT_BODY}`);
    assert.equal(standard.get("Apache-2.0"), APACHE);
    assert.equal(standard.has("Zlib"), false);
    assert.equal(withoutHolders("A\n\nCopyright 2020 X\nAll rights reserved.\n\n\nB"), "A\n\nB");
  });

  it("borrows from the same repository only under the same licences, else takes a standard text", () => {
    const packages = [
      ...withFiles,
      pkg("a-macros", { repository: "https://example.org/a.git", files: [] }),
      // The same repository, but another licence: its sibling's MIT file isn't its licence.
      pkg("a-other", { repository: "https://example.org/a", license: "Apache-2.0", files: [] }),
      pkg("alone", { license: "Zlib OR Apache-2.0 OR MIT", authors: ["Zed <z@example.org>"], files: [] }),
      pkg("nameless", { license: "Apache-2.0 OR Zlib", files: [] }),
      pkg("unknown", { license: "Fancy-1.0", files: [] }),
    ];
    const filled = new Map(fillMissing(packages).map((p) => [p.name, p]));
    assert.equal(filled.get("a").borrowed, undefined);
    assert.equal(filled.get("a-macros").borrowed, "the files of a 1.0.0, from the same repository");
    assert.deepEqual(filled.get("a-macros").files, withFiles[0].files);
    assert.match(filled.get("a-other").borrowed, /^the standard text of Apache-2\.0/);
    assert.equal(filled.get("a-other").files[0].text, APACHE);
    // MIT first when it is offered, with the package's authors as the holders.
    assert.match(filled.get("alone").borrowed, /^the standard text of MIT/);
    assert.equal(filled.get("alone").files[0].text, `Copyright (c) Zed <z@example.org>\n\nMIT License\n\n${MIT_BODY}`);
    assert.match(filled.get("nameless").borrowed, /^the standard text of Apache-2\.0/);
    // Nothing fits: it stays without a text, and the file says so.
    assert.deepEqual(filled.get("unknown").files, []);
  });
});

describe("the file", () => {
  const packages = fillMissing([
    pkg("zeta", { version: "1.10.0", files: [{ file: "LICENSE", text: mit("Zoe") }] }),
    pkg("zeta", { version: "1.9.0", files: [{ file: "LICENSE", text: mit("Zoe") }] }),
    pkg("Alpha", { files: [{ file: "LICENSE-APACHE", text: APACHE }] }),
    pkg("beta", { license: "Apache-2.0", files: [{ file: "LICENSE", text: `${APACHE}\n` }] }),
    pkg("react-thing", { kind: "npm", files: [{ file: "LICENSE", text: mit("R") }] }),
    pkg("price table", { kind: "other", version: "", note: "trimmed", files: [{ file: "x.LICENSE", text: mit("M") }] }),
    pkg("mystery", { license: "Fancy-1.0", files: [] }),
  ]);
  const text = render(packages);

  it("lists every package once, in an order no locale changes", () => {
    const crates = text.slice(text.indexOf("Rust crates (5)"), text.indexOf("npm packages"));
    const names = crates.split("\n").filter((line) => line.startsWith("  ")).map((line) => line.trim().split(" | ")[0]);
    assert.deepEqual(names, ["Alpha 1.0.0", "beta 1.0.0", "mystery 1.0.0", "zeta 1.9.0", "zeta 1.10.0"]);
    assert.match(text, /npm packages in the desktop app \(the Inter font among them\) \(1\)\n\n {2}react-thing 1\.0\.0 \| MIT/);
    assert.match(text, /Other \(1\)\n\n {2}price table \| MIT \| https:\/\/example\.org\/price table \| trimmed/);
    assert.equal(plainOrder("Zeta", "alpha"), -1);
    assert.equal(versionOrder("1.10.0", "1.9.0"), 1);
    assert.equal(versionOrder("1.0.0", "1.0.0"), 0);
    assert.equal(versionOrder("0.6.0+11769913", "0.6.0"), 1);
  });

  it("prints each distinct text once with the packages it belongs to", () => {
    const count = (needle) => text.split(needle).length - 1;
    assert.equal(count("TERMS AND CONDITIONS"), 1, "the Apache text of two packages, once");
    assert.match(text, /Belongs to 2 packages:\n {2}Alpha 1\.0\.0 \(LICENSE-APACHE\)\n {2}beta 1\.0\.0 \(LICENSE\)/);
    assert.equal(count("Copyright (c) Zoe"), 1);
    assert.match(text, /Belongs to 2 packages:\n {2}zeta 1\.9\.0 \(LICENSE\)\n {2}zeta 1\.10\.0 \(LICENSE\)/);
    assert.match(text, /Packages that carry no licence file, and for whose licence no text is given above:\n {2}mystery 1\.0\.0 \| Fancy-1\.0/);
  });

  it("is the same text whatever order the packages came in, and ends with one line break", () => {
    assert.equal(render([...packages].reverse()), text);
    assert.equal(text.endsWith("\n"), true);
    assert.equal(text.endsWith("\n\n"), false);
    assert.equal(text.includes("\r"), false);
  });
});
