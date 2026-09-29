// Installing the ChatGPT plan's Codex runtime (M2): ≈70 MB, so the download goes on while the
// student leaves Settings, and the card shows how far it got when they come back. What is
// installed and who is signed in is the facade's (useCodexStatus); this store only tracks the
// running install, and never holds anything from the sign-in.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { create } from "zustand";
import { aiKeys } from "@/api/ai-queries";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";

export type CodexInstallState =
  | { phase: "idle" }
  | { phase: "downloading"; installId: string; downloaded: number; total: number | null }
  | { phase: "verifying"; installId: string }
  | { phase: "installing"; installId: string }
  | { phase: "failed"; error: ApiError };

export const useCodexStore = create<{ install: CodexInstallState }>()(() => ({
  install: { phase: "idle" },
}));

export function isInstalling(install: CodexInstallState): boolean {
  return install.phase !== "idle" && install.phase !== "failed";
}

/** Download, verify and install the pinned Codex; a second call while one runs does nothing. */
export function useInstallCodex() {
  const api = useApi();
  const client = useQueryClient();
  return useCallback(async () => {
    if (isInstalling(useCodexStore.getState().install)) return;
    const installId = crypto.randomUUID();
    const set = (install: CodexInstallState) => useCodexStore.setState({ install });
    set({ phase: "downloading", installId, downloaded: 0, total: null });
    try {
      await api.installCodex(installId, (event) => {
        switch (event.type) {
          case "download_started":
            set({ phase: "downloading", installId, downloaded: 0, total: event.total_bytes });
            break;
          case "progress":
            set({
              phase: "downloading",
              installId,
              downloaded: event.downloaded_bytes,
              total: event.total_bytes,
            });
            break;
          case "verifying":
            set({ phase: "verifying", installId });
            break;
          case "installing":
            set({ phase: "installing", installId });
            break;
          case "done":
            break;
        }
      });
      set({ phase: "idle" });
    } catch (error) {
      const e = toApiError(error);
      // Cancelling is the student's choice, not a failure.
      set(e.kind === "cancelled" ? { phase: "idle" } : { phase: "failed", error: e });
    } finally {
      await client.invalidateQueries({ queryKey: aiKeys.all });
    }
  }, [api, client]);
}

/** Stop the running download; the partial file is deleted by the facade. */
export function useCancelCodexInstall() {
  const api = useApi();
  return useCallback(async () => {
    const install = useCodexStore.getState().install;
    if (isInstalling(install) && "installId" in install) {
      await api.cancelCodexInstall(install.installId);
    }
  }, [api]);
}
