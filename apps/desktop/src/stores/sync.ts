// Live sync progress, shared by the sidebar pill, the Sources screen and onboarding.
// Holds only what SyncEvents carry (labels, progress messages, error kinds) — no secrets.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef } from "react";
import { create } from "zustand";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { queryKeys, useStatus } from "@/api/queries";
import type { AppStatus, SourceErrorKind, SyncEvent, SyncSummary } from "@/api/types";

export interface SourceProgress {
  sourceId: string;
  label: string;
  message: string | null;
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
  begin: (total: number | null) => void;
  apply: (event: SyncEvent) => void;
  finish: (summary: SyncSummary | null, error: ApiError | null) => void;
  /** Hide the "sync failed" message (it stays hidden until the next run). */
  dismissRunError: () => void;
  reset: () => void;
}

const idle = {
  running: false,
  total: null,
  order: [],
  bySource: {},
  lastSummary: null,
  runError: null,
} satisfies Partial<SyncState>;

export const useSyncStore = create<SyncState>()((set) => ({
  ...idle,
  begin: (total) => set({ running: true, total, order: [], bySource: {}, runError: null }),
  apply: (event) =>
    set((state) => {
      const prev = state.bySource[event.source_id];
      const base: SourceProgress = prev ?? {
        sourceId: event.source_id,
        label: event.source_id,
        message: null,
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
    set((state) => ({
      running: false,
      lastSummary: summary,
      runError: error,
      // Sources that never reported back didn't run to the end: mark them stopped so no
      // spinner keeps going after the run is over.
      bySource: Object.fromEntries(
        Object.entries(state.bySource).map(([id, p]) => [
          id,
          p.result ? p : { ...p, stopped: true },
        ]),
      ),
    })),
  dismissRunError: () => set({ runError: null }),
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
      store.begin(1);
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
 */
export function useStartSync() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useCallback(
    async (sourceId?: string): Promise<boolean> => {
      const store = useSyncStore.getState();
      if (store.running) return false;
      const known = queryClient.getQueryData<AppStatus>(queryKeys.status())?.sources.length;
      store.begin(sourceId ? 1 : (known ?? null));
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
          summary = await api.syncAll({}, onEvent);
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
