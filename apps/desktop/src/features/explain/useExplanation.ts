import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import type { GenStage } from "@/api/ai";
import { useApi } from "@/api/context";
import { toApiError } from "@/api/errors";
import type { OutputLanguage, WeeklyExplanation } from "@/api/explain";
import { queryKeys } from "@/api/queries";

export const explainKeys = {
  saved: (courseId: string, week: number | null) =>
    [...queryKeys.all, "explanations", courseId, week] as const,
  language: () => [...queryKeys.all, "ai-output-language"] as const,
};

export type ExplainRunState =
  | { phase: "idle" }
  | {
      phase: "running";
      backend: string | null;
      model: string | null;
      stage: GenStage | null;
      /** Materials whose text is read (the `context` event). */
      materials: number | null;
      stopping: boolean;
    }
  | { phase: "done"; explanation: WeeklyExplanation }
  | { phase: "stopped" }
  | { phase: "failed"; error: unknown };

/**
 * One "Explain week N" run (design §5.2, §7): its GenEvents as state (no text arrives before
 * the end), Stop (cancel_generation), and the saved list refreshed after every run.
 */
export function useExplanation(courseId: string) {
  const api = useApi();
  const client = useQueryClient();
  const [state, setState] = useState<ExplainRunState>({ phase: "idle" });
  const runId = useRef<string | null>(null);

  const start = useCallback(
    async (
      week: number | null,
      options: { overrideBudget: boolean; include: string[]; uiLanguage: string },
    ) => {
      if (runId.current) return;
      const id = crypto.randomUUID();
      runId.current = id;
      setState({
        phase: "running",
        backend: null,
        model: null,
        stage: null,
        materials: null,
        stopping: false,
      });
      try {
        const explanation = await api.explainWeek(
          courseId,
          week,
          id,
          {
            include: options.include,
            override_budget: options.overrideBudget,
            ui_language: options.uiLanguage,
          },
          (event) => {
            if (event.type === "started") {
              const { backend_label, model } = event;
              setState((s) =>
                s.phase === "running" ? { ...s, backend: backend_label, model } : s,
              );
            } else if (event.type === "stage") {
              const { stage } = event;
              setState((s) => (s.phase === "running" ? { ...s, stage } : s));
            } else if (event.type === "context") {
              const materials = event.summary.materials_included;
              setState((s) => (s.phase === "running" ? { ...s, materials } : s));
            }
          },
        );
        setState({ phase: "done", explanation });
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

  return { state, start, stop };
}

/** The last 5 explanations of the course's week, newest first. */
export function useSavedExplanations(courseId: string, week: number | null) {
  const api = useApi();
  return useQuery({
    queryKey: explainKeys.saved(courseId, week),
    queryFn: () => api.savedExplanations(courseId, week),
  });
}

export function useOutputLanguage() {
  const api = useApi();
  return useQuery({ queryKey: explainKeys.language(), queryFn: () => api.aiOutputLanguage() });
}

export function useSetOutputLanguage() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: (language: OutputLanguage) => api.setAiOutputLanguage(language),
    onSuccess: (_result, language) => client.setQueryData(explainKeys.language(), language),
  });
}
