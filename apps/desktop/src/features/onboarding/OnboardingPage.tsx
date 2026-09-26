import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function OnboardingPage() {
  const { t } = useTranslation("onboarding");
  return <PageHeader title={t("title")} />;
}
