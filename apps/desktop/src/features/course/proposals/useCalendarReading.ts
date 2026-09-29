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
      stage: GenStage | null;
      stopping: boolean;
    }
  | {
      phase: "done";
      proposal: CalendarProposal;
      /** Where the text went, for the question (b) reminder. */
      backend: string;
      /** The course's first cloud run: show the one-time reminder (D37 option 2). */
      reminder: boolean;
    }
  | { phase: "stopped" }
  | { phase: "failed"; error: unknown };

/**
 * One "Read the syllabus with AI" run (calendar design §7.3): its GenEvents as state, Stop
 * (cancel_generation), and a refresh of everything afterwards (the proposal, usage, the cap).
 */
export function useCalendarReading(courseId: string) {
  const api = useApi();
  const client = useQueryClient();
  const [state, setState] = useState<ReadingState>({ phase: "idle" });
  const runId = useRef<string | null>(null);

  const start = useCallback(
    async (overrideBudget: boolean) => {
      if (runId.current) return;
      const id = crypto.randomUUID();
      runId.current = id;
      let backend: string | null = null;
      let reminder = false;
      setState({ phase: "running", backend: null, model: null, stage: null, stopping: false });
      try {
        const proposal = await api.readCourseCalendar(
          courseId,
          id,
          { override_budget: overrideBudget },
          (event) => {
            if (event.type === "started") {
              backend = event.backend_label;
              const { backend_label, model } = event;
              setState((s) =>
                s.phase === "running" ? { ...s, backend: backend_label, model } : s,
              );
            } else if (event.type === "stage") {
              const { stage } = event;
              setState((s) => (s.phase === "running" ? { ...s, stage } : s));
            } else if (event.type === "notice" && event.code === "material_sharing_reminder") {
              reminder = true;
            }
          },
        );
        setState({
          phase: "done",
          proposal,
          backend: backend ?? proposal.ai_label?.backend_label ?? "",
          reminder: reminder || proposal.sharing_reminder,
        });
      } catch (error) {
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

  /** Close the result (after the reminder was answered or dismissed). */
  const settle = useCallback(() => {
    setState((s) => (s.phase === "done" ? { ...s, reminder: false } : s));
  }, []);

  return { state, start, stop, settle };
}
