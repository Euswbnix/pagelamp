import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import type { GenStage } from "@/api/ai";
import { useApi } from "@/api/context";
import { toApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";
import type { CalendarProposal } from "@/api/types";

export type ReadingState =
  | { phase: "idle" }
  | {
      phase: "running";
      backend: string | null;
      model: string | null;
      /** An on-device model: nothing leaves this computer. */
      onDevice: boolean;
      stage: GenStage | null;
      stopping: boolean;
    }
  | { phase: "done"; proposal: CalendarProposal }
  | { phase: "stopped" }
  | { phase: "failed"; error: unknown };

/**
 * The one-time question (b) reminder (D37 option 2) a run said, however it ended: kept apart
 * from the run, so a new run doesn't drop it before it was answered or dismissed.
 */
export interface SharingReminder {
  /** Where the text went. */
  service: string;
}

/**
 * One "Read the syllabus with AI" run (calendar design §7.3): its GenEvents as state, Stop
 * (cancel_generation), and a refresh of everything afterwards (the proposal, usage, the cap).
 */
export function useCalendarReading(courseId: string) {
  const api = useApi();
  const client = useQueryClient();
  const [state, setState] = useState<ReadingState>({ phase: "idle" });
  const [reminder, setReminder] = useState<SharingReminder | null>(null);
  const runId = useRef<string | null>(null);

  const start = useCallback(
    async (overrideBudget: boolean) => {
      if (runId.current) return;
      const id = crypto.randomUUID();
      runId.current = id;
      let backend: string | null = null;
      let reminded = false;
      setState({
        phase: "running",
        backend: null,
        model: null,
        onDevice: false,
        stage: null,
        stopping: false,
      });
      try {
        const proposal = await api.readCourseCalendar(
          courseId,
          id,
          { override_budget: overrideBudget },
          (event) => {
            if (event.type === "started") {
              backend = event.backend_label;
              const { backend_label, model, on_device } = event;
              setState((s) =>
                s.phase === "running"
                  ? { ...s, backend: backend_label, model, onDevice: on_device }
                  : s,
              );
            } else if (event.type === "stage") {
              const { stage } = event;
              setState((s) => (s.phase === "running" ? { ...s, stage } : s));
            } else if (event.type === "notice" && event.code === "material_sharing_reminder") {
              reminded = true;
            }
          },
        );
        if (reminded || proposal.sharing_reminder) {
          setReminder({ service: backend ?? proposal.ai_label?.backend_label ?? "" });
        }
        setState({ phase: "done", proposal });
      } catch (error) {
        // The facade claims the reminder once the text went out, also on a run that then fails
        // or is stopped (the notice comes before it ends): it is shown then too.
        if (reminded) setReminder({ service: backend ?? "" });
        setState(
          toApiError(error).kind === "cancelled"
            ? { phase: "stopped" }
            : { phase: "failed", error },
        );
      } finally {
        runId.current = null;
        await client.invalidateQueries({ queryKey: queryKeys.all });
      }
    },
    [api, client, courseId],
  );

  const stop = useCallback(async () => {
    const id = runId.current;
    if (!id) return;
    setState((s) => (s.phase === "running" ? { ...s, stopping: true } : s));
    await api.cancelGeneration(id);
  }, [api]);

  /** Close the reminder (answered or dismissed). */
  const settle = useCallback(() => setReminder(null), []);

  return { state, reminder, start, stop, settle };
}
