// Pure helpers for the read-only study-plan card. Dates in a plan are "YYYY-MM-DD" calendar
// dates, so they compare correctly as strings.

import type { IsoDate, StoredStudyPlan, StudyPlanItem } from "@/api/types";
import { todayIso } from "@/lib/format";

const DAY = 24 * 60 * 60 * 1000;

/** A plan older than this is probably out of date, even if its horizon has not ended. */
export const PLAN_STALE_AFTER_DAYS = 7;

/** The collapsed plan shows today plus the next 2 days. */
export const FOCUS_DAYS = 3;

/** True when the plan's horizon has ended or it was made more than 7 days ago. */
export function isPlanStale(stored: StoredStudyPlan, now: Date = new Date()): boolean {
  if (stored.plan.horizon_end < todayIso(now)) return true;
  const created = Date.parse(stored.created_at);
  return !Number.isNaN(created) && now.getTime() - created > PLAN_STALE_AFTER_DAYS * DAY;
}

/** "2026-09-30" + 2 → "2026-10-02" (local calendar arithmetic, no timezone shift). */
export function addDays(date: IsoDate, days: number): IsoDate {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  return todayIso(new Date(y, m - 1, d + days));
}

export interface PlanEntry {
  item: StudyPlanItem;
  /**
   * Position in the saved plan. Plan items have no id, and a saved plan never changes, so
   * this is a stable React key.
   */
  position: number;
}

export interface PlanDay {
  date: IsoDate;
  entries: PlanEntry[];
}

/** Items grouped by date, earliest first; items keep their plan order within a day. */
export function groupPlanByDate(items: StudyPlanItem[]): PlanDay[] {
  const byDate = new Map<IsoDate, PlanEntry[]>();
  items.forEach((item, position) => {
    const list = byDate.get(item.date) ?? [];
    list.push({ item, position });
    byDate.set(item.date, list);
  });
  return [...byDate.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([date, entries]) => ({ date, entries }));
}

/** Whether `date` falls in the collapsed view: today or one of the next FOCUS_DAYS - 1 days. */
export function isInFocus(date: IsoDate, today: IsoDate): boolean {
  return date >= today && date <= addDays(today, FOCUS_DAYS - 1);
}
