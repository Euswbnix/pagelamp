import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";
import { AboutSection } from "./AboutSection";
import { AppearanceSection } from "./AppearanceSection";
import { DataSection } from "./DataSection";
import { HelpSection } from "./HelpSection";
import { PrivacySection } from "./PrivacySection";

/** Settings: appearance, data, privacy, help & feedback and about. */
export function SettingsPage() {
  const { t } = useTranslation("settings");
  return (
    <div className="max-w-3xl">
      <PageHeader title={t("title")} />
      <div className="space-y-6">
        <AppearanceSection />
        <DataSection />
        <PrivacySection />
        <HelpSection />
        <AboutSection />
      </div>
    </div>
  );
}
