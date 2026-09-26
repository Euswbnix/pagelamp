// Pure helpers for the "This week" strip.

import type { Deadline } from "@/api/types";
import { calendarDayDiff } from "@/lib/format";

export interface DayGroup {
  /** Calendar days from today: 0 = today, 1 = tomorrow. */
  dayDiff: number;
  /** A timestamp on that day, used to format the heading. */
  iso: string;
  /** Things that are due (assignments, quizzes, exams, to-dos). */
  deadlines: Deadline[];
  /** Class meetings: shown, but quieter than deadlines. */
  classes: Deadline[];
}

/** When a deadline is due, or when an event starts. */
export function deadlineTime(deadline: Deadline): string | null {
  return deadline.due_at ?? deadline.starts_at ?? null;
}

/**
 * Groups events by local calendar day, soonest first. Events without a time are dropped, and
 * with `days` set only today and the following `days - 1` calendar days are kept (the backend's
 * window is "now + N×24 h", which would reach into an extra day).
 */
export function groupByDay(items: Deadline[], now: Date = new Date(), days?: number): DayGroup[] {
  const timed = items
    .map((deadline) => ({ deadline, iso: deadlineTime(deadline) }))
    .filter((x): x is { deadline: Deadline; iso: string } => x.iso !== null)
    .sort((a, b) => Date.parse(a.iso) - Date.parse(b.iso));

  const groups = new Map<number, DayGroup>();
  for (const { deadline, iso } of timed) {
    const dayDiff = calendarDayDiff(iso, now);
    if (days !== undefined && (dayDiff < 0 || dayDiff >= days)) continue;
    let group = groups.get(dayDiff);
    if (!group) {
      group = { dayDiff, iso, deadlines: [], classes: [] };
      groups.set(dayDiff, group);
    }
    (deadline.kind === "class_event" ? group.classes : group.deadlines).push(deadline);
  }
  return [...groups.values()].sort((a, b) => a.dayDiff - b.dayDiff);
}

/** Number of real deadlines in the groups (class meetings don't count). */
export function countDue(groups: DayGroup[]): number {
  return groups.reduce((sum, group) => sum + group.deadlines.length, 0);
}
