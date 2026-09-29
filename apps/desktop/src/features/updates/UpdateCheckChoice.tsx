import { useCallback, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAcknowledgeUpdateDisclosure, useSetUpdatePrefs, useUpdatePrefs } from "@/api/queries";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";

/**
 * Onboarding's update-check disclosure (D2: on by default, disclosed, always ask before
 * installing): a switch with what the check sends. `useUpdateCheckChoice` holds the value and
 * records it, plus the fact that the student saw this, when they move on.
 */
export function useUpdateCheckChoice() {
  const prefs = useUpdatePrefs();
  const setPrefs = useSetUpdatePrefs();
  const acknowledge = useAcknowledgeUpdateDisclosure();
  const [choice, setChoice] = useState<boolean | null>(null);
  const value = choice ?? prefs.data?.auto_check ?? true;

  const record = useCallback(() => {
    void (async () => {
      try {
        if (prefs.data?.auto_check !== value) {
          await setPrefs.mutateAsync({ auto_check: value, channel: prefs.data?.channel ?? null });
        }
        await acknowledge.mutateAsync();
      } catch {
        // Not worth blocking onboarding: without the acknowledgement no check runs, and the
        // choice can be made again in Settings → Updates.
      }
    })();
  }, [prefs.data, value, setPrefs, acknowledge]);

  return { value, setValue: setChoice, record };
}

export function UpdateCheckChoice({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (value: boolean) => void;
}) {
  const { t } = useTranslation("updates");
  const id = useId();
  const hintId = useId();
  return (
    <div className="flex items-start gap-3">
      <Switch id={id} checked={value} onCheckedChange={onChange} aria-describedby={hintId} />
      <div className="space-y-0.5">
        <Label htmlFor={id}>{t("onboarding.autoCheck")}</Label>
        <p id={hintId} className="text-sm text-muted-foreground">
          {t("onboarding.autoCheckHint")}
        </p>
      </div>
    </div>
  );
}
