import { useId } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetCourseMaterialSharing } from "@/api/ai-queries";
import { type MaterialSharing, materialSharing } from "@/api/provisional/ai";
import type { Course } from "@/api/types";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { useAiErrorText } from "@/features/ai/useAiErrorText";

/** The answers a student can give; "unanswered" is only the starting state. */
export const SHARING_ANSWERS: readonly Exclude<MaterialSharing, "unanswered">[] = [
  "allowed",
  "not_sure",
  "not_allowed",
];

function isAnswer(value: string): value is Exclude<MaterialSharing, "unanswered"> {
  return (SHARING_ANSWERS as readonly string[]).includes(value);
}

/**
 * Question (b) (design §4.1, D37): may this course's materials be shared with an AI service?
 * Separate from the AI-use rule above it. Not answered and "not sure" still send, with a one-time
 * reminder; "not allowed" never reaches a cloud service. The facade enforces it; this only
 * records the answer. Neutral styling throughout: an answer is never an error.
 */
export function MaterialSharingSection({ course }: { course: Course }) {
  const { t } = useTranslation("ai");
  const errorText = useAiErrorText();
  const save = useSetCourseMaterialSharing();
  const answer = materialSharing(course);
  const ids = { heading: useId(), question: useId(), options: useId(), note: useId() };

  async function choose(value: string) {
    if (!isAnswer(value) || value === answer || save.isPending) return;
    try {
      await save.mutateAsync({ courseId: course.id, answer: value });
      toast.success(t("sharing.saved"));
    } catch {
      // Shown below (save.error).
    }
  }

  return (
    <section aria-labelledby={ids.heading} className="max-w-2xl space-y-3 rounded-lg border p-4">
      <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
        {t("sharing.title")}
      </h2>
      <div className="space-y-1">
        <p id={ids.question} className="text-sm font-medium">
          {t("sharing.question")}
        </p>
        <p className="text-sm text-muted-foreground">{t("sharing.hint")}</p>
      </div>
      <RadioGroup
        aria-labelledby={ids.question}
        aria-describedby={answer === "unanswered" ? ids.note : undefined}
        value={answer === "unanswered" ? "" : answer}
        onValueChange={(value) => void choose(value)}
      >
        {SHARING_ANSWERS.map((option) => {
          const id = `${ids.options}-${option}`;
          return (
            <div
              key={option}
              className="relative flex items-start gap-3 rounded-lg border p-3 transition-colors hover:bg-muted/40 has-data-checked:border-primary/40 has-data-checked:bg-primary/5"
            >
              <RadioGroupItem
                id={id}
                value={option}
                aria-describedby={`${id}-hint`}
                className="mt-0.5"
              />
              <div className="grid gap-1">
                <Label htmlFor={id} className="cursor-pointer after:absolute after:inset-0">
                  {t(`sharing.option.${option}`)}
                </Label>
                <p id={`${id}-hint`} className="text-sm text-muted-foreground">
                  {t(`sharing.optionHint.${option}`)}
                </p>
              </div>
            </div>
          );
        })}
      </RadioGroup>
      {answer === "unanswered" ? (
        <p id={ids.note} className="text-sm text-muted-foreground">
          {t("sharing.unanswered")}
        </p>
      ) : null}
      {save.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(save.error)}
        </p>
      ) : null}
    </section>
  );
}
