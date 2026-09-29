import { Info } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { useSetCourseMaterialSharing } from "@/api/ai-queries";
import { Button } from "@/components/ui/button";
import { SHARING_ANSWERS } from "@/features/course/policy/MaterialSharingSection";
import { paths } from "@/lib/routes";
import { useAiErrorText } from "./useAiErrorText";

/**
 * The one-time reminder on a course's first cloud run (D37; the facade's GenEvent notice
 * `material_sharing_reminder`): non-blocking, with the three answers inline. The run has already
 * gone ahead; answering (or dismissing) just closes it. Never styled as an error.
 */
export function MaterialSharingReminder({
  courseId,
  courseName,
  service,
  onClose,
}: {
  courseId: string;
  courseName: string;
  /** The service the text went to (the result's `material_sent_to`). */
  service: string;
  onClose: () => void;
}) {
  const { t } = useTranslation("ai");
  const save = useSetCourseMaterialSharing();
  const errorText = useAiErrorText();
  const ids = { heading: useId(), answer: useId() };

  async function answer(value: (typeof SHARING_ANSWERS)[number]) {
    if (save.isPending) return;
    try {
      await save.mutateAsync({ courseId, answer: value });
      toast.success(t("sharing.saved"));
      onClose();
    } catch {
      // Shown below (save.error).
    }
  }

  return (
    <section
      aria-labelledby={ids.heading}
      className="flex gap-3 rounded-lg border bg-muted/40 p-4 text-sm"
    >
      <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
      <div className="min-w-0 space-y-2">
        <h3 id={ids.heading} className="font-medium">
          {t("sharing.reminder.title")}
        </h3>
        <p className="text-muted-foreground">
          {t("sharing.reminder.body", { course: courseName, service })}
        </p>
        <p id={ids.answer}>{t("sharing.reminder.answer")}</p>
        <div role="group" aria-labelledby={ids.answer} className="flex flex-wrap gap-2">
          {SHARING_ANSWERS.map((value) => (
            <Button
              key={value}
              type="button"
              size="sm"
              variant="outline"
              disabled={save.isPending}
              onClick={() => void answer(value)}
            >
              {t(`sharing.option.${value}`)}
            </Button>
          ))}
          <Button type="button" size="sm" variant="ghost" onClick={onClose}>
            {t("sharing.reminder.dismiss")}
          </Button>
        </div>
        {save.error ? (
          <p role="alert" className="text-destructive">
            {errorText(save.error)}
          </p>
        ) : null}
      </div>
    </section>
  );
}

/**
 * The facade refused a cloud run with `material_sharing_not_allowed`: nothing was sent. Offers
 * what still works (a model on this computer, a structure-only overview) and a way to change the
 * answer. Neutral styling: respecting the answer is not an error.
 */
export function SharingNotAllowedNotice({
  courseId,
  courseName,
  onUseLocal,
  onStructureOnly,
}: {
  courseId: string;
  courseName: string;
  /** Present when an on-device model is set up. */
  onUseLocal?: () => void;
  /** Present where the feature has a structure-only variant. */
  onStructureOnly?: () => void;
}) {
  const { t } = useTranslation("ai");
  const headingId = useId();
  return (
    <section
      aria-labelledby={headingId}
      className="flex gap-3 rounded-lg border bg-muted/40 p-4 text-sm"
    >
      <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
      <div className="min-w-0 space-y-2">
        <h3 id={headingId} className="font-medium">
          {t("sharing.notAllowed.title")}
        </h3>
        <p className="text-muted-foreground">
          {t("sharing.notAllowed.body", { course: courseName })}
        </p>
        <div className="flex flex-wrap gap-2">
          {onUseLocal ? (
            <Button type="button" size="sm" onClick={onUseLocal}>
              {t("sharing.notAllowed.useLocal")}
            </Button>
          ) : null}
          {onStructureOnly ? (
            <Button type="button" size="sm" variant="outline" onClick={onStructureOnly}>
              {t("sharing.notAllowed.structureOnly")}
            </Button>
          ) : null}
          <Button asChild size="sm" variant="ghost">
            <Link to={`${paths.course(courseId)}?tab=policy`}>
              {t("sharing.notAllowed.change")}
            </Link>
          </Button>
        </div>
      </div>
    </section>
  );
}
