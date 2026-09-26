import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function CoursesPage() {
  const { t } = useTranslation("courses");
  return <PageHeader title={t("title")} />;
}
