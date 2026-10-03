import type { Ref } from "react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { CourseLifecycle, CourseTimeline } from "@/api/types";
import { WeekLabel } from "@/components/common/WeekLabel";
import { Separator } from "@/components/ui/separator";
import { describePhase, outsideWeekViews } from "@/lib/phase";
import { CourseStatus } from "../lifecycle/CourseStatus";
import { DatesNotUsed } from "./DatesNotUsed";
import { DatesUsed } from "./DatesUsed";
import { EvidenceList } from "./EvidenceList";

/**
 * "Where this course is" (calendar design §7.13): the phase and week, how sure we are, the
 * dates used and their source, the dates not used and why, the evidence, and the course's
 * status (Current, Past, …) with "I'm still taking this".
 */
export function WhereCourseIsCard({
  courseId,
  timeline,
  lifecycle,
  headingRef,
}: {
  courseId: string;
  timeline: CourseTimeline;
  lifecycle: CourseLifecycle;
  /** The card's heading, focusable from script (after the check-dates prompt goes away). */
  headingRef?: Ref<HTMLHeadingElement>;
}) {
  const { t } = useTranslation("calendar");
  const headingId = useId();
  // A course that is over, inactive or not started: its lifecycle says where it stands (the
  // status section below explains why), so no word on how sure a week is and no notes week.
  const outside = outsideWeekViews(timeline, lifecycle);
  // A week (teaching, or from the older signals) is as sure as the week; otherwise the phase.
  const confidence =
    describePhase(timeline, lifecycle).kind === "teaching"
      ? timeline.confidence
      : timeline.phase_confidence;
  // How far the professor's materials have got matters only while the course is running.
  const running = !outside && ["teaching", "break", "unknown"].includes(timeline.phase);
  const notesWeek = running ? (timeline.notes_week ?? null) : null;

  return (
    <section aria-labelledby={headingId} className="pl-callout space-y-5 p-5 text-card-foreground">
      <div className="space-y-1">
        <h2
          id={headingId}
          ref={headingRef}
          tabIndex={-1}
          className="font-heading text-base font-semibold tracking-tight outline-none"
        >
          {t("where.title")}
        </h2>
        <p className="text-lg">
          <WeekLabel timeline={timeline} lifecycle={lifecycle} />
        </p>
        {outside ? null : (
          <p className="text-sm text-muted-foreground">{t(`where.confidence.${confidence}`)}</p>
        )}
        {notesWeek !== null ? (
          <p className="text-sm text-muted-foreground">
            {t("where.notesWeek", { week: notesWeek })}
          </p>
        ) : null}
      </div>

      <EvidenceList
        items={timeline.evidence_items}
        fallback={timeline.evidence}
        title={t("where.evidence")}
        emptyText={t("where.evidenceEmpty")}
      />

      <DatesUsed term={timeline.term} />
      <DatesNotUsed items={timeline.term.not_used} />

      <Separator />

      <CourseStatus courseId={courseId} lifecycle={lifecycle} />
    </section>
  );
}
