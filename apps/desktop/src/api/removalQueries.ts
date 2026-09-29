// Query hooks for the course lifecycle, removal and the dates form v2 (calendar design §8,
// §7.10; F2). Kept beside queries.ts so the course lane's additions stay in their own file;
// the keys live under `queryKeys.all`, so every sync and every mutation refreshes them.

import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useApi } from "./context";
import { queryKeys } from "./queries";
import type { CourseDatesInput, RemoveOptions, SnoozeKind } from "./types";

export const removalKeys = {
  lifecycleSummary: () => [...queryKeys.all, "lifecycle-summary"] as const,
  removalPreview: (courseIds: string[]) =>
    [...queryKeys.all, "removal-preview", [...courseIds].sort()] as const,
  removedCourses: () => [...queryKeys.all, "removed-courses"] as const,
};

function useInvalidateAll() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: queryKeys.all });
}

// ----- reads ----------------------------------------------------------------------------------

export function useLifecycleSummary() {
  const api = useApi();
  return useQuery({
    queryKey: removalKeys.lifecycleSummary(),
    queryFn: () => api.lifecycleSummary(),
  });
}

/** What removing `courseIds` would delete and keep (the previous answer stays while ticking). */
export function useRemovalPreview(courseIds: string[], enabled = true) {
  const api = useApi();
  return useQuery({
    queryKey: removalKeys.removalPreview(courseIds),
    queryFn: () => api.removalPreview(courseIds),
    enabled: enabled && courseIds.length > 0,
    placeholderData: keepPreviousData,
  });
}

export function useRemovedCourses() {
  const api = useApi();
  return useQuery({
    queryKey: removalKeys.removedCourses(),
    queryFn: () => api.removedCourses(),
  });
}

// ----- changes ----------------------------------------------------------------------------------

export function useSnoozeLifecycleBanner() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({ mutationFn: () => api.snoozeLifecycleBanner(), onSuccess: invalidate });
}

export function useSnoozeRemovalSuggestions() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseIds: string[]; kind: SnoozeKind }) =>
      api.snoozeRemovalSuggestions(v.courseIds, v.kind),
    onSuccess: invalidate,
  });
}

export function useRemoveCourses() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseIds: string[]; options: RemoveOptions }) =>
      api.removeCourses(v.courseIds, v.options),
    onSuccess: invalidate,
  });
}

export function useRestoreCourse() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { removedId: string }) => api.restoreCourse(v.removedId),
    onSuccess: invalidate,
  });
}

export function usePurgeRemovedCourses() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { removedIds: string[] | null; permanentIfNoTrash: boolean }) =>
      api.purgeRemovedCourses(v.removedIds, v.permanentIfNoTrash),
    onSuccess: invalidate,
  });
}

export function useForgetRemovedCourse() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { removedId: string }) => api.forgetRemovedCourse(v.removedId),
    onSuccess: invalidate,
  });
}

/**
 * Undo for the "Removed" toast. The toast outlives the dialog (and, from a course page, the
 * page), so this calls the API directly rather than through a component's mutation.
 */
export function useUndoRemoval() {
  const api = useApi();
  const client = useQueryClient();
  return async (removedIds: string[]) => {
    try {
      for (const id of removedIds) await api.restoreCourse(id);
    } finally {
      await client.invalidateQueries({ queryKey: queryKeys.all });
    }
  };
}

export function useSetCourseDates() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseId: string; dates: CourseDatesInput | null }) =>
      api.setCourseDates(v.courseId, v.dates),
    onSuccess: invalidate,
  });
}
