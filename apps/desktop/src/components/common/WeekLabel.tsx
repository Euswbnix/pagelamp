import { useTranslation } from "react-i18next";
import type { CourseTimeline } from "@/api/types";

/** "Week 4 · high confidence" — or "Week unknown" / "Outside term". */
export function WeekLabel({ timeline }: { timeline: CourseTimeline }) {
  const { t } = useTranslation();
  if (timeline.outside_term) {
    return <span className="text-muted-foreground">{t("week.outsideTerm")}</span>;
  }
  if (timeline.current_week == null) {
    return <span className="text-muted-foreground">{t("week.unknown")}</span>;
  }
  return (
    <span>
      <span className="font-medium">{t("week.current", { week: timeline.current_week })}</span>
      <span className="text-muted-foreground"> · {t(`confidence.${timeline.confidence}`)}</span>
    </span>
  );
}
