// Date/time formatting with the platform's Intl APIs, in the UI language.
// All functions take `now` so they are deterministic in tests.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "5 minutes ago", "in 2 days", "yesterday" — for sync freshness and deadlines. */
export function formatRelative(iso: string, locale: string, now: Date = new Date()): string {
  const diff = Date.parse(iso) - now.getTime();
  const abs = Math.abs(diff);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (abs < MINUTE) return rtf.format(0, "second");
  if (abs < HOUR) return rtf.format(Math.round(diff / MINUTE), "minute");
  if (abs < DAY) return rtf.format(Math.round(diff / HOUR), "hour");
  return rtf.format(calendarDayDiff(iso, now), "day");
}

/** Whole calendar days between `now` and `iso` in local time (tomorrow = 1). */
export function calendarDayDiff(iso: string, now: Date = new Date()): number {
  const a = new Date(iso);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  return Math.round((startOf(a) - startOf(now)) / DAY);
}

/** "Fri, Oct 3, 11:59 PM" / "10月3日周五 23:59". */
export function formatDateTime(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(iso));
}

/** "Fri, Oct 3" / "10月3日周五". */
export function formatDay(iso: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(new Date(iso));
}

/** Calendar date "YYYY-MM-DD" → "Oct 3, 2026" (no timezone shift). */
export function formatIsoDate(date: string, locale: string): string {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(new Date(y, m - 1, d));
}

/** Calendar date "YYYY-MM-DD" → "Fri, Oct 3" (no timezone shift). */
export function formatIsoDay(date: string, locale: string): string {
  const [y, m, d] = date.split("-").map(Number) as [number, number, number];
  return new Intl.DateTimeFormat(locale, {
    weekday: "short",
    month: "short",
    day: "numeric",
  }).format(new Date(y, m - 1, d));
}

/** Today's local date as "YYYY-MM-DD". */
export function todayIso(now: Date = new Date()): string {
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${m}-${d}`;
}
