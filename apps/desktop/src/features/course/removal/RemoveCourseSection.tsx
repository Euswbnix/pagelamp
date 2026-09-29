import { Trash2 } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import type { Course, CourseLifecycle } from "@/api/types";
import { Button } from "@/components/ui/button";
import { focusPageHeading } from "@/lib/focus";
import { paths } from "@/lib/routes";
import { RemoveCoursesDialog } from "./RemoveCoursesDialog";

/**
 * Course → Settings, under "Hide this course": "Remove from PageLamp…", with the difference
 * from hiding spelled out (calendar design §8.7). After removing, the course page is gone, so
 * this goes back to the course list.
 */
export function RemoveCourseSection({
  course,
  lifecycle,
}: {
  course: Course;
  lifecycle: CourseLifecycle;
}) {
  const { t } = useTranslation("removal");
  const navigate = useNavigate();
  const ids = { heading: useId(), help: useId() };
  const [open, setOpen] = useState(false);
  const entry = {
    course_id: course.id,
    code: course.code ?? null,
    name: course.name,
    hidden: course.hidden,
    lifecycle,
  };

  return (
    <section
      aria-labelledby={ids.heading}
      className="flex flex-wrap items-start justify-between gap-4 rounded-lg border bg-card p-4"
    >
      <div className="grid gap-1">
        <h3 id={ids.heading} className="text-sm font-medium">
          {t("settings.title")}
        </h3>
        <div id={ids.help} className="space-y-1 text-sm text-muted-foreground">
          <p>{t("settings.hide")}</p>
          <p>{t("settings.remove")}</p>
        </div>
      </div>
      <Button variant="outline" aria-describedby={ids.help} onClick={() => setOpen(true)}>
        <Trash2 aria-hidden />
        {t("settings.button")}
      </Button>
      <RemoveCoursesDialog
        open={open}
        onOpenChange={setOpen}
        mode="one"
        candidates={[entry]}
        initiallySelected={[course.id]}
        onRemoved={() => {
          navigate(paths.courses);
          requestAnimationFrame(focusPageHeading);
        }}
      />
    </section>
  );
}
