import { Archive } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useLifecycleSummary, useSnoozeLifecycleBanner } from "@/api/removalQueries";
import { Button } from "@/components/ui/button";
import { focusPageHeading } from "@/lib/focus";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { Notice } from "../../courses/parts/Notice";
import { RemoveCoursesDialog } from "./RemoveCoursesDialog";

/**
 * "3 courses look finished — Review / Not now" on the Courses page (calendar design §8.2). The
 * facade decides when it shows (lifecycle_summary.show_banner); "Not now" silences the same
 * courses for 14 days.
 */
export function LifecycleBanner() {
  const { t } = useTranslation("removal");
  const errorText = useApiErrorText();
  const summary = useLifecycleSummary();
  const snooze = useSnoozeLifecycleBanner();
  const [open, setOpen] = useState(false);

  const data = summary.data;
  // After a removal the banner may go away with its dialog; the toast (global) stays.
  if (!data?.show_banner) return null;
  const candidates = data.courses.filter((c) => data.suggested.includes(c.course_id));

  async function notNow() {
    if (snooze.isPending) return;
    try {
      await snooze.mutateAsync();
      toast.success(t("banner.snoozed"));
      // The banner (and the focused button) is gone; continue from the page heading.
      focusPageHeading();
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return (
    <section aria-label={t("banner.title", { count: candidates.length })}>
      <Notice
        icon={<Archive className="size-4 text-muted-foreground" aria-hidden />}
        title={t("banner.title", { count: candidates.length })}
        action={
          <div className="flex gap-2">
            <Button size="sm" onClick={() => setOpen(true)}>
              {t("banner.review")}
            </Button>
            <Button
              size="sm"
              variant="outline"
              aria-disabled={snooze.isPending || undefined}
              onClick={() => void notNow()}
            >
              {t("banner.notNow")}
            </Button>
          </div>
        }
      >
        <p>{t("banner.body")}</p>
      </Notice>
      <RemoveCoursesDialog
        open={open}
        onOpenChange={setOpen}
        mode="review"
        candidates={candidates}
        initiallySelected={candidates.map((c) => c.course_id)}
      />
    </section>
  );
}
