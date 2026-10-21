import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import type { GenStage } from "@/api/ai";
import { useApi } from "@/api/context";
import { toApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";
import type { CalendarRunOutcome } from "@/api/types";
import { useSharingReminders } from "@/stores/sharingReminders";

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

/** A course's one-time question (b) reminder (D37 option 2), said during a batch. */
export interface CourseReminder {
  courseId: string;
  /** Where the course's text went. */
  service: string;
}

/**
 * "Read syllabi for N courses" (calendar design §7.3): one course after another, each gated on
 * its own; progress per course, Stop for the rest, and every course's outcome at the end. The
 * reminders the courses said are kept apart from the batch (also after it failed, was stopped
 * or was put away), each until it is answered or dismissed.
 */
export function useCalendarBatch() {
  const api = useApi();
  const client = useQueryClient();
  const [state, setState] = useState<BatchState>({ phase: "idle" });
  const pending = useSharingReminders((s) => s.pending);
  const addReminder = useSharingReminders((s) => s.add);
  const settleReminder = useSharingReminders((s) => s.settle);
  // The batches' reminders still to answer, in the order said (on the course's page too).
  const reminders: CourseReminder[] = Object.entries(pending)
    .filter(([, r]) => r.batch)
    .map(([courseId, r]) => ({ courseId, service: r.service }));
  const batchId = useRef<string | null>(null);

  const start = useCallback(
    async (courseIds: string[], overrideBudget: boolean) => {
      if (batchId.current || courseIds.length === 0) return;
      const id = crypto.randomUUID();
      batchId.current = id;
      const outcomes: CalendarRunOutcome[] = [];
      // Where each course's run went, for its reminder.
      const services = new Map<string, string>();
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
            } else if (event.type === "gen" && event.event.type === "started") {
              services.set(event.course_id, event.event.backend_label);
            } else if (event.type === "gen" && event.event.type === "stage") {
              const { stage } = event.event;
              setState((s) => (s.phase === "running" ? { ...s, stage } : s));
            } else if (
              event.type === "gen" &&
              event.event.type === "notice" &&
              event.event.code === "material_sharing_reminder"
            ) {
              // Said before the course's end, however its run ends.
              const service = services.get(event.course_id) ?? "";
              addReminder(event.course_id, { service, batch: true });
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
    [api, client, addReminder],
  );

  const stop = useCallback(async () => {
    const id = batchId.current;
    if (!id) return;
    setState((s) => (s.phase === "running" ? { ...s, stopping: true } : s));
    await api.cancelGeneration(id);
  }, [api]);

  const reset = useCallback(() => setState({ phase: "idle" }), []);

  /** Close a course's reminder (answered or dismissed), here and on its page. */
  const settle = settleReminder;

  return { state, reminders, start, stop, reset, settle };
}
