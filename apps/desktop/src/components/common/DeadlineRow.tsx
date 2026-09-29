import {
  CalendarClock,
  ClipboardList,
  Clock,
  GraduationCap,
  NotebookPen,
  Presentation,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Deadline, EventKind } from "@/api/types";
import { calendarDayDiff, formatDateTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { ExternalLink } from "./ExternalLink";
import { RelativeTime } from "./RelativeTime";

const ICON: Record<EventKind, typeof CalendarClock> = {
  assignment_due: NotebookPen,
  quiz_due: ClipboardList,
  exam: GraduationCap,
  class_event: Presentation,
  planner_item: ClipboardList,
  other: CalendarClock,
};

interface DeadlineRowProps {
  deadline: Deadline;
  /** Prefix the course code (for cross-course lists). */
  showCourse?: boolean;
  now?: Date;
}

/** One deadline/event: icon, kind, title (links to the LMS when known), due time. */
export function DeadlineRow({ deadline, showCourse = false, now }: DeadlineRowProps) {
  const { t, i18n } = useTranslation();
  const when = deadline.due_at ?? deadline.starts_at ?? null;
  const Icon = ICON[deadline.kind];
  const soon =
    when !== null &&
    calendarDayDiff(when, now) <= 1 &&
    Date.parse(when) >= (now ?? new Date()).getTime();
  const past = when !== null && Date.parse(when) < (now ?? new Date()).getTime();
  return (
    <li className={cn("flex items-start gap-3 py-2.5", past && "opacity-60")}>
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-baseline gap-x-2">
          {showCourse && deadline.course_code ? (
            <span className="text-xs font-medium text-muted-foreground">
              {deadline.course_code}
            </span>
          ) : null}
          {deadline.url ? (
            <ExternalLink
              href={deadline.url}
              className="font-medium underline decoration-muted-foreground/40 underline-offset-4"
              showIcon={false}
            >
              {deadline.title}
            </ExternalLink>
          ) : (
            <span className="font-medium">{deadline.title}</span>
          )}
        </div>
        <div className="text-xs text-muted-foreground">
          {t(`eventKind.${deadline.kind}`)}
          {when ? ` · ${formatDateTime(when, i18n.language)}` : null}
        </div>
      </div>
      {when ? (
        // Soon: ink words with an amber clock (status colours only on glyphs, §4.2).
        <span
          className={cn(
            "inline-flex shrink-0 items-center gap-1 text-xs",
            soon ? "font-medium text-foreground" : "text-muted-foreground",
          )}
        >
          {soon ? <Clock className="size-3 text-warning" aria-hidden /> : null}
          <RelativeTime iso={when} now={now} />
        </span>
      ) : null}
    </li>
  );
}
