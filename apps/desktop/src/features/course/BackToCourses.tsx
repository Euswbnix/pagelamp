import { ArrowLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { paths } from "@/lib/routes";

/** "← All courses": the toolbar row's leading item (PageHeader `leading`), else above the page. */
export function BackToCourses() {
  const { t } = useTranslation("course");
  return (
    <Link
      to={paths.courses}
      className="inline-flex items-center gap-1.5 rounded-sm text-sm text-muted-foreground hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
    >
      <ArrowLeft className="size-4" aria-hidden />
      {t("backToCourses")}
    </Link>
  );
}
