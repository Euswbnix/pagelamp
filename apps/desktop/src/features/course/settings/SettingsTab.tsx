import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { Course, CourseLifecycle } from "@/api/types";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { REMOVAL_UI } from "../removal/availability";
import { RemoveCourseSection } from "../removal/RemoveCourseSection";
import { useCourseHidden } from "../useCourseHidden";

/** Per-course settings: hide the course from the list and the AI app, or remove it. */
export function SettingsTab({ course, lifecycle }: { course: Course; lifecycle: CourseLifecycle }) {
  const { t } = useTranslation("course");
  const ids = { heading: useId(), hide: useId(), hideHelp: useId() };
  const { setHidden, isPending, pendingValue } = useCourseHidden(course);

  return (
    <section aria-labelledby={ids.heading} className="max-w-2xl space-y-4">
      <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
        {t("settings.title")}
      </h2>
      <div className="pl-callout flex items-start justify-between gap-6 p-4">
        <div className="grid gap-1">
          <Label htmlFor={ids.hide}>{t("settings.hideLabel")}</Label>
          <p id={ids.hideHelp} className="text-sm text-muted-foreground">
            {t("settings.hideDescription")}
          </p>
        </div>
        <Switch
          id={ids.hide}
          checked={pendingValue ?? course.hidden}
          // Not disabled while saving, so keyboard focus stays on the switch.
          onCheckedChange={(checked) => {
            if (!isPending) void setHidden(checked);
          }}
          aria-describedby={ids.hideHelp}
        />
      </div>
      {REMOVAL_UI ? <RemoveCourseSection course={course} lifecycle={lifecycle} /> : null}
    </section>
  );
}
