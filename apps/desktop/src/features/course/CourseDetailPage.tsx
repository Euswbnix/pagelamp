import { useParams } from "react-router";
import { isApiError } from "@/api/errors";
import { useCourseOverview } from "@/api/queries";
import { CourseDetailSkeleton, CourseLoadError, CourseNotFound } from "./CoursePageStates";
import { CourseTabs } from "./CourseTabs";
import { CourseHeader } from "./header/CourseHeader";

/**
 * One course: header (name, policy, week, freshness) and tabs for this week's materials,
 * timeline + term dates, deadlines, AI policy and settings.
 * Route: /courses/:courseId — react-router hands us the decoded `course.id`.
 */
export function CourseDetailPage() {
  const { courseId = "" } = useParams();
  const overview = useCourseOverview(courseId);

  // A course that is gone (e.g. its source was removed) must not keep showing stale data.
  if (isApiError(overview.error, "not_found")) return <CourseNotFound />;
  // Keep showing loaded data if a background refetch fails for another reason.
  if (overview.data) {
    return (
      <>
        <CourseHeader overview={overview.data} />
        {/* Keyed by course, so drafts and tab state never carry over to another course. */}
        <CourseTabs key={overview.data.course.id} overview={overview.data} />
      </>
    );
  }
  if (overview.isError) {
    return <CourseLoadError error={overview.error} onRetry={() => void overview.refetch()} />;
  }
  return <CourseDetailSkeleton />;
}
