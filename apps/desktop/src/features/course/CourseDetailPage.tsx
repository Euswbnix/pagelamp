import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function CourseDetailPage() {
  const { t } = useTranslation("course");
  return <PageHeader title={t("title")} />;
}
