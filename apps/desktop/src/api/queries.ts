// TanStack Query hooks — the way screens read and change data. Screens never call `useApi()`
// methods directly for reads; they use these hooks so caching and invalidation stay consistent.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useApi } from "./context";
import type { AiPolicy, IsoDate } from "./types";

export const queryKeys = {
  all: ["weekmark"] as const,
  status: () => [...queryKeys.all, "status"] as const,
  sources: () => [...queryKeys.all, "sources"] as const,
  courses: () => [...queryKeys.all, "courses"] as const,
  course: (courseId: string) => [...queryKeys.all, "course", courseId] as const,
  week: (courseId: string, week: number | null) =>
    [...queryKeys.all, "course", courseId, "week", week] as const,
  deadlines: (courseId: string | null, daysAhead: number, daysBack: number) =>
    [...queryKeys.all, "deadlines", courseId, daysAhead, daysBack] as const,
  studyPlan: () => [...queryKeys.all, "study-plan"] as const,
  mcpConfigs: () => [...queryKeys.all, "mcp-configs"] as const,
  lastCrash: () => [...queryKeys.all, "last-crash"] as const,
  // Deliberately outside `all`: a sync finishing (which invalidates `all`) must not swap the
  // text the student is reviewing before they copy it.
  diagnosticReport: () => ["diagnostic-report"] as const,
};

// ----- reads ----------------------------------------------------------------------------------

export function useStatus() {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.status(),
    queryFn: () => api.status(),
    // While another process (e.g. the CLI) is syncing, poll so "busy" clears by itself.
    refetchInterval: (query) => (query.state.data?.sync_in_progress ? 3000 : false),
  });
}

export function useSources() {
  const api = useApi();
  return useQuery({ queryKey: queryKeys.sources(), queryFn: () => api.listSources() });
}

/** All courses including hidden ones — filter on `course.hidden` in the UI. */
export function useCourses() {
  const api = useApi();
  return useQuery({ queryKey: queryKeys.courses(), queryFn: () => api.listCourses() });
}

export function useCourseOverview(courseId: string) {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.course(courseId),
    queryFn: () => api.courseOverview(courseId),
  });
}

/** `week` null = the course's current week. */
export function useWeekMaterials(courseId: string, week: number | null) {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.week(courseId, week),
    queryFn: () => api.weekMaterials(courseId, week),
    placeholderData: (previous) => previous,
  });
}

/** `day` (e.g. from useToday) only keys the cache, so the window moves on at midnight. */
export function useDeadlines(
  courseId: string | null,
  daysAhead: number,
  daysBack = 0,
  day?: string,
) {
  const api = useApi();
  return useQuery({
    queryKey: [...queryKeys.deadlines(courseId, daysAhead, daysBack), day ?? null],
    queryFn: () => api.listDeadlines(courseId, daysAhead, daysBack),
  });
}

export function useStudyPlan() {
  const api = useApi();
  return useQuery({ queryKey: queryKeys.studyPlan(), queryFn: () => api.latestStudyPlan() });
}

export function useMcpClientConfigs() {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.mcpConfigs(),
    queryFn: () => api.mcpClientConfigs(),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/** What the panic hook recorded last time (null = nothing to report). */
export function useLastCrash() {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.lastCrash(),
    queryFn: () => api.lastCrash(),
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/**
 * The diagnostic report, fetched while `enabled` (the preview is open). `gcTime: 0` drops it as
 * soon as the preview closes, so every preview shows a fresh report and none stays in memory.
 */
export function useDiagnosticReport(enabled: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: queryKeys.diagnosticReport(),
    queryFn: () => api.diagnosticReport(),
    enabled,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 0,
  });
}

// ----- writes ---------------------------------------------------------------------------------
//
// Secrets (tokens, feed URLs) are passed straight through as mutation variables and are never
// put in a query key or cache. `gcTime: 0` drops the mutation (and its variables) from the
// mutation cache as soon as it settles.
//
// Every write invalidates all queries and awaits the refetch in onSuccess. If a component
// remounts on that refetch (e.g. a form keyed on saved values), per-call
// `mutate(vars, { onSuccess })` callbacks never fire — use `await mutateAsync(vars)` inside
// try/catch and show the toast afterwards.

function useInvalidateAll() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: queryKeys.all });
}

export function useAddCanvasSource() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { baseUrl: string; token: string }) => api.addCanvasSource(v.baseUrl, v.token),
    onSuccess: invalidate,
    gcTime: 0,
  });
}

export function useAddFolderSource() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { path: string; termStart: IsoDate | null; label: string | null }) =>
      api.addFolderSource(v.path, v.termStart, v.label),
    onSuccess: invalidate,
  });
}

export function useAddIcalSource() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { feedUrl: string; label: string | null }) =>
      api.addIcalSource(v.feedUrl, v.label),
    onSuccess: invalidate,
    gcTime: 0,
  });
}

export function useUpdateSourceSecret() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { sourceId: string; secret: string }) =>
      api.updateSourceSecret(v.sourceId, v.secret),
    onSuccess: invalidate,
    gcTime: 0,
  });
}

export function useRemoveSource() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (sourceId: string) => api.removeSource(sourceId),
    onSuccess: invalidate,
  });
}

export function useSetCoursePolicy() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseId: string; policy: AiPolicy; note: string | null }) =>
      api.setCoursePolicy(v.courseId, v.policy, v.note),
    onSuccess: invalidate,
  });
}

export function useSetCourseTerm() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseId: string; start: IsoDate | null; end: IsoDate | null }) =>
      api.setCourseTerm(v.courseId, v.start, v.end),
    onSuccess: invalidate,
  });
}

export function useSetCourseAiAccess() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseId: string; allowed: boolean }) =>
      api.setCourseAiAccess(v.courseId, v.allowed),
    onSuccess: invalidate,
  });
}

export function useSetCourseHidden() {
  const api = useApi();
  const invalidate = useInvalidateAll();
  return useMutation({
    mutationFn: (v: { courseId: string; hidden: boolean }) =>
      api.setCourseHidden(v.courseId, v.hidden),
    onSuccess: invalidate,
  });
}

export function useClearLastCrash() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => api.clearLastCrash(),
    onSuccess: () => client.setQueryData(queryKeys.lastCrash(), null),
  });
}
