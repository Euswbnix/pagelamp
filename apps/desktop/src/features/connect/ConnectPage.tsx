import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/common/PageHeader";

// Placeholder — replaced by the real screen.
export function ConnectPage() {
  const { t } = useTranslation("connect");
  return <PageHeader title={t("title")} />;
}
