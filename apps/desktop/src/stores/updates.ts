// State of this launch's update check and install (M0.4). Preferences and what's due at launch
// live in the facade (api/queries.ts); this store only remembers what the check found and how
// far an install got, so the notice, Settings and the install dialog agree.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { create } from "zustand";
import type { AvailableUpdate } from "@/api/client";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";

export type InstallState =
  | { phase: "idle" }
  | { phase: "downloading"; downloaded: number; total: number | null }
  | { phase: "installing" }
  | { phase: "restarting" }
  | { phase: "failed"; error: ApiError };

interface UpdateState {
  /** What the last check of this launch found; null = nothing (or not checked yet). */
  available: AvailableUpdate | null;
  checking: boolean;
  /** The last check failed (this launch). */
  checkError: ApiError | null;
  /** The last check of this launch finished (either way). */
  checked: boolean;
  /** "Later" on the update notice: hidden until the next launch or the next check. */
  noticeDismissed: boolean;
  install: InstallState;
  /**
   * The version this launch was updated from (post-update banner). Kept from the first
   * startup_tasks answer, which a later refetch may no longer report.
   */
  updatedFrom: string | null;
  updatedDismissed: boolean;
  dismissNotice: () => void;
  dismissUpdated: () => void;
  reset: () => void;
}

const initial = {
  available: null,
  checking: false,
  checkError: null,
  checked: false,
  noticeDismissed: false,
  install: { phase: "idle" },
  updatedFrom: null,
  updatedDismissed: false,
} satisfies Partial<UpdateState>;

export const useUpdateStore = create<UpdateState>()((set) => ({
  ...initial,
  dismissNotice: () => set({ noticeDismissed: true }),
  dismissUpdated: () => set({ updatedDismissed: true }),
  reset: () => set(initial),
}));

/** Check the effective channel now. Resolves what was found (never rejects). */
export function useCheckForUpdate() {
  const api = useApi();
  const client = useQueryClient();
  return useCallback(async (): Promise<AvailableUpdate | null> => {
    if (useUpdateStore.getState().checking) return useUpdateStore.getState().available;
    useUpdateStore.setState({ checking: true, checkError: null });
    try {
      const available = await api.checkForUpdate();
      useUpdateStore.setState({ available, checked: true, noticeDismissed: false });
      return available;
    } catch (error) {
      useUpdateStore.setState({ checkError: toApiError(error), checked: true });
      return null;
    } finally {
      useUpdateStore.setState({ checking: false });
      void client.invalidateQueries({ queryKey: queryKeys.lastUpdateCheck() });
    }
  }, [api, client]);
}

/**
 * Download and install the update the last check found, then restart (Rust does the restart;
 * on Windows the installer closes PageLamp). Only called after the student confirmed.
 */
export function useInstallUpdate() {
  const api = useApi();
  return useCallback(async () => {
    const { install } = useUpdateStore.getState();
    if (install.phase !== "idle" && install.phase !== "failed") return;
    useUpdateStore.setState({ install: { phase: "downloading", downloaded: 0, total: null } });
    try {
      await api.installUpdate((event) => {
        switch (event.type) {
          case "download_started":
            useUpdateStore.setState({
              install: { phase: "downloading", downloaded: 0, total: event.total_bytes ?? null },
            });
            break;
          case "progress":
            useUpdateStore.setState({
              install: {
                phase: "downloading",
                downloaded: event.downloaded_bytes,
                total: event.total_bytes ?? null,
              },
            });
            break;
          case "installing":
            useUpdateStore.setState({ install: { phase: "installing" } });
            break;
          case "restarting":
            useUpdateStore.setState({ install: { phase: "restarting" } });
            break;
        }
      });
    } catch (error) {
      useUpdateStore.setState({ install: { phase: "failed", error: toApiError(error) } });
    }
  }, [api]);
}
