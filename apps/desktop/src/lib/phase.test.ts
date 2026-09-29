import { describe, expect, it } from "vitest";
import { type CalendarFields, calendarFields, resolution } from "@/api/mock/calendar";
import type { CourseTimeline } from "@/api/types";
import { describePhase, formatShortDate } from "./phase";

function timeline(
  fields: Partial<CalendarFields> & Pick<CalendarFields, "phase">,
  week: number | null = null,
): CourseTimeline {
  return {
    as_of: "2026-09-28",
    confidence: "medium",
    current_module_ids: [],
    current_week: week,
    evidence: [],
    outside_term: false,
    ...calendarFields(fields),
  };
}

describe("describePhase", () => {
  it("names a teaching week with its confidence", () => {
    expect(describePhase(timeline({ phase: "teaching", default_week: 4 }, 4))).toEqual({
      kind: "teaching",
      week: 4,
      confidence: "medium",
    });
  });

  it("names numbered and unnumbered breaks", () => {
    expect(
      describePhase(
        timeline({ phase: "break", current_break_kind: "reading_week", default_week: 7 }, 7),
      ),
    ).toEqual({ kind: "breakNumbered", week: 7, breakKind: "reading_week" });
    expect(
      describePhase(
        timeline({ phase: "break", current_break_kind: "reading_week", break_after_week: 6 }),
      ),
    ).toEqual({ kind: "breakAfter", week: 6, breakKind: "reading_week" });
    expect(describePhase(timeline({ phase: "break" }))).toEqual({
      kind: "break",
      breakKind: "other",
    });
  });

  it("names the exam period, with the last teaching week when known", () => {
    expect(describePhase(timeline({ phase: "exam_period", last_teaching_week: 12 }))).toEqual({
      kind: "examsAfter",
      week: 12,
    });
    expect(describePhase(timeline({ phase: "exam_period" }))).toEqual({ kind: "exams" });
  });

  it("gives the first day of classes before a course starts", () => {
    const term = resolution({
      teaching: [{ first_class: "2027-01-11", last_class: null, first_week_number: 1 }],
    });
    expect(describePhase(timeline({ phase: "not_started", term }))).toEqual({
      kind: "startsOn",
      date: "2027-01-11",
    });
    expect(describePhase(timeline({ phase: "not_started" }))).toEqual({ kind: "notStarted" });
  });

  it("never shows a week for an ended course", () => {
    expect(describePhase(timeline({ phase: "ended" }, 22))).toEqual({ kind: "ended" });
  });

  it("keeps a week from the older signals when the phase is unknown", () => {
    expect(describePhase(timeline({ phase: "unknown" }, 3))).toEqual({
      kind: "teaching",
      week: 3,
      confidence: "medium",
    });
    expect(describePhase(timeline({ phase: "unknown" }))).toEqual({ kind: "unknown" });
  });
});

describe("formatShortDate", () => {
  it("leaves out the year when it is this year", () => {
    expect(formatShortDate("2026-10-12", "en", "2026-09-28")).toBe("Oct 12");
    expect(formatShortDate("2027-01-11", "en", "2026-09-28")).toBe("Jan 11, 2027");
    expect(formatShortDate("2027-01-11", "zh-CN", "2026-09-28")).toBe("2027年1月11日");
  });
});
