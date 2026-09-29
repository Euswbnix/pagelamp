// i18n setup. Strings live in locales/<lang>/<namespace>.json — one namespace per screen
// (onboarding.json, courses.json, …) plus common.json for shared words.
//
// Adding a string: put it in BOTH locales/en/<ns>.json and locales/zh-CN/<ns>.json
// (src/i18n/i18n.test.ts fails if the keys differ). Write natural Chinese, not a word-for-word
// translation. The product name is available everywhere as {{product}}.

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { brand, type Locale, SUPPORTED_LOCALES } from "@/brand";
import { neutralize, restoreValues } from "./safeValues";

type Resources = Record<string, Record<string, Record<string, unknown>>>;

const files = import.meta.glob<Record<string, unknown>>("./locales/*/*.json", {
  eager: true,
  import: "default",
});

/** { en: { common: {...}, courses: {...} }, "zh-CN": { ... } } built from the file layout. */
export const resources: Resources = {};
for (const [path, messages] of Object.entries(files)) {
  const match = /\.\/locales\/([^/]+)\/([^/]+)\.json$/.exec(path);
  if (!match) continue;
  const [, lng, ns] = match as unknown as [string, string, string];
  resources[lng] ??= {};
  resources[lng][ns] = messages;
}

export const NAMESPACES = Object.keys(resources.en ?? {});

export function isSupportedLocale(value: unknown): value is Locale {
  return SUPPORTED_LOCALES.includes(value as Locale);
}

export function initI18n(locale: Locale | null = null) {
  if (i18n.isInitialized) return i18n;
  i18n
    .use(initReactI18next)
    .use(restoreValues)
    .init({
      resources,
      lng: locale ?? brand.defaultLocale,
      fallbackLng: "en",
      supportedLngs: [...SUPPORTED_LOCALES],
      ns: NAMESPACES,
      defaultNS: "common",
      interpolation: {
        // Not HTML escaping (React escapes text): values are neutralised so markup inside them
        // ("{{when}}", "$t(…)") is never acted on — see safeValues.ts.
        escapeValue: true,
        escape: neutralize,
        skipOnVariables: true,
        defaultVariables: { product: brand.productName },
      },
      postProcess: ["restoreValues"],
      returnNull: false,
    });
  return i18n;
}

export default i18n;
