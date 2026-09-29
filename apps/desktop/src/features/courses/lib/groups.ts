// Grouping for the course list: Current, Upcoming and Past, as the facade's lifecycle says
// (CourseLifecycle.group; calendar design §7.13, §8.2). The UI only sorts within each group.

import type { CourseSummary } from "@/api/types";
import { sortCourses } from "./courses";

export interface CourseGroups {
  current: CourseSummary[];
  upcoming: CourseSummary[];
  past: CourseSummary[];
}

/** Hidden courses are left out unless `showHidden`; each group is sorted like the old list. */
export function groupCourses(list: CourseSummary[], showHidden: boolean): CourseGroups {
  const shown = list.filter((c) => showHidden || !c.course.hidden);
  const of = (group: CourseSummary["lifecycle"]["group"]) =>
    sortCourses(shown.filter((c) => c.lifecycle.group === group));
  // Upcoming: the one starting soonest first (then by code, as sortCourses does).
  const upcoming = of("upcoming").sort((a, b) =>
    (a.lifecycle.starts_on ?? "9999").localeCompare(b.lifecycle.starts_on ?? "9999"),
  );
  return { current: of("current"), upcoming, past: of("past") };
}
