import { describe, expect, it } from "vitest";
import { ev } from "@/api/mock/calendar";
import i18n, { resources } from "@/i18n";
import { evidenceText, isDateKey, isKnownCode, templateFor } from "./evidence";

const en = i18n.getFixedT("en", "calendar");
const zh = i18n.getFixedT("zh-CN", "calendar");

describe("evidence items", () => {
  it("has the same codes and variants in English and Chinese (CAL-53, UI half)", () => {
    // Every EvidenceCode having English text is checked at compile time (evidence.ts).
    const codes = (lng: string) =>
      Object.keys(
        (resources[lng] as Record<string, { evidence: object }>).calendar?.evidence ?? {},
      );
    expect(codes("zh-CN").sort()).toEqual(codes("en").sort());
    expect(codes("en")).toContain("dates_may_be_wrong");
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
    expect(
      evidenceText(ev("signal_agrees", { signal: "recent_materials", week: 4 }), en, "en"),
    ).toBe("Agrees with recent materials (week 4)");
    expect(
      evidenceText(
        ev("dates_not_used", {
          source: "lms_term",
          start: "2026-05-04",
          end: "2027-01-31",
          reason: "starts_after_end",
        }),
        en,
        "en",
      ),
    ).toBe("Not used: the Canvas term dates, May 4, 2026 → Jan 31, 2027. Ends before it starts.");
    expect(
      evidenceText(
        ev("in_break", { kind: "reading_week", start: "2026-10-12", end: "2026-10-16" }),
        zh,
        "zh-CN",
      ),
    ).toBe("阅读周：2026年10月12日 – 2026年10月16日");
  });

  it("uses the variant without an absent optional param", () => {
    expect(templateFor("lms_term_dates", new Set(["start"]))).toEqual({
      key: "lms_term_dates_no_end_no_term_name",
      missing: [],
    });
    expect(evidenceText(ev("lms_term_dates", { start: "2026-09-08" }), en, "en")).toBe(
      "The Canvas term start date is used: Sep 8, 2026",
    );
    expect(evidenceText(ev("student_dates", { end: "2026-12-08" }), en, "en")).toBe(
      "Your last day of classes, Dec 8, 2026, is used",
    );
    // No variant for this combination: the gap shows as "…", never as "{{week}}".
    expect(evidenceText(ev("notes_ahead"), en, "en")).toBe("Week … materials are already posted");
  });

  it("shows text written elsewhere as it is, never interpolated", () => {
    const item = ev("week_from_module_unlock", {
      title: "<b>Week 4</b> {{week}} {{product}}",
      date: "2026-09-26",
      week: 4,
    });
    expect(evidenceText(item, en, "en")).toBe(
      "Module “<b>Week 4</b> {{week}} {{product}}” unlocked Sep 26, 2026 → week 4",
    );
  });

  it("gives a neutral line for a code this version doesn't know", () => {
    expect(isKnownCode("from_the_future")).toBe(false);
    expect(isKnownCode("unknown")).toBe(false);
    expect(evidenceText({ code: "from_the_future", params: [] }, en, "en")).toBe(
      "Another clue this version of PageLamp can't show",
    );
  });

  it("recognises date params by key", () => {
    expect(["date", "since", "until", "monday", "starts_on"].every(isDateKey)).toBe(true);
    expect(isDateKey("week")).toBe(false);
  });
});
