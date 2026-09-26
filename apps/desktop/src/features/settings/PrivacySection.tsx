import { BookOpen, KeyRound, type LucideIcon, Send, ToggleRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AiDisclosure } from "@/components/common/AiDisclosure";
import { formatDate } from "@/lib/format";
import { useUiStore } from "@/stores/ui";
import { SettingsSection } from "./SettingsSection";

const POINTS: { key: "keychain" | "readOnly" | "aiProvider" | "perCourse"; icon: LucideIcon }[] = [
  { key: "keychain", icon: KeyRound },
  { key: "readOnly", icon: BookOpen },
  { key: "aiProvider", icon: Send },
  { key: "perCourse", icon: ToggleRight },
];

/** The required AI disclosure (and whether it was acknowledged) plus the promises behind it. */
export function PrivacySection() {
  const { t } = useTranslation("settings");
  const { t: tc, i18n } = useTranslation();
  const acknowledgedAt = useUiStore((s) => s.aiDisclosureAcknowledgedAt);
  return (
    <SettingsSection title={t("privacy.title")}>
      <div className="space-y-2">
        <AiDisclosure />
        <p className="text-xs text-muted-foreground">
          {acknowledgedAt
            ? tc("disclosure.acknowledgedOn", { date: formatDate(acknowledgedAt, i18n.language) })
            : t("privacy.notAcknowledged")}
        </p>
      </div>
      <ul className="space-y-2.5">
        {POINTS.map(({ key, icon: Icon }) => (
          <li key={key} className="flex gap-2.5">
            <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
            <span>{t(`privacy.${key}`)}</span>
          </li>
        ))}
      </ul>
    </SettingsSection>
  );
}
