import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetMonthlyBudget } from "@/api/ai-queries";
import type { BudgetStatus } from "@/api/provisional/ai";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { formatUsd, parseUsd, usdInputValue } from "./lib/money";
import { useAiErrorText } from "./useAiErrorText";

/**
 * The soft monthly budget for API keys (D18: US$5, warn at 80%), in micro-USD, with this month's
 * use. The facade enforces it before each run; this only shows and changes it.
 */
export function BudgetField({ budget }: { budget: BudgetStatus }) {
  // Re-seed the form whenever the saved value changes (after saving, or elsewhere).
  return <BudgetForm key={String(budget.monthly_micro_usd)} budget={budget} />;
}

function BudgetForm({ budget }: { budget: BudgetStatus }) {
  const { t, i18n } = useTranslation("ai");
  const save = useSetMonthlyBudget();
  const errorText = useAiErrorText();
  const saved = budget.monthly_micro_usd ?? null;
  const [text, setText] = useState(saved === null ? "" : usdInputValue(saved));
  const [noLimit, setNoLimit] = useState(saved === null);
  const [invalid, setInvalid] = useState(false);
  const ids = {
    heading: useId(),
    amount: useId(),
    hint: useId(),
    noLimit: useId(),
    error: useId(),
  };
  const locale = i18n.language;
  const spent = formatUsd(budget.spent_micro_usd, locale);
  const percent =
    saved && saved > 0 ? Math.min(100, Math.round((budget.spent_micro_usd / saved) * 100)) : null;

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const value = noLimit ? null : parseUsd(text);
    if (!noLimit && value === null) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    try {
      await save.mutateAsync(value);
      toast.success(t("budget.saved"));
    } catch {
      // Shown below (save.error).
    }
  }

  return (
    <form onSubmit={submit} noValidate aria-labelledby={ids.heading} className="space-y-3">
      <div className="space-y-1">
        <h3 id={ids.heading} className="font-medium">
          {t("budget.title")}
        </h3>
        <p id={ids.hint} className="text-sm text-muted-foreground">
          {t("budget.hint")}
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor={ids.amount}>{t("budget.amount")}</Label>
          <Input
            id={ids.amount}
            inputMode="decimal"
            className="w-32"
            value={text}
            disabled={noLimit}
            onChange={(e) => setText(e.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={invalid ? ids.error : ids.hint}
          />
        </div>
        <div className="flex items-center gap-2 pb-2">
          <Checkbox
            id={ids.noLimit}
            checked={noLimit}
            onCheckedChange={(v) => setNoLimit(v === true)}
          />
          <Label htmlFor={ids.noLimit} className="font-normal">
            {t("budget.noLimit")}
          </Label>
        </div>
        <Button type="submit" variant="outline" disabled={save.isPending}>
          {t("budget.save")}
        </Button>
      </div>
      {invalid ? (
        <p id={ids.error} role="alert" className="text-sm text-destructive">
          {t("budget.invalid")}
        </p>
      ) : null}
      {save.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(save.error)}
        </p>
      ) : null}
      <div className="space-y-1.5">
        <p className="text-sm">
          {saved === null
            ? t("budget.usedNoBudget", { spent })
            : t("budget.used", { spent, budget: formatUsd(saved, locale) })}
        </p>
        {percent !== null ? (
          <Progress value={percent} aria-label={t("budget.progress")} className="max-w-sm" />
        ) : null}
        {percent !== null && percent >= budget.warn_at_percent ? (
          <p className="text-sm font-medium">{t("budget.warn", { percent })}</p>
        ) : null}
      </div>
    </form>
  );
}
