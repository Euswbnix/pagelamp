// Tests for apps/macos/scripts/gen-strings.mjs. Run: node --test apps/macos/scripts/test/*.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, describe, it } from "node:test";
import {
  buildCatalog,
  flattenMessages,
  generate,
  loadProductName,
  PATHS,
  REPO_ROOT,
  renderStrings,
  renderStringsdict,
  SCRIPT_DIR,
  toAppleFormat,
} from "../gen-strings.mjs";

const script = join(SCRIPT_DIR, "gen-strings.mjs");
const temps = [];
after(() => {
  for (const d of temps) rmSync(d, { recursive: true, force: true });
});

/** A throwaway repository with the given locale files, Mac fragments and brand. */
function makeRoot({ shared = {}, fragments = {}, productName = "Lamp" } = {}) {
  const root = mkdtempSync(join(tmpdir(), "pl-strings-"));
  temps.push(root);
  const write = (rel, content) => {
    mkdirSync(dirname(join(root, rel)), { recursive: true });
    writeFileSync(join(root, rel), typeof content === "string" ? content : JSON.stringify(content, null, 2));
  };
  for (const [lang, namespaces] of Object.entries(shared))
    for (const [ns, tree] of Object.entries(namespaces)) write(`${PATHS.locales}/${lang}/${ns}.json`, tree);
  for (const [file, tree] of Object.entries(fragments)) write(`${PATHS.fragments}/${file}`, tree);
  write(`${PATHS.brands}/default.ts`, `const brand = {\n  id: "default",\n  productName: "${productName}",\n};\nexport default brand;\n`);
  return root;
}
const catalog = (en, zh, product = "Lamp") => buildCatalog({ en: new Map(Object.entries(en)), "zh-CN": new Map(Object.entries(zh)) }, product);
const byKey = (entries) => Object.fromEntries(entries.map((e) => [e.key, e]));

describe("flattening", () => {
  it("turns nested namespaces into dotted keys, in file order", () => {
    assert.deepEqual(flattenMessages({ a: { b: "x", c: { d: "y" } }, e: "z" }, "common", "f"), [
      ["common.a.b", "x"],
      ["common.a.c.d", "y"],
      ["common.e", "z"],
    ]);
  });

  it("rejects non-string values and dotted keys", () => {
    assert.throws(() => flattenMessages({ a: 1 }, "ns", "f.json"), /f\.json: ns\.a must be a string/);
    assert.throws(() => flattenMessages({ a: ["x"] }, "ns", "f.json"), /an array/);
    assert.throws(() => flattenMessages({ "a.b": "x" }, "ns", "f.json"), /contains "\."/);
  });
});

describe("placeholders", () => {
  it("numbers by first appearance in English and keeps the numbers in Chinese", () => {
    const [e] = catalog({ "c.k": "{{indexed}} of {{count}} readable" }, { "c.k": "{{count}} 份中 {{indexed}} 份可读取" });
    assert.deepEqual(e.args, ["indexed", "count"]);
    assert.equal(toAppleFormat(e.texts.en, e.args), "%1$@ of %2$@ readable");
    assert.equal(toAppleFormat(e.texts["zh-CN"], e.args), "%2$@ 份中 %1$@ 份可读取");
  });

  it("reuses the number of a repeated placeholder", () => {
    const [e] = catalog({ "c.k": "{{a}} and {{b}}, again {{a}}" }, { "c.k": "{{b}}{{a}}" });
    assert.equal(toAppleFormat(e.texts.en, e.args), "%1$@ and %2$@, again %1$@");
  });

  it("substitutes {{product}} from the brand and does not count it as an argument", () => {
    const [e] = catalog({ "c.k": "Welcome to {{ product }}, {{name}}" }, { "c.k": "欢迎使用 {{product}}，{{name}}" }, "PageLamp");
    assert.deepEqual(e.args, ["name"]);
    assert.equal(toAppleFormat(e.texts.en, e.args), "Welcome to PageLamp, %1$@");
    assert.equal(toAppleFormat(e.texts["zh-CN"], e.args), "欢迎使用 PageLamp，%1$@");
  });

  it("escapes % only in strings that take arguments", () => {
    const entries = byKey(catalog({ "c.a": "100% of {{n}}", "c.b": "100% sure" }, { "c.a": "{{n}} 的 100%", "c.b": "百分之百" }));
    assert.equal(toAppleFormat(entries["c.a"].texts.en, entries["c.a"].args), "100%% of %1$@");
    assert.equal(toAppleFormat(entries["c.b"].texts.en, entries["c.b"].args), "100% sure");
  });

  it("rejects placeholders that differ between languages", () => {
    assert.throws(() => catalog({ "c.k": "Hi {{name}}" }, { "c.k": "你好 {{who}}" }), /zh-CN c\.k uses \{\{who\}\} but en uses \{\{name\}\}/);
  });

  it("rejects i18next features Apple strings cannot express", () => {
    assert.throws(() => catalog({ "c.k": "{{n, number}}" }, { "c.k": "{{n, number}}" }), /unsupported interpolation/);
    assert.throws(() => catalog({ "c.k": "$t(c.other)" }, { "c.k": "$t(c.other)" }), /nesting/);
  });
});

describe("plurals", () => {
  const en = { "c.files_one": "{{count}} file in {{course}}", "c.files_other": "{{count}} files in {{course}}", "c.plain": "Plain" };
  const zh = { "c.files_one": "{{course}} 里有 {{count}} 个文件", "c.files_other": "{{course}} 里有 {{count}} 个文件", "c.plain": "普通" };
  const entries = catalog(en, zh);

  it("groups _one/_other into one stringsdict key; count is an integer", () => {
    const e = byKey(entries)["c.files"];
    assert.equal(e.plural, true);
    assert.deepEqual(e.args, ["count", "course"]);
    const en = renderStringsdict(entries, "en");
    assert.match(en, /<key>c\.files<\/key>/);
    assert.match(en, /<string>%1\$#@count@<\/string>/);
    assert.match(en, /<key>one<\/key>\n\t+<string>%1\$ld file in %2\$@<\/string>/);
    assert.match(en, /<key>other<\/key>\n\t+<string>%1\$ld files in %2\$@<\/string>/);
    assert.doesNotMatch(renderStrings(entries, "en"), /c\.files/);
  });

  it("emits only the categories Chinese has (other)", () => {
    const zhDict = renderStringsdict(entries, "zh-CN");
    assert.doesNotMatch(zhDict, /<key>one<\/key>/);
    assert.match(zhDict, /<key>other<\/key>\n\t+<string>%2\$@ 里有 %1\$ld 个文件<\/string>/);
  });

  it("gives count an argument even when no form prints it", () => {
    const e = byKey(catalog({ "c.x_one": "One {{thing}}", "c.x_other": "Many {{thing}}" }, { "c.x_one": "{{thing}}", "c.x_other": "{{thing}}" }))["c.x"];
    assert.deepEqual(e.args, ["thing", "count"]);
  });

  it("rejects a plural without _other and a plural that is also a plain key", () => {
    assert.throws(() => catalog({ "c.x_one": "a" }, { "c.x_one": "a" }), /plural forms need c\.x_other/);
    assert.throws(() => catalog({ "c.x": "a", "c.x_other": "b" }, { "c.x": "a", "c.x_other": "b" }), /both a plain key and a plural/);
  });
});

describe("parity", () => {
  it("reports keys missing on either side", () => {
    assert.throws(() => catalog({ "c.a": "A", "c.b": "B" }, { "c.a": "甲", "c.c": "丙" }), /missing in zh-CN: c\.b[\s\S]*only in zh-CN: c\.c/);
  });

  it("requires a zh-CN partner for every Mac fragment and rejects duplicate keys", () => {
    const shared = { en: { common: { ok: "OK" } }, "zh-CN": { common: { ok: "好" } } };
    const lonely = makeRoot({ shared, fragments: { "mac.en.json": { a: "A" } } });
    assert.throws(() => generate(lonely), /fragments differ between en \[mac\] and zh-CN \[\]/);
    const dup = makeRoot({
      shared,
      fragments: {
        "mac.en.json": { course: { title: "A" } },
        "mac.zh-CN.json": { course: { title: "甲" } },
        "thisweek.en.json": { course: { title: "B" } },
        "thisweek.zh-CN.json": { course: { title: "乙" } },
      },
    });
    assert.throws(() => generate(dup), /mac\.course\.title is defined in both .*mac\.en\.json and .*thisweek\.en\.json/);
  });

  it("merges every fragment under mac.*", () => {
    const root = makeRoot({
      shared: { en: { common: { ok: "OK" } }, "zh-CN": { common: { ok: "好" } } },
      fragments: {
        "mac.en.json": { nav: { home: "Home" } },
        "mac.zh-CN.json": { nav: { home: "首页" } },
        "setup.en.json": { setup: { title: "Set Up {{product}}" } },
        "setup.zh-CN.json": { setup: { title: "设置 {{product}}" } },
      },
    });
    const { entries } = generate(root);
    assert.deepEqual(
      entries.map((e) => e.key),
      ["common.ok", "mac.nav.home", "mac.setup.title"],
    );
    assert.equal(byKey(entries)["mac.setup.title"].texts.en, "Set Up Lamp");
  });

  it("reads productName from the brand file", () => {
    assert.equal(loadProductName(REPO_ROOT), "PageLamp");
  });
});

describe(".strings output", () => {
  it("escapes quotes, backslashes and newlines, sorted by key", () => {
    const entries = catalog({ "z.b": 'Say "hi"\\now', "z.a": "line\nbreak" }, { "z.b": "说“嗨”", "z.a": "换\n行" });
    assert.equal(renderStrings(entries, "en").split("\n").slice(2).join("\n"), '"z.a" = "line\\nbreak";\n"z.b" = "Say \\"hi\\"\\\\now";\n');
  });

  const plutil = process.platform === "darwin" ? spawnSync("plutil", ["-help"]) : { error: true };
  it("the generated files are valid property lists", { skip: plutil.error ? "needs plutil" : false }, () => {
    const root = makeRoot();
    const { files } = generate(REPO_ROOT);
    for (const f of files.filter((x) => !x.path.endsWith(".swift"))) {
      const p = join(root, f.path);
      mkdirSync(dirname(p), { recursive: true });
      writeFileSync(p, f.content);
      const r = spawnSync("plutil", ["-lint", p], { encoding: "utf8" });
      assert.equal(r.status, 0, r.stdout + r.stderr);
    }
  });
});

describe("--check", () => {
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });

  it("passes on fresh output and fails on drift", () => {
    const root = makeRoot({
      shared: { en: { common: { hi: "Hi {{name}}", n_one: "{{count}} item", n_other: "{{count}} items" } }, "zh-CN": { common: { hi: "你好 {{name}}", n_one: "{{count}} 项", n_other: "{{count}} 项" } } },
      fragments: { "mac.en.json": { a: "A" }, "mac.zh-CN.json": { a: "甲" } },
    });
    assert.equal(run("--root", root).status, 0);
    assert.equal(run("--check", "--root", root).status, 0);
    writeFileSync(join(root, PATHS.fragments, "mac.zh-CN.json"), JSON.stringify({ a: "乙" }));
    const drift = run("--check", "--root", root);
    assert.equal(drift.status, 1);
    assert.match(drift.stderr, /zh-Hans\.lproj\/Localizable\.strings/);
    assert.doesNotMatch(drift.stderr, /en\.lproj/);
  });

  it("exits 2 with a message on a parity error", () => {
    const root = makeRoot({ shared: { en: { common: { a: "A" } }, "zh-CN": { common: {} } } });
    const r = run("--check", "--root", root);
    assert.equal(r.status, 2);
    assert.match(r.stderr, /missing in zh-CN: common\.a/);
  });

  it("the committed files are up to date", () => {
    const r = run("--check");
    assert.equal(r.status, 0, r.stderr);
  });
});

describe("the real catalog", () => {
  const { entries, files } = generate();
  it("keeps the i18next plural keys and the shared policy wording", () => {
    const e = byKey(entries);
    assert.equal(e["courses.thisWeek.summary"].plural, true);
    assert.match(e["common.disclosure.full"].texts.en, /from PageLamp and sends them/);
    const resources = files.filter((f) => !f.path.endsWith(".swift"));
    assert.equal(resources.length, 4);
    assert.ok(!resources.some((f) => f.content.includes("{{")), "no i18next placeholder survives");
  });

  const sdk = process.platform === "darwin" ? spawnSync("xcrun", ["--sdk", "macosx", "--show-sdk-path"], { encoding: "utf8" }) : null;
  it("L10nKeys.swift typechecks (Swift 6, macOS 26, warnings as errors, either default isolation)", { skip: !sdk || sdk.status !== 0 ? "needs Xcode" : false }, () => {
    const dir = mkdtempSync(join(tmpdir(), "pl-keys-"));
    temps.push(dir);
    const file = join(dir, "L10nKeys.swift");
    writeFileSync(file, files.find((f) => f.path === PATHS.keys).content);
    const base = ["swiftc", "-typecheck", "-swift-version", "6", "-warnings-as-errors", "-sdk", sdk.stdout.trim(), "-target", "arm64-apple-macos26.0"];
    for (const isolation of [[], ["-default-isolation", "MainActor"]]) {
      const r = spawnSync("xcrun", [...base, ...isolation, file], { encoding: "utf8" });
      assert.equal(r.status, 0, `${isolation.join(" ") || "nonisolated"}: ${r.stderr}`);
    }
  });
});
