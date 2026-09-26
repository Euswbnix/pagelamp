import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function SourcesPage() {
  const { t } = useTranslation("sources");
  return <PageHeader title={t("title")} />;
}
