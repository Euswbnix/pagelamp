// Evidence items (code + params) → translated sentences (calendar design §3.3; CAL-53).
// Params follow the facade's key conventions: dates are ISO dates; `source`, `kind`, `reason`
// and `signal` are enum values we translate; counts and weeks are integers; anything else
// (a module title, a term name, a session code) is text written elsewhere, shown as is.
// A code with an optional param that is absent uses the variant "<code>_no_<param>". That text never goes through i18next's
// interpolation (a title like "{{week}}" would be filled in there): it is swapped in afterwards.

import type { TFunction } from "i18next";
import type { EvidenceCode, EvidenceItem } from "@/api/types";
import en from "@/i18n/locales/en/calendar.json";
import { formatIsoDate } from "@/lib/format";

// Compile-time half of CAL-53: every code the facade can send has English text (and the i18n
// test keeps zh-CN's keys equal to English's).
en.evidence satisfies Record<EvidenceCode | "unknown", string>;

const TEMPLATES: Record<string, string> = en.evidence;

const DATE_KEYS = new Set(["date", "start", "end", "since", "until", "monday"]);

export function isDateKey(key: string): boolean {
  return DATE_KEYS.has(key) || key.endsWith("_on");
}

export function isKnownCode(code: string): code is EvidenceCode {
  return code !== "unknown" && Object.hasOwn(TEMPLATES, code);
}

function placeholders(template: string): string[] {
  return [...template.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1] ?? "");
}

/**
 * The template key for `code` given the params present: "<code>_no_a_no_b" when the params
 * a and b the full sentence needs are absent (and that variant exists), else `code`. Also
 * returns the params still missing, which are then shown as "…" rather than "{{a}}".
 */
export function templateFor(code: EvidenceCode, present: Set<string>) {
  const needed = placeholders(TEMPLATES[code] ?? "").filter((p) => p !== "product");
  const missing = [...new Set(needed.filter((p) => !present.has(p)))].sort();
  if (missing.length === 0) return { key: code, missing };
  const variant = `${code}${missing.map((p) => `_no_${p}`).join("")}`;
  return Object.hasOwn(TEMPLATES, variant) ? { key: variant, missing: [] } : { key: code, missing };
}

// Marks where a plain-text param goes. The translations contain no control characters, and the
// swap is one pass over the translated sentence, so the text itself is never scanned again.
const MARK = "\u0001";
const MARKS = new RegExp(`${MARK}(\\d+)${MARK}`, "g");

/**
 * `t(key, params)`, where `texts` hold plain text written by someone else (an instructor's
 * module title, a provider's model name). Those values never go through i18next's
 * interpolation, which would fill in a "{{week}}" inside them; they are swapped in afterwards.
 */
export function translateWithText(
  // Any namespace's t: the key is built at run time.
  t: TFunction<"calendar"> | TFunction<"proposals">,
  key: string,
  params: Record<string, string | number>,
  texts: Record<string, string> = {},
): string {
  const values: string[] = [];
  const all: Record<string, string | number> = { ...params };
  for (const [name, value] of Object.entries(texts)) {
    all[name] = `${MARK}${values.length}${MARK}`;
    values.push(value);
  }
  const translate = t as unknown as (key: string, options: object) => string;
  return translate(key, all).replace(MARKS, (_, index: string) => values[Number(index)] ?? "");
}

/**
 * The params of one item, formatted for display in `locale`; plain-text params (anything that
 * isn't a date, a number or a known enum) come back separately in `texts`.
 */
export function evidenceParams(
  item: EvidenceItem,
  t: TFunction<"calendar">,
  locale: string,
): { params: Record<string, string>; texts: Record<string, string> } {
  const params: Record<string, string> = {};
  const texts: Record<string, string> = {};
  for (const { key, value } of item.params) {
    if (isDateKey(key) && /^\d{4}-\d{2}-\d{2}$/.test(value)) {
      params[key] = formatIsoDate(value, locale);
    } else if (key === "source") {
      params[key] = t(`anchor.${value}` as "anchor.none", { defaultValue: value });
    } else if (key === "kind") {
      params[key] = t(`breakKind.${value}` as "breakKind.other", { defaultValue: value });
    } else if (key === "reason") {
      params[key] = t(`reject.${value}` as "reject.starts_after_end", { defaultValue: value });
    } else if (key === "signal") {
      params[key] = t(`signal.${value}` as "signal.dates", { defaultValue: value });
    } else if (/^\d+$/.test(value)) {
      params[key] = value;
    } else {
      texts[key] = value;
    }
  }
  return { params, texts };
}

/** One evidence item as a sentence; a code this version doesn't know gets a neutral line. */
export function evidenceText(item: EvidenceItem, t: TFunction<"calendar">, locale: string): string {
  if (!isKnownCode(item.code)) return t("evidence.unknown");
  const { params, texts } = evidenceParams(item, t, locale);
  const { key, missing } = templateFor(item.code, new Set(item.params.map((p) => p.key)));
  for (const name of missing) params[name] = "…";
  return translateWithText(t, `evidence.${key}`, params, texts);
}
