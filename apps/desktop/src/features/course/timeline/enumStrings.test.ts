// Every value of the enums the timeline names has words in both languages. The evidence codes
// are checked when compiling (evidence.ts); these are looked up by a key built at run time, so a
// value added in Rust would show the bare key.

import { expect, it } from "vitest";
import type { CalendarOrigin, TermAnchorSource } from "@/api/types";
import en from "@/i18n/locales/en/calendar.json";
import zh from "@/i18n/locales/zh-CN/calendar.json";

// A Record over the union: a value added to the enum fails to compile until it is listed here.
const ANCHORS: Record<TermAnchorSource, true> = {
  student_confirmed: true,
  lms_course_dates: true,
  lms_term: true,
  folder_config: true,
  institution_calendar: true,
  same_session: true,
  published_week_labels: true,
  none: true,
};
const ORIGINS: Record<CalendarOrigin, true> = {
  scan: true,
  ai: true,
  ai_app: true,
  restored: true,
  user: true,
  legacy: true,
};

it.each([
  ["en", en],
  ["zh-CN", zh],
] as const)("names every source of a course's dates in %s", (_, strings) => {
  for (const anchor of Object.keys(ANCHORS)) {
    expect(strings.anchor, anchor).toHaveProperty(anchor);
    expect(strings.anchorTitle, anchor).toHaveProperty(anchor);
  }
  for (const origin of Object.keys(ORIGINS)) {
    expect(strings.origin, origin).toHaveProperty(origin);
  }
});
