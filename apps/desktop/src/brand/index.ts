// Picks the brand at build time: VITE_BRAND=<id> selects brands/<id>.ts (default: "default").
// vite.config.ts fails the build if that file does not exist.

import type { Brand, Locale, Localized } from "./types";

export type { Brand, Locale, Localized } from "./types";

const brands = import.meta.glob<{ default: Brand }>("./brands/*.ts", { eager: true });

function selectBrand(): Brand {
  const id = import.meta.env.VITE_BRAND || "default";
  const selected = brands[`./brands/${id}.ts`]?.default;
  if (!selected) throw new Error(`Unknown brand "${id}" (no src/brand/brands/${id}.ts)`);
  return selected;
}

export const brand: Brand = selectBrand();

/** Pick the string for `locale`, falling back to English. */
export function localized(text: Localized, locale: string): string {
  return (locale === "zh-CN" ? text["zh-CN"] : undefined) ?? text.en;
}

export const SUPPORTED_LOCALES: readonly Locale[] = ["en", "zh-CN"];

/** Apply the brand's accent colours to the shadcn theme variables (see src/index.css). */
export function applyBrandTheme(b: Brand = brand): void {
  const id = "brand-theme";
  let style = document.getElementById(id) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = id;
    document.head.append(style);
  }
  const { accent, accentForeground, accentDark, accentDarkForeground } = b.colors;
  style.textContent = [
    `:root{--brand:${accent};--brand-foreground:${accentForeground};}`,
    `.dark{--brand:${accentDark};--brand-foreground:${accentDarkForeground};}`,
  ].join("\n");
}
