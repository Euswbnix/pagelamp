import { Clock } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCourses, useStudyPlan } from "@/api/queries";
import type { StoredStudyPlan } from "@/api/types";
import { ErrorState } from "@/components/common/ErrorState";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { formatIsoDate } from "@/lib/format";
import { isPlanStale } from "./lib/plan";
import { Section } from "./parts/Section";
import { PlanSkeleton } from "./Skeletons";
import { StudyPlanBody } from "./StudyPlanBody";
import { StudyPlanEmpty } from "./StudyPlanEmpty";

/**
 * The latest study plan the student's AI app saved over MCP (`save_study_plan`). Read-only
 * in v0.1: to change it, the student asks their AI app.
 */
export function StudyPlanCard() {
  const { t } = useTranslation("courses");
  const plan = useStudyPlan();
  // For showing course codes next to plan items; the list below the card loads it anyway.
  const courses = useCourses();

  let description: ReactNode = null;
  let body: ReactNode;
  if (plan.isPending) {
    body = <PlanSkeleton />;
  } else if (plan.isError) {
    body = (
      <ErrorState
        error={plan.error}
        title={t("plan.errorTitle")}
        onRetry={() => void plan.refetch()}
      />
    );
  } else if (!plan.data) {
    body = <StudyPlanEmpty />;
  } else {
    description = <PlanMeta stored={plan.data} />;
    body = <StudyPlanBody plan={plan.data.plan} courses={courses.data} />;
  }

  return (
    <Section title={t("plan.title")} description={description}>
      {body}
    </Section>
  );
}

/** "Made by your AI app 2 days ago · covers …" and, if needed, the out-of-date warning. */
function PlanMeta({ stored }: { stored: StoredStudyPlan }) {
  const { t, i18n } = useTranslation("courses");
  const { horizon_start: start, horizon_end: end } = stored.plan;
  return (
    <div className="space-y-1">
      <p>
        <SentenceWithTime
          iso={stored.created_at}
          text={t("plan.madeBy", {
            when: WHEN,
            start: formatIsoDate(start, i18n.language),
            end: formatIsoDate(end, i18n.language),
          })}
        />
      </p>
      {isPlanStale(stored) ? (
        <p className="flex items-center gap-1.5 text-foreground">
          <Clock className="size-4 shrink-0 text-warning" aria-hidden />
          {t("plan.stale")}
        </p>
      ) : null}
    </div>
  );
}
