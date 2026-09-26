// Live sync progress, shared by the sidebar pill, the Sources screen and onboarding.
// Holds only what SyncEvents carry (labels, progress messages, error kinds) — no secrets.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { create } from "zustand";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";
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
  finish: (summary, error) => set({ running: false, lastSummary: summary, runError: error }),
  reset: () => set(idle),
}));

/** "2 of 3 sources done" for the running sync; total is null when unknown. */
export function useSyncCounts(): { done: number; total: number | null } {
  const total = useSyncStore((s) => s.total);
  const done = useSyncStore((s) => s.order.filter((id) => s.bySource[id]?.result).length);
  return { done, total };
}

/**
 * Start a sync of every source (or one source). Returns a promise that resolves when the run
 * ends; it never rejects — failures land in the store (`runError`, per-source `result`).
 */
export function useStartSync() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useCallback(
    async (sourceId?: string) => {
      const store = useSyncStore.getState();
      if (store.running) return;
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
    },
    [api, queryClient],
  );
}
