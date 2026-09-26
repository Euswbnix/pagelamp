import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function SettingsPage() {
  const { t } = useTranslation("settings");
  return <PageHeader title={t("title")} />;
}
