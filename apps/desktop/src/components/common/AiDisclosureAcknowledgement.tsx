import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { formatDate } from "@/lib/format";
import { useUiStore } from "@/stores/ui";
import { AiDisclosure } from "./AiDisclosure";

/**
 * The AI disclosure with its one-time "I understand" (docs/ARCHITECTURE.md §3 rule 8). The
 * acknowledgement is stored in the UI prefs; screens gate "continue" on it.
 */
export function AiDisclosureAcknowledgement() {
  const { t, i18n } = useTranslation();
  const acknowledgedAt = useUiStore((s) => s.aiDisclosureAcknowledgedAt);
  const setAcknowledged = useUiStore((s) => s.setAiDisclosureAcknowledged);
  const id = useId();
  return (
    <div className="space-y-3">
      <AiDisclosure />
      <div className="flex items-center gap-3 px-1">
        <Checkbox
          id={id}
          checked={acknowledgedAt !== null}
          onCheckedChange={(value) => setAcknowledged(value === true)}
        />
        <Label htmlFor={id}>{t("disclosure.acknowledge")}</Label>
      </div>
      {acknowledgedAt ? (
        <p className="px-1 text-xs text-muted-foreground">
          {t("disclosure.acknowledgedOn", { date: formatDate(acknowledgedAt, i18n.language) })}
        </p>
      ) : null}
    </div>
  );
}
