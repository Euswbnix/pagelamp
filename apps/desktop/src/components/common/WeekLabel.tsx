import { useTranslation } from "react-i18next";
import type { CourseTimeline } from "@/api/types";
import { describePhase, formatShortDate } from "@/lib/phase";
import { useToday } from "@/lib/useToday";

/**
 * Where the course is: "Week 4 · medium confidence", "Reading week (after week 7)", "Exams",
 * "Ended", "Starts Jan 11" or "Week unknown" (calendar design §7.13).
 */
export function WeekLabel({ timeline }: { timeline: CourseTimeline }) {
  const { t, i18n } = useTranslation("calendar");
  const { t: tc } = useTranslation();
  const today = useToday();
  const label = describePhase(timeline);
  const muted = (text: string) => <span className="text-muted-foreground">{text}</span>;
  const strong = (text: string) => <span className="font-medium">{text}</span>;

  switch (label.kind) {
    case "teaching":
      return (
        <span>
          {strong(t("phase.teaching", { week: label.week }))}
          <span className="text-muted-foreground"> · {tc(`confidence.${label.confidence}`)}</span>
        </span>
      );
    case "breakNumbered":
      return strong(
        t("phase.breakNumbered", { week: label.week, kind: t(`breakKind.${label.breakKind}`) }),
      );
    case "breakAfter":
      return strong(
        t("phase.breakAfter", { week: label.week, kind: t(`breakKind.${label.breakKind}`) }),
      );
    case "break":
      return strong(t("phase.break", { kind: t(`breakKind.${label.breakKind}`) }));
    case "examsAfter":
      return strong(t("phase.examsAfter", { week: label.week }));
    case "exams":
      return strong(t("phase.exams"));
    case "ended":
      return muted(t("phase.ended"));
    case "startsOn":
      return muted(
        t("phase.startsOn", { date: formatShortDate(label.date, i18n.language, today) }),
      );
    case "notStarted":
      return muted(t("phase.notStarted"));
    case "unknown":
      return muted(t("phase.unknown"));
  }
}
