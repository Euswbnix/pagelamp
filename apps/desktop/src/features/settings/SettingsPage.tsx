import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";
import { AiModelsSection } from "@/features/ai/AiModelsSection";
import { UsageSection } from "@/features/ai/UsageSection";
import { REMOVAL_UI } from "@/features/course/removal/availability";
import { RemovedCoursesSection } from "@/features/course/removal/RemovedCoursesSection";
import { REMINDERS_UI } from "@/features/reminders/availability";
import { RemindersSection } from "@/features/reminders/RemindersSection";
import { AI_SETUP_ENABLED } from "@/lib/features";
import { AboutSection } from "./AboutSection";
import { AppearanceSection } from "./AppearanceSection";
import { DataSection } from "./DataSection";
import { HelpSection } from "./HelpSection";
import { PrivacySection } from "./PrivacySection";
import { UpdatesSection } from "./UpdatesSection";

/**
 * Settings: appearance, data, reminders (M3), privacy, AI models and usage (M1), updates, help &
 * feedback and about.
 */
export function SettingsPage() {
  const { t } = useTranslation("settings");
  return (
    <div className="max-w-3xl">
      <PageHeader title={t("title")} />
      <div className="space-y-6">
        <AppearanceSection />
        <DataSection />
        {REMOVAL_UI ? <RemovedCoursesSection /> : null}
        {REMINDERS_UI ? <RemindersSection /> : null}
        <PrivacySection />
        {AI_SETUP_ENABLED ? (
          <>
            <AiModelsSection />
            <UsageSection />
          </>
        ) : null}
        <UpdatesSection />
        <HelpSection />
        <AboutSection />
      </div>
    </div>
  );
}
