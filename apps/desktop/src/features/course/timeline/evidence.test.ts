import { describe, expect, it } from "vitest";
import { ev } from "@/api/mock/calendar";
import i18n, { resources } from "@/i18n";
import { evidenceText, isDateKey, isKnownCode } from "./evidence";

const en = i18n.getFixedT("en", "calendar");
const zh = i18n.getFixedT("zh-CN", "calendar");

describe("evidence items", () => {
  it("has English and Chinese text for every known code (CAL-53, UI half)", () => {
    const calendar = (lng: string) =>
      (resources[lng] as Record<string, { evidence: object }>).calendar?.evidence ?? {};
    const enCodes = Object.keys(calendar("en"));
    const zhCodes = Object.keys(calendar("zh-CN"));
    expect(zhCodes.sort()).toEqual(enCodes.sort());
    expect(enCodes).toContain("dates_may_be_wrong");
    expect(enCodes).toContain("term_looks_like_enrollment_window");
  });

  it("formats dates, enum values and numbers from the params", () => {
    const item = ev("term_looks_like_enrollment_window", {
      term_name: "Fall 2026",
      start: "2026-05-04",
      end: "2027-01-31",
      weeks: 39,
    });
    expect(evidenceText(item, en, "en")).toBe(
      "Canvas term “Fall 2026” runs May 4, 2026 → Jan 31, 2027 (39 weeks): longer than a teaching term, so it isn't used to count weeks",
    );
    expect(evidenceText(ev("signal_agrees", { source: "lms_term", week: 4 }), en, "en")).toBe(
      "Agrees with the Canvas term dates (week 4)",
    );
    expect(
      evidenceText(ev("in_break", { kind: "reading_week", end: "2026-10-16" }), zh, "zh-CN"),
    ).toBe("阅读周，到 2026年10月16日 为止");
  });

  it("shows instructor-written text as it is", () => {
    const item = ev("module_unlocked", {
      title: "<b>Week 4</b> {{week}}",
      date: "2026-09-26",
      week: 4,
    });
    expect(evidenceText(item, en, "en")).toBe(
      "Module “<b>Week 4</b> {{week}}” unlocked Sep 26, 2026 → week 4",
    );
  });

  it("gives a neutral line for a code this version doesn't know", () => {
    expect(isKnownCode("from_the_future")).toBe(false);
    expect(evidenceText(ev("from_the_future", { date: "2026-01-01" }), en, "en")).toBe(
      "Another clue this version of PageLamp can't show",
    );
  });

  it("recognises date params by key", () => {
    expect(["date", "since", "monday", "starts_on"].every(isDateKey)).toBe(true);
    expect(isDateKey("week")).toBe(false);
  });
});
