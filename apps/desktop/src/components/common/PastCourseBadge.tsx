import { Archive } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { CourseLifecycle } from "@/api/types";
import { Badge } from "@/components/ui/badge";

/**
 * "Past course": the lifecycle puts the course under Past (it looks finished, or has been
 * inactive for a long time; calendar design §8.1). Its data is kept and its deadlines still
 * remind; only the grouping changes. Renders nothing for other courses.
 */
export function PastCourseBadge({ lifecycle }: { lifecycle: CourseLifecycle }) {
  const { t } = useTranslation();
  const { t: tcal } = useTranslation("calendar");
  if (lifecycle.group !== "past") return null;
  const hint =
    lifecycle.state === "inactive" ? tcal("pastBadge.inactive") : tcal("pastBadge.ended");
  return (
    <Badge variant="outline" title={hint}>
      <Archive aria-hidden />
      {t("pastCourse.badge")}
    </Badge>
  );
}
