import { FileWarning } from "lucide-react";
import { useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useCourseCalendar } from "@/api/proposalQueries";
import type { AcceptedCalendar, Course, CourseTimeline } from "@/api/types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { formatIsoDate } from "@/lib/format";
import { ProposalCard } from "./ProposalCard";
import { READ_SYLLABUS_HEADING } from "./ReadSyllabus";

/**
 * On the Timeline tab (F3): the stale banner for the calendar in force, and every pending
 * proposal (scan, AI, the student's AI app). Renders nothing when there is neither.
 */
export function ProposalsSection({
  course,
  timeline,
  onEmptied,
}: {
  course: Course;
  timeline: CourseTimeline;
  /** Where focus goes when the last proposal is gone (the section goes with it). */
  onEmptied: () => void;
}) {
  const { t } = useTranslation("proposals");
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const view = useCourseCalendar(course.id);
  const data = view.data;
  if (!data) return null;
  const stale = data.accepted?.stale ? data.accepted : null;
  if (!stale && data.proposals.length === 0) return null;
  // A proposal accepted or dismissed goes away; continue from this section's heading.
  // …or from the card below when the section went too.
  const focusHeading = () =>
    requestAnimationFrame(() =>
      headingRef.current?.isConnected ? headingRef.current.focus() : onEmptied(),
    );

  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <div className="space-y-1">
        <h2
          id={headingId}
          ref={headingRef}
          tabIndex={-1}
          className="font-heading text-base font-semibold tracking-tight outline-none"
        >
          {t("section.title")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("section.description")}</p>
      </div>
      {stale ? <StaleBanner accepted={stale} /> : null}
      {data.proposals.map((proposal) => (
        <ProposalCard
          key={proposal.id}
          proposal={proposal}
          course={course}
          timeline={timeline}
          onGone={focusHeading}
        />
      ))}
    </section>
  );
}

/** "The syllabus changed on … — re-read?" (design §7.8): the calendar stays in force. */
function StaleBanner({ accepted }: { accepted: AcceptedCalendar }) {
  const { t, i18n } = useTranslation("proposals");
  const list = new Intl.ListFormat(i18n.language, { type: "conjunction" });
  return (
    <Alert role="status">
      <FileWarning aria-hidden />
      <AlertTitle>
        {t("stale.title", {
          date: accepted.stale_since ? formatIsoDate(accepted.stale_since, i18n.language) : "…",
        })}
      </AlertTitle>
      <AlertDescription>
        {t("stale.body", { materials: list.format(accepted.changed_materials) })}
      </AlertDescription>
      <div className="col-start-2 mt-2">
        <Button type="button" size="sm" variant="outline" onClick={goToReading}>
          {t("stale.reread")}
        </Button>
      </div>
    </Alert>
  );
}

/** To the scan and "Read with AI", further down the Timeline tab. */
function goToReading() {
  const heading = document.getElementById(READ_SYLLABUS_HEADING);
  heading?.scrollIntoView({ block: "center" });
  heading?.focus({ preventScroll: true });
}
