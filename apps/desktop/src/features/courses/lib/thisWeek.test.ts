import { describe, expect, it } from "vitest";
import type { Deadline, EventKind } from "@/api/types";
import { countDue, groupByDay } from "./thisWeek";

// Fixed local clock: Friday 2026-09-25, 10:00.
const NOW = new Date(2026, 8, 25, 10, 0);

function event(title: string, kind: EventKind, day: number, hour: number): Deadline {
  const when = new Date(2026, 8, 25 + day, hour, 0).toISOString();
  const isDue = kind !== "class_event" && kind !== "exam";
  return {
    id: title,
    source_id: "ical:test",
    title,
    kind,
    due_at: isDue ? when : null,
    starts_at: isDue ? null : when,
    updated_at: NOW.toISOString(),
  };
}

describe("groupByDay", () => {
  it("groups by calendar day, soonest first, with classes kept apart", () => {
    const groups = groupByDay(
      [
        event("Quiz", "quiz_due", 3, 9),
        event("Lecture", "class_event", 1, 10),
        event("Essay", "assignment_due", 1, 23),
        event("Lab", "assignment_due", 1, 8),
        event("Late tonight", "assignment_due", 0, 23),
      ],
      NOW,
    );
    expect(groups.map((g) => g.dayDiff)).toEqual([0, 1, 3]);
    expect(groups[1]?.deadlines.map((d) => d.title)).toEqual(["Lab", "Essay"]);
    expect(groups[1]?.classes.map((d) => d.title)).toEqual(["Lecture"]);
    expect(groups[0]?.deadlines.map((d) => d.title)).toEqual(["Late tonight"]);
  });

  it("drops events without any time", () => {
    const untimed: Deadline = { ...event("No date", "other", 1, 9), due_at: null, starts_at: null };
    expect(groupByDay([untimed], NOW)).toEqual([]);
  });
});

describe("countDue", () => {
  it("counts deadlines and exams but not class meetings or untimed events", () => {
    const untimed: Deadline = { ...event("No date", "other", 1, 9), due_at: null, starts_at: null };
    const groups = groupByDay(
      [
        event("Lecture", "class_event", 1, 10),
        event("Essay", "assignment_due", 1, 23),
        event("Midterm", "exam", 2, 18),
        untimed,
      ],
      NOW,
    );
    expect(countDue(groups)).toBe(2);
  });
});
