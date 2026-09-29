// "What changes" lines of a calendar proposal (CalendarChange: code + params, like evidence).

import type { TFunction } from "i18next";
import type { CalendarChange, ChangeCode } from "@/api/types";
import en from "@/i18n/locales/en/proposals.json";
import { formatIsoDate } from "@/lib/format";
import { translateWithText } from "../timeline/evidence";

// Compile-time check: every ChangeCode has English text (the i18n test keeps zh-CN equal).
en.card.change satisfies Record<ChangeCode | "unknown", string>;

const ISO = /^\d{4}-\d{2}-\d{2}$/;

export function changeText(
  change: CalendarChange,
  t: TFunction<"proposals">,
  tcal: TFunction<"calendar">,
  locale: string,
): string {
  if (!Object.hasOwn(en.card.change, change.code) || change.code === ("unknown" as string)) {
    return t("card.change.unknown");
  }
  const params: Record<string, string> = {};
  for (const { key, value } of change.params) {
    if (ISO.test(value)) params[key] = formatIsoDate(value, locale);
    else if (key === "kind")
      params[key] = tcal(`breakKind.${value}` as "breakKind.other", { defaultValue: value });
    else if (key === "from_phase" || key === "to_phase")
      params[key] = t(`card.phase.${value}` as "card.phase.unknown", { defaultValue: value });
    else if (change.code === "week_today_changes" && /^\d+$/.test(value))
      params[key] = tcal("phase.teaching", { week: value });
    else params[key] = value || "—";
  }
  const present = new Set(change.params.map((p) => p.key));
  // Optional params: an unknown week reads "Week unknown"; a first date has no "from".
  if (change.code === "week_today_changes") {
    for (const name of ["from", "to"]) if (!present.has(name)) params[name] = tcal("phase.unknown");
  }
  const variant = `${change.code}_no_from`;
  const key =
    !present.has("from") && Object.hasOwn(en.card.change, variant) ? variant : change.code;
  return translateWithText(t, `card.change.${key}`, params);
}
