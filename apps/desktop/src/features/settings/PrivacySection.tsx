import { BookOpen, KeyRound, type LucideIcon, Send } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AiDisclosure } from "@/components/common/AiDisclosure";
import { SettingsSection } from "./SettingsSection";

const POINTS: { key: "keychain" | "readOnly" | "aiProvider"; icon: LucideIcon }[] = [
  { key: "keychain", icon: KeyRound },
  { key: "readOnly", icon: BookOpen },
  { key: "aiProvider", icon: Send },
];

/** The required AI disclosure plus the concrete promises behind it. */
export function PrivacySection() {
  const { t } = useTranslation("settings");
  return (
    <SettingsSection title={t("privacy.title")}>
      <AiDisclosure />
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
