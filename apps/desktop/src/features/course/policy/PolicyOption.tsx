import { useTranslation } from "react-i18next";
import type { AiPolicy } from "@/api/types";
import { POLICY_ICON } from "@/components/common/PolicyBadge";
import { Label } from "@/components/ui/label";
import { RadioGroupItem } from "@/components/ui/radio-group";

/** One choice card: radio + icon + label, with the description announced as its description. */
export function PolicyOption({ policy, idPrefix }: { policy: AiPolicy; idPrefix: string }) {
  const { t } = useTranslation();
  const id = `${idPrefix}-${policy}`;
  const descriptionId = `${id}-description`;
  const Icon = POLICY_ICON[policy];
  return (
    <div className="relative flex items-start gap-3 rounded-row px-3 py-3 transition-colors hover:bg-muted has-data-checked:bg-ink/9">
      <RadioGroupItem id={id} value={policy} aria-describedby={descriptionId} className="mt-0.5" />
      <div className="grid gap-1">
        {/* The label's ::after covers the whole card, so clicking anywhere selects it. */}
        <Label htmlFor={id} className="cursor-pointer after:absolute after:inset-0">
          <Icon className="size-4 text-muted-foreground" aria-hidden />
          {t(`policy.${policy}`)}
        </Label>
        <p id={descriptionId} className="text-sm text-muted-foreground">
          {t(`policy.description.${policy}`)}
        </p>
      </div>
    </div>
  );
}
