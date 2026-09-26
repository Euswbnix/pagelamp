import { Archive } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/components/ui/badge";

/**
 * "Past course": Canvas no longer lists the course as active (e.g. the term ended). Its data is
 * kept; removing old courses comes in a later version.
 */
export function PastCourseBadge() {
  const { t } = useTranslation();
  return (
    <Badge variant="outline" title={t("pastCourse.hint")}>
      <Archive aria-hidden />
      {t("pastCourse.badge")}
    </Badge>
  );
}
