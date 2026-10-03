// Live sync progress, shared by the sidebar pill, the Sources screen and onboarding.
// Holds only what SyncEvents carry (labels, progress messages, error kinds) — no secrets.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef } from "react";
import { create } from "zustand";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { queryKeys, useStatus } from "@/api/queries";
import type {
  AppStatus,
  AutoSyncTrigger,
  SourceErrorKind,
  SyncEvent,
  SyncStage,
  SyncSummary,
} from "@/api/types";

export interface SourceProgress {
  sourceId: string;
  label: string;
  /** The facade's English step text (CLI, logs); the UI translates `stage` when it is set. */
  message: string | null;
  /** What the step is (translated in the UI), and the course it is about. */
  stage: SyncStage | null;
  course: string | null;
  current: number | null;
  total: number | null;
  warnings: string[];
  /** Set when the source finished. */
  result: { ok: boolean; error: string | null; errorKind: SourceErrorKind | null } | null;
  /** The run ended (e.g. failed as a whole) before this source finished. */
  stopped: boolean;
}

interface SyncState {
  running: boolean;
  /** How many sources this run covers (1 for a single-source sync), when known. */
  total: number | null;
  /** Source ids in the order they started. */
  order: string[];
  bySource: Record<string, SourceProgress>;
  lastSummary: SyncSummary | null;
  /** Error that stopped the whole run (e.g. `busy`). Per-source failures live in bySource. */
  runError: ApiError | null;
  /** The course whose files this run downloads (download_course_files); null for a sync. */
  downloadCourseId: string | null;
  /**
   * What started the current or last run: null = the student, else the trigger of an automatic
   * sync. An automatic run that goes wrong leaves no trace here (see `finish`).
   */
  automatic: AutoSyncTrigger | null;
  /**
   * No automatic sync starts before this time (ms): half an hour after the last one started,
   * and after the student stopped any sync ("not now"). A backstop next to the facade's clock.
   */
  noAutomaticBefore: number;
  /**
   * The last automatic run found a problem only the student can fix (an expired token, a missing
   * folder). The source's card and the pill show it; screen readers hear it once.
   */
  automaticProblem: boolean;
  /** The student is in the capsule or its details: an automatic run's problem stays on screen. */
  watched: boolean;
  /** Until when (ms) an answer to "what's due?" counts as attended: the student just acted. */
  attendedUntil: number;
  begin: (
    total: number | null,
    downloadCourseId?: string | null,
    automatic?: AutoSyncTrigger | null,
  ) => void;
  apply: (event: SyncEvent) => void;
  finish: (summary: SyncSummary | null, error: ApiError | null) => void;
  /** The student pressed Stop; the run ends at its next file, course or download. */
  stopping: boolean;
  /** The last run ended because the student stopped it (not a failure). */
  stoppedByUser: boolean;
  requestStop: () => void;
  /** Hide the "sync failed" message (it stays hidden until the next run). */
  dismissRunError: () => void;
  /** "Hide" on the last run's result: nothing of it stays on screen. */
  hideRun: () => void;
  /** The student opened, fronted or changed something in the app just now. */
  noteStudentAction: () => void;
  reset: () => void;
}

/** How long after the student's action an answer to "what's due?" counts as attended. */
export const ATTENDED_WINDOW_MS = 30_000;

/** The least time between two automatic starts, and after a Stop, whatever the answers say. */
export const AUTO_SYNC_MIN_GAP_MS = 30 * 60 * 1000;

/** No run to show: before the first one, after "Hide", after an automatic run that went wrong. */
const noRun = {
  running: false,
  total: null,
  order: [],
  bySource: {},
  lastSummary: null,
  runError: null,
  downloadCourseId: null,
  stopping: false,
  stoppedByUser: false,
  automatic: null,
} satisfies Partial<SyncState>;

const idle = {
  ...noRun,
  noAutomaticBefore: 0,
  automaticProblem: false,
  watched: false,
  attendedUntil: 0,
} satisfies Partial<SyncState>;

/** Problems a later automatic sync can't fix: the student has to replace or re-add something. */
function needsTheStudent(kind: SourceErrorKind | null | undefined): boolean {
  return kind === "auth_expired_or_revoked" || kind === "not_found";
}

export const useSyncStore = create<SyncState>()((set) => ({
  ...idle,
  begin: (total, downloadCourseId = null, automatic = null) =>
    set((state) => ({
      running: true,
      total,
      order: [],
      bySource: {},
      runError: null,
      downloadCourseId,
      stopping: false,
      stoppedByUser: false,
      automatic,
      automaticProblem: false,
      noAutomaticBefore: automatic ? Date.now() + AUTO_SYNC_MIN_GAP_MS : state.noAutomaticBefore,
    })),
  apply: (event) =>
    set((state) => {
      const prev = state.bySource[event.source_id];
      const base: SourceProgress = prev ?? {
        sourceId: event.source_id,
        label: event.source_id,
        message: null,
        stage: null,
        course: null,
        current: null,
        total: null,
        warnings: [],
        result: null,
        stopped: false,
      };
      let next: SourceProgress;
      switch (event.type) {
        case "source_started":
          next = { ...base, label: event.label };
          break;
        case "progress":
          next = {
            ...base,
            message: event.message,
            stage: event.stage ?? null,
            course: event.course ?? null,
            current: event.current ?? null,
            total: event.total ?? null,
          };
          break;
        case "warning":
          next = { ...base, warnings: [...base.warnings, event.message] };
          break;
        case "source_finished":
          next = {
            ...base,
            result: {
              ok: event.ok,
              error: event.error ?? null,
              errorKind: event.error_kind ?? null,
            },
          };
          break;
      }
      return {
        order: prev ? state.order : [...state.order, event.source_id],
        bySource: { ...state.bySource, [event.source_id]: next },
      };
    }),
  finish: (summary, error) =>
    set((state) => {
      // Stopped by the student (cancel_sync): not a failure. The source it stopped in reports
      // `ok: false` without an error kind; show it as stopped, like the ones never reached.
      const stoppedByUser = error?.kind === "cancelled";
      // An automatic sync the student didn't ask for stays quiet when it goes wrong: refused
      // (another sync, an update installing), not due any more (an empty answer), or a source
      // failed. Nothing of the run stays on screen; what the student must fix is on the source
      // itself. Stopping it, or watching it in the capsule, makes it end like any other run
      // (a run with no event yet has no capsule to watch: what the student is in is an older one).
      if (state.automatic && !stoppedByUser && (!state.watched || state.order.length === 0)) {
        const sources = Object.values(state.bySource);
        const clean =
          !error &&
          summary !== null &&
          summary.ok &&
          summary.results.length > 0 &&
          sources.every((p) => p.result === null || p.result.ok);
        if (!clean) {
          return {
            ...noRun,
            automaticProblem:
              (summary?.results.some((r) => !r.ok && needsTheStudent(r.error_kind)) ?? false) ||
              sources.some((p) => needsTheStudent(p.result?.errorKind)),
          };
        }
      }
      return {
        running: false,
        downloadCourseId: null,
        lastSummary: summary,
        runError: stoppedByUser ? null : error,
        stopping: false,
        stoppedByUser,
        // The student said "not now": PageLamp doesn't start one by itself right afterwards.
        noAutomaticBefore: stoppedByUser
          ? Date.now() + AUTO_SYNC_MIN_GAP_MS
          : state.noAutomaticBefore,
        // Sources that never reported back didn't run to the end: mark them stopped so no
        // spinner keeps going after the run is over.
        bySource: Object.fromEntries(
          Object.entries(state.bySource).map(([id, p]) => [
            id,
            p.result && !(stoppedByUser && !p.result.ok && !p.result.errorKind)
              ? p
              : { ...p, result: null, stopped: true },
          ]),
        ),
      };
    }),
  requestStop: () => set({ stopping: true }),
  dismissRunError: () => set({ runError: null }),
  hideRun: () => set(noRun),
  noteStudentAction: () => set({ attendedUntil: Date.now() + ATTENDED_WINDOW_MS }),
  reset: () => set(idle),
}));

/** "2 of 3 sources done" for the running sync; total is null when unknown. */
export function useSyncCounts(): { done: number; total: number | null } {
  const total = useSyncStore((s) => s.total);
  const done = useSyncStore((s) => s.order.filter((id) => s.bySource[id]?.result).length);
  return { done, total };
}

/**
 * "Download & index files" for one Canvas course. It is a sync of that course's source, so it
 * shares the sync progress store (and the one-sync-at-a-time rule). Resolves false when another
 * run is already active.
 */
export function useDownloadCourseFiles() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useCallback(
    async (courseId: string): Promise<boolean> => {
      const store = useSyncStore.getState();
      if (store.running) return false;
      store.begin(1, courseId);
      const onEvent = (event: SyncEvent) => useSyncStore.getState().apply(event);
      try {
        const result = await api.downloadCourseFiles(courseId, onEvent);
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(
          {
            started_at: result.started_at,
            finished_at: result.finished_at,
            ok: result.ok,
            results: [result],
          },
          null,
        );
      } catch (error) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(null, toApiError(error));
      }
      return true;
    },
    [api, queryClient],
  );
}

/**
 * Start a sync of every source (or one source). Resolves `true` when this call ran a sync (it
 * never rejects — failures land in the store: `runError`, per-source `result`), or `false`
 * when it did nothing because another run in this window was already active.
 *
 * `automatic` marks a sync of every source that PageLamp starts by itself (useAutoSync is the
 * only caller): the facade is told the trigger, and a run that goes wrong stays quiet.
 */
export function useStartSync() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useCallback(
    async (sourceId?: string, options?: { automatic?: AutoSyncTrigger }): Promise<boolean> => {
      const store = useSyncStore.getState();
      if (store.running) return false;
      const automatic = sourceId ? null : (options?.automatic ?? null);
      // An automatic run leaves out the sources only the student can fix.
      const known = queryClient
        .getQueryData<AppStatus>(queryKeys.status())
        ?.sources.filter((s) => !automatic || !needsTheStudent(s.last_error_kind)).length;
      store.begin(sourceId ? 1 : (known ?? null), null, automatic);
      const onEvent = (event: SyncEvent) => useSyncStore.getState().apply(event);
      try {
        let summary: SyncSummary;
        if (sourceId) {
          const result = await api.syncSource(sourceId, {}, onEvent);
          summary = {
            started_at: result.started_at,
            finished_at: result.finished_at,
            ok: result.ok,
            results: [result],
          };
        } else {
          summary = await api.syncAll(automatic ? { automatic } : {}, onEvent);
        }
        // Refresh data BEFORE marking the run finished, so no screen briefly mistakes a
        // status fetched during our own run (sync_in_progress: true) for another process.
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(summary, null);
      } catch (error) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(null, toApiError(error));
      }
      return true;
    },
    [api, queryClient],
  );
}

/**
 * Runs `action` once, when this window's current run has ended (at once if none is running).
 * For work that had to wait for a run the student didn't start. Not from inside the store's
 * notification: whoever shows the run must see it end before the next one begins.
 */
export function afterCurrentRun(action: () => void) {
  if (!useSyncStore.getState().running) {
    action();
    return;
  }
  const unsubscribe = useSyncStore.subscribe((state) => {
    if (state.running) return;
    unsubscribe();
    setTimeout(action, 0);
  });
}

/**
 * Stop this window's running sync or download (cancel_sync). The run ends with the sources it
 * didn't finish marked stopped. A sync in another process (the CLI) can't be stopped from here.
 */
export function useStopSync() {
  const api = useApi();
  return useCallback(async () => {
    const store = useSyncStore.getState();
    if (!store.running || store.stopping) return;
    store.requestStop();
    try {
      await api.cancelSync();
    } catch {
      useSyncStore.setState({ stopping: false });
    }
  }, [api]);
}

/**
 * Is a sync running? `running`: started from this window. `external`: another process (e.g.
 * the CLI) holds the sync lock, as the status query reports. `busy`: either — a new sync would
 * fail with "busy" right now.
 */
export function useSyncActivity() {
  const status = useStatus();
  const running = useSyncStore((s) => s.running);
  const external = !running && status.data?.sync_in_progress === true;
  return { running, external, busy: running || external, sources: status.data?.sources ?? [] };
}

/**
 * When another process's sync ends, the courses, sources and deadlines it wrote are new: refresh
 * everything, not just the status. Mount once, in the app shell.
 */
export function useRefreshAfterExternalSync() {
  const queryClient = useQueryClient();
  const { external } = useSyncActivity();
  const wasExternal = useRef(false);
  useEffect(() => {
    if (wasExternal.current && !external && !useSyncStore.getState().running) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.all });
    }
    wasExternal.current = external;
  }, [external, queryClient]);
}
