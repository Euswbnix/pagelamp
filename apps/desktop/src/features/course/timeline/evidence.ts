// Evidence items (code + params) → translated sentences (calendar design §3.3; CAL-53).
// Params follow backend-2's key conventions: dates are ISO dates, `source`/`kind`/`reason` are
// enum values we translate, numbers stay numbers, anything else (a module title, a term name)
// is instructor-written text shown as plain text. That text never goes through i18next's
// interpolation (a title like "{{week}}" would be filled in there): it is swapped in afterwards.

import type { TFunction } from "i18next";
import type { EvidenceItem } from "@/api/provisional/courseCalendar";
import en from "@/i18n/locales/en/calendar.json";
import { formatIsoDate } from "@/lib/format";

/** Codes with a translation. Provisional until the facade exports its EvidenceCode enum. */
export type EvidenceCode = Exclude<keyof typeof en.evidence, "unknown">;

const DATE_KEYS = new Set(["date", "start", "end", "since", "until", "monday"]);

export function isDateKey(key: string): boolean {
  return DATE_KEYS.has(key) || key.endsWith("_on");
}

export function isKnownCode(code: string): code is EvidenceCode {
  return code !== "unknown" && Object.hasOwn(en.evidence, code);
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
  t: TFunction<"calendar">,
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
  return translateWithText(t, `evidence.${item.code}`, params, texts);
}
