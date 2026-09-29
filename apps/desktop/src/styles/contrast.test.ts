// Text contrast of the Lamplight tokens (docs/design/macos-shell.md §8, §10), computed from the
// generated tokens.css itself: every context the app can be in (light/dark × normal/increased
// contrast × no backdrop/Mica) resolves its --pl-* values the way the cascade does, and text must
// reach WCAG AA on every surface it sits on, glass and Mica included.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/tokens.css"), "utf8");

/** Declarations of one top-level block, e.g. `:root.dark` (not the @media copies). */
function block(selector: string): Record<string, string> {
  const start = css.indexOf(`\n${selector} {\n`);
  if (start < 0) throw new Error(`no block ${selector}`);
  const body = css.slice(start + selector.length + 4, css.indexOf("\n}\n", start));
  const decls: Record<string, string> = {};
  for (const m of body.matchAll(/--(pl-[a-z0-9-]+):\s*([^;]+);/g)) {
    decls[m[1] as string] = (m[2] as string).trim();
  }
  return decls;
}

type Ctx = { dark: boolean; more: boolean; mica: boolean };

/** The --pl-* values for a context, applying blocks in the cascade's order. */
function resolve(ctx: Ctx): (name: string) => string {
  const selectors = [":root"];
  if (ctx.dark) selectors.push(":root.dark");
  if (ctx.mica) selectors.push(':root[data-backdrop="mica"]');
  if (ctx.more) selectors.push(':root[data-contrast="more"]');
  if (ctx.dark && ctx.mica) selectors.push(':root.dark[data-backdrop="mica"]');
  if (ctx.dark && ctx.more) selectors.push(':root.dark[data-contrast="more"]');
  if (ctx.dark && ctx.mica && ctx.more)
    selectors.push(':root.dark[data-backdrop="mica"][data-contrast="more"]');
  const values: Record<string, string> = {};
  for (const s of selectors) Object.assign(values, block(s));
  const get = (name: string): string => {
    const v = values[name];
    if (!v) throw new Error(`--${name} unset`);
    const alias = /^var\(--(pl-[a-z0-9-]+)\)$/.exec(v);
    return alias ? get(alias[1] as string) : v;
  };
  return get;
}

type Rgba = [number, number, number, number];

/** CSS Color 4: oklch → OKLab → linear sRGB → sRGB (0…1), gamut-clipped. */
function parse(color: string): Rgba {
  const oklch = /^oklch\(([\d.]+) ([\d.]+) ([\d.]+)(?: \/ ([\d.]+))?\)$/.exec(color);
  if (oklch) {
    const [L, C, h, a] = [Number(oklch[1]), Number(oklch[2]), Number(oklch[3]), oklch[4]];
    const A = C * Math.cos((h * Math.PI) / 180);
    const B = C * Math.sin((h * Math.PI) / 180);
    const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
    const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
    const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
    const lin = [
      4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
      -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
      -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
    ];
    const enc = (x: number) => {
      const c = Math.min(1, Math.max(0, x));
      return c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055;
    };
    return [enc(lin[0] ?? 0), enc(lin[1] ?? 0), enc(lin[2] ?? 0), a === undefined ? 1 : Number(a)];
  }
  const rgb = /^rgb\(([\d.]+) ([\d.]+) ([\d.]+)(?: \/ ([\d.]+))?\)$/.exec(color);
  if (rgb) {
    return [
      Number(rgb[1]) / 255,
      Number(rgb[2]) / 255,
      Number(rgb[3]) / 255,
      rgb[4] === undefined ? 1 : Number(rgb[4]),
    ];
  }
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex) {
    const n = Number.parseInt(hex[1] as string, 16);
    return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255, 1];
  }
  throw new Error(`can't parse ${color}`);
}

/** Source-over in sRGB, as browsers composite. */
function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top[3];
  return [
    top[0] * a + bottom[0] * (1 - a),
    top[1] * a + bottom[1] * (1 - a),
    top[2] * a + bottom[2] * (1 - a),
    1,
  ];
}

function luminance([r, g, b]: Rgba): number {
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(text: Rgba, background: Rgba): number {
  const fg = text[3] < 1 ? over(text, background) : text;
  const [a, b] = [luminance(fg), luminance(background)].sort((x, y) => y - x) as [number, number];
  return (a + 0.05) / (b + 0.05);
}

const CONTEXTS: (Ctx & { name: string })[] = [];
for (const dark of [false, true]) {
  for (const more of [false, true]) {
    for (const mica of [false, true]) {
      CONTEXTS.push({
        dark,
        more,
        mica,
        name: `${dark ? "dark" : "light"}${more ? " · more" : ""}${mica ? " · mica" : ""}`,
      });
    }
  }
}

/** Mica approximations used by the spec's measurements (§8): #F3F3F3 / #202020. */
const MICA = { light: parse("#f3f3f3"), dark: parse("#202020") };

describe("Lamplight token contrast (WCAG AA)", () => {
  it.each(CONTEXTS)("$name: text on every surface", (ctx) => {
    const t = resolve(ctx);
    const text = {
      primary: parse(t("pl-color-text-primary")),
      secondary: parse(t("pl-color-text-secondary")),
    };
    const content = parse(t("pl-color-surface-content"));
    const surfaces: Record<string, Rgba> = {
      content,
      raised: parse(t("pl-color-surface-raised")),
      sidebar: parse(t("pl-color-surface-sidebar")),
      popover: parse(t("pl-color-surface-popover")),
    };
    // Glass: its tint over what is usually behind it (the content paper), blur aside.
    surfaces.glass = over(parse(t("pl-color-glass-tint")), content);
    // index.css's quiet fills: a 5–7 % wash of ink (notes, skeletons, hover) on a raised card.
    const ink = parse(t("pl-color-text-primary"));
    surfaces["ink wash on raised"] = over(
      [ink[0], ink[1], ink[2], 0.07],
      parse(t("pl-color-surface-raised")),
    );
    if (ctx.mica) {
      const mica = ctx.dark ? MICA.dark : MICA.light;
      surfaces["mica card"] = over(parse(t("pl-color-mica-layer-fill")), mica);
      surfaces["glass on mica"] = over(parse(t("pl-color-glass-tint")), mica);
    }
    const report: string[] = [];
    for (const [surface, bg] of Object.entries(surfaces)) {
      for (const [role, fg] of Object.entries(text)) {
        const ratio = contrast(fg, bg);
        if (ratio < 4.5) report.push(`${role} on ${surface}: ${ratio.toFixed(2)}`);
      }
    }
    expect(report).toEqual([]);
  });

  it.each(CONTEXTS)("$name: focus ring and lamp rule stand out (3:1)", (ctx) => {
    const t = resolve(ctx);
    const content = parse(t("pl-color-surface-content"));
    // The ring is the brand accent (index.css); tokens' accent is the default brand.
    expect(contrast(parse(t("pl-color-accent")), content)).toBeGreaterThanOrEqual(3);
    expect(contrast(parse(t("pl-color-lamp-rule")), content)).toBeGreaterThanOrEqual(3);
  });
});
