import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetModeAWeeklyCap } from "@/api/ai-queries";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAiErrorText } from "@/features/ai/useAiErrorText";

/**
 * The weekly limit on PageLamp's ChatGPT-plan runs (design §2.3: mode A has no money budget,
 * but runs past the plan's limit may use ChatGPT credits). The facade enforces it
 * (`weekly_run_cap_reached`); this shows and changes it.
 */
export function WeeklyCapField({ cap, runs }: { cap: number | null; runs: number }) {
  // Re-seed the form whenever the saved value changes.
  return <CapForm key={String(cap)} cap={cap} runs={runs} />;
}

function CapForm({ cap, runs }: { cap: number | null; runs: number }) {
  const { t } = useTranslation("ai");
  const save = useSetModeAWeeklyCap();
  const errorText = useAiErrorText();
  const [text, setText] = useState(cap === null ? "" : String(cap));
  const [noLimit, setNoLimit] = useState(cap === null);
  const [invalid, setInvalid] = useState(false);
  const ids = {
    heading: useId(),
    amount: useId(),
    hint: useId(),
    noLimit: useId(),
    error: useId(),
  };

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const value = noLimit ? null : /^\d+$/.test(text.trim()) ? Number(text.trim()) : Number.NaN;
    if (value !== null && (!Number.isInteger(value) || value < 1)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    try {
      await save.mutateAsync(value);
      toast.success(t("codex.cap.saved"));
    } catch {
      // Shown below (save.error).
    }
  }

  return (
    <form onSubmit={submit} noValidate aria-labelledby={ids.heading} className="space-y-3">
      <div className="space-y-1">
        <h4 id={ids.heading} className="text-sm font-medium">
          {t("codex.cap.title")}
        </h4>
        <p id={ids.hint} className="text-sm text-muted-foreground">
          {t("codex.cap.hint")}
        </p>
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor={ids.amount}>{t("codex.cap.amount")}</Label>
          <Input
            id={ids.amount}
            inputMode="numeric"
            className="w-24"
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
            {t("codex.cap.noLimit")}
          </Label>
        </div>
        <Button type="submit" variant="outline" disabled={save.isPending}>
          {t("codex.cap.save")}
        </Button>
      </div>
      {invalid ? (
        <p id={ids.error} role="alert" className="text-sm text-destructive">
          {t("codex.cap.invalid")}
        </p>
      ) : null}
      {save.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(save.error)}
        </p>
      ) : null}
      <p className="text-sm">
        {cap === null ? t("codex.cap.usedNoCap", { runs }) : t("codex.cap.used", { runs, cap })}
      </p>
    </form>
  );
}
