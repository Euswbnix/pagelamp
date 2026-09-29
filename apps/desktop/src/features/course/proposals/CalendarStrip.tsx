import { useTranslation } from "react-i18next";
import type { CourseCalendar } from "@/api/types";
import { cn } from "@/lib/utils";

const DAY = 86_400_000;

function dayOf(iso: string): number {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return Date.UTC(y, m - 1, d) / DAY;
}

/**
 * A horizontal picture of a proposed calendar: teaching, breaks, the exam period and today.
 * Decorative (aria-hidden): the list of dates beside it says the same in words.
 */
export function CalendarStrip({ calendar, today }: { calendar: CourseCalendar; today: string }) {
  const { t } = useTranslation("proposals");
  const starts = calendar.segments.map((s) => s.first_class);
  const ends = [
    ...calendar.segments.map((s) => s.last_class ?? s.first_class),
    ...(calendar.exam_period ? [calendar.exam_period.end] : []),
  ];
  if (starts.length === 0) return null;
  const from = Math.min(...starts.map(dayOf));
  const to = Math.max(...ends.map(dayOf), from + 7);
  const pos = (iso: string) => ((dayOf(iso) - from) / (to - from)) * 100;
  const span = (start: string, end: string) => ({
    left: `${Math.max(0, pos(start))}%`,
    width: `${Math.max(0.8, pos(end) - pos(start))}%`,
  });
  const todayPos = pos(today);

  return (
    <div aria-hidden title={t("card.stripLabel")} className="relative h-6 rounded-md bg-muted">
      {calendar.segments.map((s) => (
        <div
          key={s.first_class}
          className="absolute inset-y-1 rounded-sm bg-primary/70"
          style={span(s.first_class, s.last_class ?? s.first_class)}
        />
      ))}
      {calendar.breaks.map((b) => (
        <div
          key={b.span.start}
          className="absolute inset-y-0.5 rounded-sm bg-background ring-1 ring-foreground/20"
          style={span(b.span.start, b.span.end)}
        />
      ))}
      {calendar.exam_period ? (
        <div
          className="absolute inset-y-1 rounded-sm bg-warning/60"
          style={span(calendar.exam_period.start, calendar.exam_period.end)}
        />
      ) : null}
      <div
        className={cn(
          "absolute -inset-y-1 w-0.5 bg-foreground",
          (todayPos < 0 || todayPos > 100) && "hidden",
        )}
        style={{ left: `${todayPos}%` }}
      />
    </div>
  );
}
