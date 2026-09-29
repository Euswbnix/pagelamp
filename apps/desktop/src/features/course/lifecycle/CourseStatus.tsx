import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { CourseLifecycle } from "@/api/provisional/courseCalendar";
import { formatIsoDate } from "@/lib/format";
import { EvidenceList } from "../timeline/EvidenceList";
import { KeepCurrentControl } from "./KeepCurrentControl";

/**
 * "Course status": the lifecycle the facade computed (Current, Finishing, Ended, …), what it
 * means for the course, the evidence, and "I'm still taking this" (calendar design §8.1).
 */
export function CourseStatus({
  courseId,
  lifecycle,
}: {
  courseId: string;
  lifecycle: CourseLifecycle;
}) {
  const { t, i18n } = useTranslation("calendar");
  const headingId = useId();
  const date = (iso: string) => formatIsoDate(iso, i18n.language);
  const kept = !!lifecycle.kept_current_until;
  const evidence = kept
    ? lifecycle.evidence_items.filter((item) => item.code !== "kept_current_until")
    : lifecycle.evidence_items;
  const facts = [
    lifecycle.since ? t("status.since", { date: date(lifecycle.since) }) : null,
    lifecycle.last_activity
      ? t("status.lastActivity", { date: date(lifecycle.last_activity) })
      : null,
    lifecycle.next_event ? t("status.nextEvent", { date: date(lifecycle.next_event) }) : null,
  ].filter((fact): fact is string => fact !== null);

  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div className="space-y-1">
        <h3 id={headingId} className="text-sm font-medium">
          {t("status.title")}
        </h3>
        <p className="text-sm">
          <span className="font-medium">{t(`status.state.${lifecycle.state}`)}</span>
          {facts.length > 0 ? (
            <span className="text-muted-foreground"> · {facts.join(" · ")}</span>
          ) : null}
        </p>
        {/* When the student kept the course current, the control below says so instead. */}
        {kept ? null : (
          <p className="text-sm text-muted-foreground">{t(`status.hint.${lifecycle.state}`)}</p>
        )}
      </div>
      {evidence.length > 0 ? (
        <EvidenceList items={evidence} fallback={[]} title={t("where.evidence")} />
      ) : null}
      <KeepCurrentControl courseId={courseId} lifecycle={lifecycle} />
    </section>
  );
}
