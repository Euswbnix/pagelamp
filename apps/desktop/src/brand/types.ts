// A "brand" is everything a distribution may change without touching screen code: product
// name, colours, logo, links, default language and school-specific wording.
//
// To make a distribution: copy brands/default.ts to brands/<id>.ts, edit it, and build with
// `VITE_BRAND=<id>`. See README.md → "Branding".

export type Locale = "en" | "zh-CN";

/** Text in every supported language. English is required; other languages fall back to it. */
export type Localized = { en: string } & Partial<Record<Exclude<Locale, "en">, string>>;

/** A CSS colour, e.g. "oklch(0.47 0.08 250)" or "#2f5d8a". */
export type CssColor = string;

export interface BrandColors {
  /** Main accent (buttons, focus rings, active nav) in the light theme. */
  accent: CssColor;
  /** Text/icon colour on top of `accent`. */
  accentForeground: CssColor;
  /** Accent in the dark theme (usually lighter). */
  accentDark: CssColor;
  accentDarkForeground: CssColor;
}

export interface BrandLinks {
  /** Project homepage / source code. */
  homepage?: string;
  /** Where students get help (FAQ, docs). */
  help?: string;
  /** Where to report a problem. */
  issues?: string;
}

export interface Brand {
  /** Must match the file name in brands/ and the VITE_BRAND value. */
  id: string;
  /** Shown in the sidebar, window title and throughout the copy ({{product}}). */
  productName: string;
  tagline: Localized;
  /** Language used until the student picks one in Settings. */
  defaultLocale: Locale;
  colors: BrandColors;
  /**
   * Logo image URLs (import an SVG/PNG from ../assets). `null` = the built-in neutral mark.
   * Never ship a logo you don't have the rights to.
   */
  logo: { light: string; dark?: string } | null;
  links: BrandLinks;
  /** Optional credit line in Settings → About, e.g. "Maintained by …". */
  distributedBy?: Localized;
  /**
   * Shown in the course AI-policy editor. Put school-specific guidance here, e.g. what your
   * university's default rule is when a syllabus says nothing.
   */
  aiPolicyHint: Localized;
}
