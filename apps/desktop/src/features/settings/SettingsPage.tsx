import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";
import { AboutSection } from "./AboutSection";
import { AppearanceSection } from "./AppearanceSection";
import { DataSection } from "./DataSection";
import { PrivacySection } from "./PrivacySection";
import { RemindersSection } from "./RemindersSection";

/** Settings: appearance, (upcoming) reminders, where the data lives, privacy and about. */
export function SettingsPage() {
  const { t } = useTranslation("settings");
  return (
    <div className="max-w-3xl">
      <PageHeader title={t("title")} />
      <div className="space-y-6">
        <AppearanceSection />
        <RemindersSection />
        <DataSection />
        <PrivacySection />
        <AboutSection />
      </div>
    </div>
  );
}
