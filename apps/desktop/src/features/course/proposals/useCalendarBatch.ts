import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import type { GenStage } from "@/api/ai";
import { useApi } from "@/api/context";
import { toApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";
import type { CalendarRunOutcome } from "@/api/types";

export type BatchState =
  | { phase: "idle" }
  | {
      phase: "running";
      courseId: string | null;
      index: number;
      total: number;
      stage: GenStage | null;
      stopping: boolean;
      outcomes: CalendarRunOutcome[];
    }
  | { phase: "finished"; outcomes: CalendarRunOutcome[]; stopped: boolean }
  | { phase: "failed"; error: unknown };

/**
 * "Read syllabi for N courses" (calendar design §7.3): one course after another, each gated on
 * its own; progress per course, Stop for the rest, and every course's outcome at the end.
 */
export function useCalendarBatch() {
  const api = useApi();
  const client = useQueryClient();
  const [state, setState] = useState<BatchState>({ phase: "idle" });
  const batchId = useRef<string | null>(null);

  const start = useCallback(
    async (courseIds: string[], overrideBudget: boolean) => {
      if (batchId.current || courseIds.length === 0) return;
      const id = crypto.randomUUID();
      batchId.current = id;
      const outcomes: CalendarRunOutcome[] = [];
      setState({
        phase: "running",
        courseId: null,
        index: 0,
        total: courseIds.length,
        stage: null,
        stopping: false,
        outcomes,
      });
      try {
        const all = await api.readCourseCalendars(
          courseIds,
          id,
          { override_budget: overrideBudget },
          (event) => {
            if (event.type === "course_started") {
              const { course_id, index, total } = event;
              setState((s) =>
                s.phase === "running"
                  ? { ...s, courseId: course_id, index, total, stage: null }
                  : s,
              );
            } else if (event.type === "gen" && event.event.type === "stage") {
              const { stage } = event.event;
              setState((s) => (s.phase === "running" ? { ...s, stage } : s));
            } else if (event.type === "course_finished") {
              outcomes.push(event.outcome);
              setState((s) => (s.phase === "running" ? { ...s, outcomes: [...outcomes] } : s));
            }
          },
        );
        const stopped = all.some((o) => o.error === "cancelled") || all.length < courseIds.length;
        setState({ phase: "finished", outcomes: all, stopped });
      } catch (error) {
        setState(
          toApiError(error).kind === "cancelled"
            ? { phase: "finished", outcomes, stopped: true }
            : { phase: "failed", error },
        );
      } finally {
        batchId.current = null;
        await client.invalidateQueries({ queryKey: queryKeys.all });
      }
    },
    [api, client],
  );

  const stop = useCallback(async () => {
    const id = batchId.current;
    if (!id) return;
    setState((s) => (s.phase === "running" ? { ...s, stopping: true } : s));
    await api.cancelGeneration(id);
  }, [api]);

  const reset = useCallback(() => setState({ phase: "idle" }), []);

  return { state, start, stop, reset };
}
