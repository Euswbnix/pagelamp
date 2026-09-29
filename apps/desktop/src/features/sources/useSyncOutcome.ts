import { useSyncStore } from "@/stores/sync";

/**
 * Where the latest sync run stands, derived from the live sync store:
 * - idle: nothing has run since the app started (or the result was dismissed)
 * - failed: the whole run stopped (e.g. `busy`); per-source failures are `doneWithErrors`
 * - stopped: the student stopped it (Stop); not a failure
 */
export type SyncOutcome = "idle" | "running" | "done" | "doneWithErrors" | "failed" | "stopped";

export function useSyncOutcome(): SyncOutcome {
  const running = useSyncStore((s) => s.running);
  const runError = useSyncStore((s) => s.runError);
  const summary = useSyncStore((s) => s.lastSummary);
  const stoppedByUser = useSyncStore((s) => s.stoppedByUser);
  const anyFailed = useSyncStore((s) =>
    Object.values(s.bySource).some((p) => p.result !== null && !p.result.ok),
  );
  if (running) return "running";
  if (stoppedByUser) return "stopped";
  if (runError) return "failed";
  if (summary) return summary.ok && !anyFailed ? "done" : "doneWithErrors";
  return "idle";
}
