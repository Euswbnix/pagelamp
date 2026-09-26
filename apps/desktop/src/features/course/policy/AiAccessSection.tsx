import { useId } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetCourseAiAccess } from "@/api/queries";
import type { AiMaterialsState, Course } from "@/api/types";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * "Let my AI app read this course's materials" (docs/ARCHITECTURE.md §3 rule 8). The switch
 * stores `course.ai_access`; `state` is the backend's effective verdict, where a "No AI" policy
 * wins: the switch then shows off and can't be changed, but its stored value is kept and applies
 * again once the policy changes.
 */
export function AiAccessSection({ course, state }: { course: Course; state: AiMaterialsState }) {
  const { t } = useTranslation("course");
  const errorText = useApiErrorText();
  const mutation = useSetCourseAiAccess();
  const ids = { heading: useId(), switch: useId(), note: useId() };
  const withheld = state === "withheld_by_policy";

  async function toggle(allowed: boolean) {
    // aria-disabled (not disabled) keeps the switch focusable so its note can be read.
    if (withheld || mutation.isPending) return;
    try {
      await mutation.mutateAsync({ courseId: course.id, allowed });
      toast.success(allowed ? t("aiAccess.nowOn") : t("aiAccess.nowOff"));
    } catch {
      // Shown inline below (mutation.error).
    }
  }

  return (
    <section aria-labelledby={ids.heading} className="max-w-2xl space-y-3 rounded-lg border p-4">
      <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
        {t("aiAccess.title")}
      </h2>
      <div className="flex items-start justify-between gap-4">
        <Label htmlFor={ids.switch} className="leading-snug">
          {t("aiAccess.label")}
        </Label>
        <Switch
          id={ids.switch}
          checked={!withheld && course.ai_access}
          onCheckedChange={(checked) => void toggle(checked)}
          aria-disabled={withheld || undefined}
          aria-describedby={ids.note}
          className="aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
        />
      </div>
      <p id={ids.note} className="text-sm text-muted-foreground">
        {t(`aiAccess.note.${state}`)}
      </p>
      {mutation.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(mutation.error)}
        </p>
      ) : null}
    </section>
  );
}
