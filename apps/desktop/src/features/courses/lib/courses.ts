// Pure helpers for the course list.

import type { CourseSummary, SourceErrorKind, SourceRecord } from "@/api/types";

function sortKey(summary: CourseSummary): string {
  return summary.course.code ?? summary.course.name;
}

/** Visible courses by code, then hidden ones (when shown) by code. */
export function sortCourses(list: CourseSummary[]): CourseSummary[] {
  return [...list].sort(
    (a, b) =>
      Number(a.course.hidden) - Number(b.course.hidden) ||
      sortKey(a).localeCompare(sortKey(b), undefined, { numeric: true, sensitivity: "base" }),
  );
}

/**
 * The short label to show for a study-plan item's course. Plan items name a course by id
 * (or sometimes by code). Unknown ids look like "canvas:host/course/42", which is not worth
 * showing; a bare unknown code is.
 */
export function courseLabelFor(
  courseIdOrCode: string | null | undefined,
  courses: CourseSummary[] | undefined,
): string | null {
  if (!courseIdOrCode) return null;
  const match = courses?.find(
    (c) => c.course.id === courseIdOrCode || c.course.code === courseIdOrCode,
  );
  if (match) return match.course.code ?? match.course.name;
  return /[:/]/.test(courseIdOrCode) ? null : courseIdOrCode;
}

/** source id → why its last sync failed (only failing sources are included). */
export function sourceErrors(sources: SourceRecord[] | undefined): Map<string, SourceErrorKind> {
  const map = new Map<string, SourceErrorKind>();
  for (const source of sources ?? []) {
    if (source.last_error_kind) map.set(source.id, source.last_error_kind);
  }
  return map;
}
