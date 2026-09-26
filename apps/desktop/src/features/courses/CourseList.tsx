import { useId } from "react";
import { useTranslation } from "react-i18next";
import { useStatus } from "@/api/queries";
import type { CourseSummary } from "@/api/types";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useUiStore } from "@/stores/ui";
import { CourseCard } from "./CourseCard";
import { sortCourses, sourceErrors } from "./lib/courses";
import { Section } from "./parts/Section";

/** The course cards. Hidden courses appear only when "Show hidden courses" is on. */
export function CourseList({ courses }: { courses: CourseSummary[] }) {
  const { t } = useTranslation("courses");
  const switchId = useId();
  const showHidden = useUiStore((s) => s.showHiddenCourses);
  const setShowHidden = useUiStore((s) => s.setShowHiddenCourses);
  const status = useStatus();
  const errors = sourceErrors(status.data?.sources);

  const hiddenCount = courses.filter((c) => c.course.hidden).length;
  const shown = sortCourses(courses.filter((c) => showHidden || !c.course.hidden));

  return (
    <Section
      title={t("list.title")}
      actions={
        hiddenCount > 0 ? (
          <div className="flex items-center gap-2">
            <Switch id={switchId} checked={showHidden} onCheckedChange={setShowHidden} />
            <Label htmlFor={switchId} className="font-normal">
              {t("list.showHidden", { count: hiddenCount })}
            </Label>
          </div>
        ) : null
      }
    >
      {shown.length === 0 ? (
        <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">
          {t("list.allHidden")}
        </p>
      ) : (
        <ul className="grid gap-3 sm:grid-cols-2">
          {shown.map((summary) => (
            <li key={summary.course.id}>
              <CourseCard
                summary={summary}
                sourceError={errors.get(summary.course.source_id) ?? null}
              />
            </li>
          ))}
        </ul>
      )}
    </Section>
  );
}
