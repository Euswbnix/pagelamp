import type { CourseSummary, IsoDate } from "@/api/types";
import { addDays } from "@/features/courses/lib/plan";

/** How far ahead an upcoming course counts as active (the facade's rule, design §5.1). */
const UPCOMING_DAYS = 14;

/**
 * The courses a plan covers by default: visible and active (lifecycle `is_active`: Current,
 * Finishing, Unknown, or Upcoming within 14 days), the same set generate_study_plan plans when
 * given none. Mirrors the facade until CourseLifecycle carries `is_active` itself.
 */
export function activeCourses(list: CourseSummary[], today: IsoDate): CourseSummary[] {
  const horizon = addDays(today, UPCOMING_DAYS);
  return list.filter(({ course, lifecycle }) => {
    if (course.hidden) return false;
    switch (lifecycle.state) {
      case "current":
      case "finishing":
      case "unknown":
        return true;
      case "upcoming":
        return !!lifecycle.starts_on && lifecycle.starts_on <= horizon;
      default:
        return false;
    }
  });
}
