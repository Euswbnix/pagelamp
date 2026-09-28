// Tests for design/tokens/gen-tokens.mjs. Run: node --test design/tokens/test/*.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, describe, it } from "node:test";
import { fileURLToPath } from "node:url";
import {
  BEZIER_TOLERANCE,
  bezierError,
  buildCssRules,
  colorToRgb,
  cssLinear,
  generate,
  hexOf,
  loadResolver,
  OUTPUTS,
  REPO_ROOT,
  resolveTokens,
  resolveTree,
  settleMs,
  springCurve,
  TOKENS_DIR,
} from "../gen-tokens.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const script = join(TOKENS_DIR, "gen-tokens.mjs");
const temps = [];
const tempDir = () => {
  const d = mkdtempSync(join(tmpdir(), "pl-tokens-"));
  temps.push(d);
  return d;
};
after(() => {
  for (const d of temps) rmSync(d, { recursive: true, force: true });
});
const channelDelta = (a, b) => Math.max(...a.map((x, i) => Math.abs(x - b[i])));
const hexToRgb = (hex) => [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16) / 255);

describe("OKLCH → sRGB", () => {
  // Reference values from CSS Color 4 / Björn Ottosson's OKLab (the sRGB primaries and mid grey).
  const known = [
    [[1, 0, 0], "#ffffff"],
    [[0, 0, 0], "#000000"],
    [[0.59987, 0, 0], "#808080"],
    [[0.62796, 0.25768, 29.2339], "#ff0000"],
    [[0.86644, 0.29483, 142.4953], "#00ff00"],
    [[0.45201, 0.31321, 264.052], "#0000ff"],
  ];
  for (const [lch, hex] of known) {
    it(`oklch(${lch.join(" ")}) is ${hex} within 1/255`, () => {
      const { srgb } = colorToRgb({ colorSpace: "oklch", components: lch });
      assert.ok(channelDelta(srgb, hexToRgb(hex)) <= 1 / 255 + 1e-9, `${hexOf(srgb)} vs ${hex}`);
    });
  }

  // Rows of the spec's colour table (docs/design/macos-shell.md §10.1), typed in independently.
  const spec = [
    [[0.47, 0.08, 252], "#385d86"],
    [[0.78, 0.07, 252], "#97bbe4"],
    [[0.86, 0.1, 78], "#f5c984"],
    [[0.74, 0.11, 72], "#d69f58"],
    [[0.6, 0.12, 78], "#a77612"],
    [[0.62, 0.15, 55], "#c9690c"],
    [[0.86, 0.1, 65], "#ffc48a"],
    [[0.56, 0.19, 27], "#cc3430"],
    [[0.22, 0.012, 260], "#171b20"],
    [[0.13, 0.02, 252], "#03080f"],
  ];
  for (const [lch, hex] of spec) {
    it(`spec §10.1 oklch(${lch.join(" ")}) is ${hex} within 1/255`, () => {
      const { srgb, inGamut } = colorToRgb({ colorSpace: "oklch", components: lch });
      assert.ok(inGamut);
      assert.ok(channelDelta(srgb, hexToRgb(hex)) <= 1 / 255 + 1e-9, `${hexOf(srgb)} vs ${hex}`);
    });
  }

  it("every colour token, in every permutation, matches its declared hex", () => {
    const loaded = loadResolver();
    let checked = 0;
    for (const theme of ["light", "dark"])
      for (const contrast of ["normal", "more"])
        for (const backdrop of ["none", "mica"]) {
          const { all } = resolveTokens(resolveTree(loaded, { theme, contrast, backdrop }));
          for (const t of all.filter((x) => x.type === "color" && x.value.hex)) {
            assert.ok(channelDelta(colorToRgb(t.value).srgb, hexToRgb(t.value.hex)) <= 1 / 255 + 1e-9, t.name);
            checked++;
          }
        }
    assert.ok(checked > 100);
  });

  it("converts sRGB red to Display P3 (0.9175, 0.2003, 0.1386)", () => {
    const { p3 } = colorToRgb({ colorSpace: "oklch", components: [0.62796, 0.25768, 29.2339] });
    assert.ok(channelDelta(p3, [0.9175, 0.2003, 0.1386]) <= 1 / 255, p3.join(" "));
  });

  it("flags colours outside sRGB (they get Display P3 in Swift)", () => {
    const vivid = { colorSpace: "oklch", components: [0.7, 0.25, 145] };
    assert.equal(colorToRgb(vivid).inGamut, false);
    const swift = generate().find((f) => f.path === OUTPUTS.swift).content;
    assert.ok(!swift.includes(".displayP3("), "no current token is outside sRGB");
  });
});

describe("springs", () => {
  const fixture = JSON.parse(readFileSync(join(here, "fixtures", "swiftui-springs.json"), "utf8"));
  const loaded = loadResolver();
  const { byName } = resolveTokens(resolveTree(loaded));

  for (const f of fixture) {
    it(`${f.token} matches SwiftUI's Spring (stiffness, damping, response)`, () => {
      const token = byName.get(f.token);
      const s = token.ext.spring;
      assert.equal(s.duration, Number(f.duration.toFixed(4)));
      assert.equal(s.bounce, Number(f.bounce.toFixed(4)));
      const c = springCurve(s.duration, s.bounce);
      assert.ok(Math.abs(c.stiffness - f.stiffness) < 1e-6, `stiffness ${c.stiffness} vs ${f.stiffness}`);
      assert.ok(Math.abs(c.damping - f.damping) < 1e-6, `damping ${c.damping} vs ${f.damping}`);
      assert.equal(f.mass, 1);
      // the formula of spec §10.4
      assert.ok(Math.abs(c.stiffness - ((2 * Math.PI) / s.duration) ** 2) < 1e-9);
      assert.ok(Math.abs(c.damping - (4 * Math.PI * (1 - s.bounce)) / s.duration) < 1e-9);
      // the stored, rounded values
      assert.equal(s.stiffness, Number(f.stiffness.toFixed(2)));
      assert.equal(s.damping, Number(f.damping.toFixed(2)));
      f.samplesEvery50ms.forEach((y, i) => {
        assert.ok(Math.abs(c.x(i * 0.05) - y) < 1e-4, `t=${i * 0.05}: ${c.x(i * 0.05)} vs SwiftUI ${y}`);
      });
    });
  }

  it("settle time: within 5e-4 of the target from then on, not a millisecond earlier", () => {
    for (const f of fixture) {
      const s = byName.get(f.token).ext.spring;
      const { x } = springCurve(s.duration, s.bounce);
      const settle = settleMs(x);
      assert.equal(s.settleMs, settle, f.token);
      for (let t = settle; t <= settle + 2000; t += 0.5) assert.ok(Math.abs(x(t / 1000) - 1) < 5e-4, `${f.token} at ${t} ms`);
      let outside = false;
      for (let t = settle - 1; t < settle; t += 0.1) outside ||= Math.abs(x(t / 1000) - 1) >= 5e-4;
      assert.ok(outside, `${f.token} already settled before ${settle} ms`);
    }
  });

  it("CSS linear() samples the spring and ends exactly at 1", () => {
    const { x } = springCurve(0.45, 0);
    const text = cssLinear(x, 717);
    const pts = text.slice("linear(".length, -1).split(", ").map(Number);
    assert.equal(pts.length, 25);
    assert.equal(pts[0], 0);
    assert.equal(pts.at(-1), 1);
    pts.slice(0, -1).forEach((p, i) => assert.ok(Math.abs(p - x((0.717 * i) / 24)) < 1e-4));
  });

  it("the stored cubic-bezier fallbacks stay close to the springs", () => {
    for (const f of fixture) {
      const t = byName.get(f.token);
      const { x } = springCurve(t.ext.spring.duration, t.ext.spring.bounce);
      assert.ok(bezierError(t.value.timingFunction, x, t.ext.spring.settleMs) <= BEZIER_TOLERANCE, f.token);
    }
  });
});

describe("CSS", () => {
  const files = generate();
  const css = files.find((f) => f.path === OUTPUTS.css).content;

  it("has a block per non-empty context and the media fallbacks", () => {
    for (const sel of [
      ":root {",
      ":root.dark {",
      ':root[data-contrast="more"] {',
      ':root.dark[data-contrast="more"] {',
      ':root[data-platform="windows"] {',
      ':root[data-platform="linux"] {',
      ':root[data-backdrop="mica"] {',
      ':root[data-transparency="reduced"] {',
      "@media (prefers-contrast: more) {",
      "@media (prefers-reduced-transparency: reduce) {",
    ])
      assert.ok(css.includes(sel), sel);
  });

  it("the cascade reproduces every resolver permutation (checked by the generator)", () => {
    assert.doesNotThrow(() => buildCssRules(loadResolver()));
  });

  it("increased contrast in dark gets the IC-dark values; glass tint aliases the popover", () => {
    const block = css.slice(css.indexOf(':root.dark[data-contrast="more"] {'));
    assert.match(block.slice(0, block.indexOf("}")), /--pl-color-text-primary: oklch\(1 0 0\);/);
    assert.match(css, /--pl-color-glass-tint: var\(--pl-color-surface-popover\);/);
  });

  it("glass.css only uses variables tokens.css defines", () => {
    assert.doesNotThrow(() => generate());
    const glass = files.find((f) => f.path === OUTPUTS.glass).content;
    assert.match(glass, /\.pl-glass \{/);
  });

  it("output is deterministic", () => {
    assert.deepEqual(generate(), files);
  });
});

describe("Swift", () => {
  const swift = generate().find((f) => f.path === OUTPUTS.swift).content;

  it("declares the token enums and nothing named Glass", () => {
    for (const e of ["PLColor", "PLRadius", "PLSpace", "PLLayout", "PLSize", "PLType", "PLMotion"]) assert.match(swift, new RegExp(`^nonisolated enum ${e} \\{`, "m"));
    const code = swift.split("\n").filter((l) => !l.trim().startsWith("//"));
    assert.ok(!code.some((l) => /\bGlass\b|glassEffect/.test(l)));
  });

  it("uses the four appearances and skips system-provided tokens", () => {
    assert.match(swift, /accessibilityHighContrastDarkAqua/);
    assert.ok(!swift.includes("textPrimary"), "text colours are the system's on the Mac");
    assert.match(swift, /static let lampWashOpacityDark: Double = 0\.17/);
    assert.match(swift, /static let calm: Animation = \.smooth\(duration: 0\.45\)/);
    assert.match(swift, /static let quickSpring = Spring\(duration: 0\.3, bounce: 0\.15\)/);
    assert.match(swift, /static let weekButtonMin: CGFloat = 26/);
  });

  const sdk = process.platform === "darwin" ? spawnSync("xcrun", ["--sdk", "macosx", "--show-sdk-path"], { encoding: "utf8" }) : null;
  it("typechecks with swiftc (Swift 6, macOS 26, warnings as errors, either default isolation)", { skip: !sdk || sdk.status !== 0 ? "needs Xcode" : false }, () => {
    const dir = tempDir();
    const file = join(dir, "PLTokens.swift");
    writeFileSync(file, swift);
    const base = ["swiftc", "-typecheck", "-swift-version", "6", "-warnings-as-errors", "-sdk", sdk.stdout.trim(), "-target", "arm64-apple-macos26.0"];
    for (const isolation of [[], ["-default-isolation", "MainActor"]]) {
      const r = spawnSync("xcrun", [...base, ...isolation, file], { encoding: "utf8" });
      assert.equal(r.status, 0, `${isolation.join(" ") || "nonisolated"}: ${r.stderr}`);
    }
  });
});

describe("validation", () => {
  const copyTokens = () => {
    const dir = tempDir();
    cpSync(TOKENS_DIR, dir, { recursive: true, filter: (src) => !src.startsWith(join(TOKENS_DIR, "test")) });
    return dir;
  };
  const edit = (dir, file, fn) => {
    const p = join(dir, file);
    writeFileSync(p, fn(readFileSync(p, "utf8")));
  };

  it("rejects a hex that does not match its OKLCH value", () => {
    const dir = copyTokens();
    edit(dir, "pagelamp.tokens.json", (s) => s.replace('"hex": "#385d86"', '"hex": "#3a5f88"'));
    assert.throws(() => generate(dir), /hex #3a5f88 does not match/);
  });

  it("rejects derived spring numbers that drift from duration + bounce", () => {
    const dir = copyTokens();
    edit(dir, "pagelamp.tokens.json", (s) => s.replace('"stiffness": 194.96', '"stiffness": 190'));
    assert.throws(() => generate(dir), /stiffness is 190, the formula gives 194.96/);
  });

  it("rejects an alias to a missing token", () => {
    const dir = copyTokens();
    edit(dir, "contrast/more.tokens.json", (s) => s.replace("{palette.ic.accent}", "{palette.ic.nope}"));
    assert.throws(() => generate(dir), /points to no token/);
  });

  it("rejects a context that adds a token the base set lacks", () => {
    const dir = copyTokens();
    edit(dir, "platform/linux.tokens.json", (s) => s.replace('"radius": {', '"radius": {\n    "extra": { "$value": { "value": 1, "unit": "px" } },'));
    assert.throws(() => generate(dir), /different tokens than the base set/);
  });
});

describe("--check", () => {
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });

  it("passes on fresh output and fails on drift or a missing file", () => {
    const root = tempDir();
    assert.equal(run("--root", root).status, 0);
    assert.equal(run("--check", "--root", root).status, 0);
    const css = join(root, OUTPUTS.css);
    writeFileSync(css, readFileSync(css, "utf8").replace("--pl-color-accent: oklch(0.47 0.08 252);", "--pl-color-accent: red;"));
    const drift = run("--check", "--root", root);
    assert.equal(drift.status, 1);
    assert.match(drift.stderr, /apps\/desktop\/src\/styles\/tokens\.css/);
    rmSync(join(root, OUTPUTS.swift));
    assert.match(run("--check", "--root", root).stderr, /PLTokens\.swift/);
  });

  it("the committed files are up to date", () => {
    const r = run("--check", "--root", REPO_ROOT);
    assert.equal(r.status, 0, r.stderr);
  });
});
