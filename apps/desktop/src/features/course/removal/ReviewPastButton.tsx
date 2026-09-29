import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useLifecycleSummary } from "@/api/removalQueries";
import { Button } from "@/components/ui/button";
import { RemoveCoursesDialog } from "./RemoveCoursesDialog";

/**
 * "Review past courses…" beside the Past group (calendar design §8.7): the checklist of every
 * past course, hidden ones included, with the facade's suggestions ticked.
 */
export function ReviewPastButton() {
  const { t } = useTranslation("removal");
  const summary = useLifecycleSummary();
  const [open, setOpen] = useState(false);
  const data = summary.data;
  const candidates = data?.courses.filter((c) => c.lifecycle.group === "past") ?? [];
  if (candidates.length === 0 && !open) return null;

  return (
    <>
      <Button size="sm" variant="outline" onClick={() => setOpen(true)}>
        {t("reviewPast")}
      </Button>
      <RemoveCoursesDialog
        open={open}
        onOpenChange={setOpen}
        mode="review"
        candidates={candidates}
        initiallySelected={candidates
          .filter((c) => data?.suggested.includes(c.course_id))
          .map((c) => c.course_id)}
      />
    </>
  );
}
