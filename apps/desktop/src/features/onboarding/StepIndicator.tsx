import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

export const STEPS = ["welcome", "source", "sync"] as const;
export type StepName = (typeof STEPS)[number];

/** "Step 2 of 3" plus a labelled bar per step; the current step has aria-current="step". */
export function StepIndicator({ current }: { current: StepName }) {
  const { t } = useTranslation("onboarding");
  const index = STEPS.indexOf(current);
  return (
    <div className="space-y-2">
      <p className="text-sm text-muted-foreground">
        {t("steps.progress", { step: index + 1, total: STEPS.length })}
      </p>
      <ol aria-label={t("steps.label")} className="grid grid-cols-3 gap-2">
        {STEPS.map((step, i) => (
          <li
            key={step}
            aria-current={i === index ? "step" : undefined}
            className="flex flex-col gap-1.5 text-xs"
          >
            <span
              className={cn("h-1 rounded-full", i <= index ? "bg-primary" : "bg-muted")}
              aria-hidden
            />
            <span
              className={cn(
                "flex items-center gap-1",
                i === index ? "font-medium text-foreground" : "text-muted-foreground",
              )}
            >
              {i < index ? <Check className="size-3.5" aria-hidden /> : null}
              {t(`steps.${step}`)}
              {i < index ? <span className="sr-only"> {t("steps.done")}</span> : null}
            </span>
          </li>
        ))}
      </ol>
    </div>
  );
}
