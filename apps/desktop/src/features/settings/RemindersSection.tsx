import { Clock } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { SettingsSection } from "./SettingsSection";

/** Placeholder for local reminders (planned for v0.2). The switch is visible but disabled. */
export function RemindersSection() {
  const { t } = useTranslation("settings");
  const switchId = useId();
  const badgeId = useId();
  const descriptionId = useId();

  return (
    <SettingsSection title={t("reminders.title")}>
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1.5">
          <div className="flex flex-wrap items-center gap-2">
            <Label htmlFor={switchId}>{t("reminders.weekly")}</Label>
            <Badge id={badgeId} variant="outline">
              <Clock aria-hidden />
              {t("reminders.comingSoon")}
            </Badge>
          </div>
          <p id={descriptionId} className="text-sm text-muted-foreground">
            {t("reminders.description")}
          </p>
        </div>
        <Switch
          id={switchId}
          checked={false}
          disabled
          aria-disabled="true"
          aria-describedby={`${badgeId} ${descriptionId}`}
          className="mt-0.5"
        />
      </div>
    </SettingsSection>
  );
}
