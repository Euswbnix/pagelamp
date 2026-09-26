import {
  CalendarClock,
  CalendarDays,
  ChevronRight,
  EyeOff,
  FileText,
  type LucideIcon,
  TriangleAlert,
} from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import type { CourseSummary, SourceErrorKind } from "@/api/types";
import { PolicyBadge } from "@/components/common/PolicyBadge";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { WeekLabel } from "@/components/common/WeekLabel";
import { Badge } from "@/components/ui/badge";
import { paths } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { deadlineTime } from "./lib/thisWeek";

interface CourseCardProps {
  summary: CourseSummary;
  /** Why this course's source last failed to sync, if it did. */
  sourceError: SourceErrorKind | null;
}

/**
 * One course: where it is this week, what's next, its AI policy and how much of it the AI
 * app can read. The title link is stretched over the whole card ("stretched link"), so the
 * card is clickable while the page still has exactly one link per course and nothing
 * interactive nested inside it.
 */
export function CourseCard({ summary, sourceError }: CourseCardProps) {
  const { t } = useTranslation("courses");
  const { t: tc } = useTranslation();
  const { course, timeline, counts, next_deadline: next } = summary;
  const nextWhen = next ? deadlineTime(next) : null;
  const weekUnknown = timeline.current_week == null && !timeline.outside_term;

  return (
    <article
      className={cn(
        "relative flex h-full flex-col gap-3 rounded-xl bg-card p-4 text-sm ring-1 ring-foreground/10 transition-colors",
        "hover:bg-muted/40 has-[a:focus-visible]:ring-3 has-[a:focus-visible]:ring-ring/50",
        course.hidden && "bg-muted/30",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <h3 className="min-w-0">
          <Link
            to={paths.course(course.id)}
            className="outline-none after:absolute after:inset-0 after:rounded-xl"
          >
            <span className="block font-heading text-base font-semibold tracking-tight">
              {course.code ?? course.name}
            </span>{" "}
            {course.code ? (
              <span className="block text-muted-foreground">{course.name}</span>
            ) : null}
          </Link>
        </h3>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <PolicyBadge policy={course.ai_policy} />
          {course.ai_policy === "unknown" ? (
            <span className="inline-flex items-center gap-0.5 text-xs text-muted-foreground">
              {t("card.setPolicy")}
              <ChevronRight className="size-3" aria-hidden />
            </span>
          ) : null}
        </div>
      </div>

      <ul className="space-y-1.5">
        <Fact icon={CalendarDays}>
          <WeekLabel timeline={timeline} />
          {weekUnknown ? (
            <span className="block text-xs text-muted-foreground">{t("card.setTerm")}</span>
          ) : null}
        </Fact>
        <Fact icon={CalendarClock}>
          {next && nextWhen ? (
            <SentenceWithTime
              text={t("card.next", { title: next.title, when: WHEN })}
              iso={nextWhen}
            />
          ) : (
            <span className="text-muted-foreground">{t("card.noDeadlines")}</span>
          )}
        </Fact>
        <Fact icon={FileText}>
          {counts.materials > 0 ? (
            t("card.readable", { indexed: counts.indexed_materials, count: counts.materials })
          ) : (
            <span className="text-muted-foreground">{t("card.noFiles")}</span>
          )}
        </Fact>
      </ul>

      <div className="mt-auto flex flex-wrap items-center gap-x-3 gap-y-1 border-t pt-3 text-xs text-muted-foreground">
        <span>
          {summary.last_synced_at ? (
            <SentenceWithTime
              text={t("card.synced", { source: summary.source_label, when: WHEN })}
              iso={summary.last_synced_at}
            />
          ) : (
            t("card.notSynced", { source: summary.source_label })
          )}
        </span>
        {sourceError ? (
          <span className="inline-flex items-center gap-1 text-foreground">
            <TriangleAlert className="size-3.5 text-warning" aria-hidden />
            {tc(`sourceError.${sourceError}`)}
          </span>
        ) : null}
        {course.hidden ? (
          <Badge variant="outline">
            <EyeOff aria-hidden />
            {t("card.hidden")}
          </Badge>
        ) : null}
      </div>
    </article>
  );
}

function Fact({ icon: Icon, children }: { icon: LucideIcon; children: ReactNode }) {
  return (
    <li className="flex items-start gap-2">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0">{children}</div>
    </li>
  );
}
