import { CalendarX } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Course, CourseTimeline } from "@/api/types";
import { WeekLabel } from "@/components/common/WeekLabel";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Separator } from "@/components/ui/separator";
import { TermDatesForm } from "./TermDatesForm";

/** Where the course is now, why we think so (backend evidence, verbatim), and a way to fix it. */
export function TimelineTab({ course, timeline }: { course: Course; timeline: CourseTimeline }) {
  const { t } = useTranslation("course");
  const headingId = useId();
  const evidenceId = useId();
  // The backend's reasons are shown exactly as given; de-duplicated only so keys stay unique.
  const evidence = [...new Set(timeline.evidence)];

  return (
    <div className="max-w-2xl space-y-8">
      <section aria-labelledby={headingId} className="space-y-4">
        <div className="space-y-1">
          <h2 id={headingId} className="font-heading text-base font-semibold tracking-tight">
            {t("timeline.title")}
          </h2>
          <p className="text-lg">
            <WeekLabel timeline={timeline} />
          </p>
          <p className="text-sm text-muted-foreground">
            {t(`timeline.confidence.${timeline.confidence}`)}
          </p>
        </div>

        {timeline.outside_term ? (
          <Alert role="status">
            <CalendarX aria-hidden />
            <AlertDescription>{t("timeline.outsideTerm")}</AlertDescription>
          </Alert>
        ) : null}

        <div className="space-y-2">
          <h3 id={evidenceId} className="text-sm font-medium">
            {t("timeline.evidenceTitle")}
          </h3>
          {evidence.length > 0 ? (
            <ul
              aria-labelledby={evidenceId}
              className="list-disc space-y-1 pl-5 text-sm text-muted-foreground"
            >
              {evidence.map((reason) => (
                <li key={reason} lang="en">
                  {reason}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-sm text-muted-foreground">{t("timeline.evidenceEmpty")}</p>
          )}
        </div>
      </section>

      <Separator />

      <TermDatesForm course={course} />
    </div>
  );
}
