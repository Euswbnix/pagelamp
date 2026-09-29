import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { EstimateRequest } from "@/api/ai";
import type { WeeklyExplanation } from "@/api/explain";
import { useWeekMaterials } from "@/api/queries";
import type { CourseOverview } from "@/api/types";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { GenerateButton } from "@/features/ai/GenerateButton";
import { MaterialSharingReminder } from "@/features/ai/MaterialSharingNotices";
import { useAiErrorText } from "@/features/ai/useAiErrorText";
import { formatDate } from "@/lib/format";
import { useFocusOnMount } from "@/lib/useFocusOnMount";
import { ExplanationView } from "./ExplanationView";
import { type ExplainRunState, useExplanation, useSavedExplanations } from "./useExplanation";

/**
 * Course → Explain (design §5.2, §7): a week, "≈ $x" and Explain, the run's stages with Stop
 * (no text arrives before the end), the explanation with its citations, and the last 5 of the
 * week. Disabled with the course's reason when its materials aren't readable.
 */
export function ExplainTab({
  overview,
  onOpenPolicy,
}: {
  overview: CourseOverview;
  onOpenPolicy: () => void;
}) {
  const { t, i18n } = useTranslation("explain");
  const { course, timeline } = overview;
  const current = timeline.current_week ?? null;
  const [picked, setPicked] = useState<number | null>(null);
  const weeks = useWeekMaterials(course.id, null);
  const week = picked ?? timeline.default_week ?? current;
  const saved = useSavedExplanations(course.id, week);
  const run = useExplanation(course.id);
  const [shownId, setShownId] = useState<string | null>(null);
  const [reminderClosed, setReminderClosed] = useState<string | null>(null);
  const weekLabelId = useId();
  const resultRef = useRef<HTMLDivElement>(null);
  const { state } = run;
  // A run's explanation replaces its progress (and Stop): the focus goes to the result.
  useEffect(() => {
    if (state.phase === "done") resultRef.current?.focus();
  }, [state.phase]);

  const blocked = course.hidden
    ? "course_hidden"
    : overview.ai_materials !== "readable"
      ? overview.ai_materials
      : null;
  if (blocked) {
    return (
      <div className="space-y-3">
        <h3 className="text-sm font-medium">{t("title")}</h3>
        <p className="text-sm">{t(`blocked.${blocked}`)}</p>
        {blocked !== "course_hidden" ? (
          <Button type="button" variant="outline" size="sm" onClick={onOpenPolicy}>
            {t("openPolicy")}
          </Button>
        ) : null}
      </div>
    );
  }

  const list = saved.data ?? [];
  const fresh = state.phase === "done" ? state.explanation : null;
  const shown: WeeklyExplanation | null =
    list.find((e) => e.meta.generation_id === shownId) ??
    (fresh && fresh.week === week ? fresh : null) ??
    list[0] ??
    null;
  const request: EstimateRequest | null =
    week === null ? null : { feature: "weekly_explanation", course: course.id, week };
  const start = (include: string[], overrideBudget = false) => {
    setShownId(null);
    void run.start(week, { overrideBudget, include, uiLanguage: i18n.language });
  };

  return (
    <div className="space-y-6">
      <div className="space-y-3">
        <div className="space-y-1">
          <h3 className="text-sm font-medium">{t("title")}</h3>
          <p className="text-sm text-muted-foreground">{t("hint")}</p>
        </div>
        <div className="flex flex-wrap items-end gap-3">
          <div className="space-y-1">
            <p id={weekLabelId} className="text-xs font-medium">
              {t("week")}
            </p>
            <Select
              value={week === null ? "" : String(week)}
              onValueChange={(value) => {
                setPicked(Number(value));
                setShownId(null);
              }}
              disabled={state.phase === "running"}
            >
              <SelectTrigger aria-labelledby={weekLabelId} className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(weeks.data?.available_weeks ?? (week ? [week] : [])).map((w) => (
                  <SelectItem key={w} value={String(w)}>
                    {w === current ? t("thisWeek", { week: w }) : t("weekOption", { week: w })}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        {state.phase === "running" ? (
          <ExplainProgress state={state} onStop={() => void run.stop()} />
        ) : (
          <GenerateButton
            request={request}
            label={week === null ? t("title") : t("generate", { week })}
            onGenerate={({ overrideBudget }) => start([], overrideBudget)}
          />
        )}
        <ExplainOutcome state={state} />
      </div>

      {shown ? (
        <section
          ref={resultRef}
          tabIndex={-1}
          aria-label={t("result.regionLabel", { week: shown.week ?? week ?? "" })}
          className="space-y-4 border-t pt-4 outline-none"
        >
          {shown.stale ? (
            <div className="flex flex-wrap items-center gap-3 text-sm">
              <p>{t("result.stale")}</p>
              <Button type="button" size="sm" variant="outline" onClick={() => start([])}>
                {t("result.regenerate")}
              </Button>
            </div>
          ) : null}
          <ExplanationView explanation={shown} onIncludeLeftOut={(ids) => start(ids)} />
          {shown.sharing_reminder && reminderClosed !== shown.meta.generation_id ? (
            <MaterialSharingReminder
              courseId={course.id}
              courseName={course.code ?? course.name}
              service={shown.meta.backend_label}
              onClose={() => setReminderClosed(shown.meta.generation_id)}
            />
          ) : null}
        </section>
      ) : null}

      {list.length > 1 ? (
        <section className="space-y-2">
          <h4 className="text-sm font-medium">{t("history.title")}</h4>
          <ul className="divide-y border-y text-sm">
            {list.map((item) => {
              const isShown = item.meta.generation_id === shown?.meta.generation_id;
              return (
                <li
                  key={item.meta.generation_id}
                  className="flex items-center justify-between gap-3 py-1.5"
                >
                  <span>
                    {formatDate(item.meta.created_at, i18n.language)}
                    <span className="text-muted-foreground"> · {item.meta.model}</span>
                  </span>
                  {isShown ? (
                    <span className="text-xs text-muted-foreground">{t("history.showing")}</span>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      aria-label={`${t("history.show")} ${formatDate(item.meta.created_at, i18n.language)}`}
                      onClick={() => setShownId(item.meta.generation_id)}
                    >
                      {t("history.show")}
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

/** "Writing with <backend> · <model>", how many materials are read, the stage, and Stop. */
function ExplainProgress({
  state,
  onStop,
}: {
  state: Extract<ExplainRunState, { phase: "running" }>;
  onStop: () => void;
}) {
  const { t } = useTranslation("explain");
  // The button that started the run is gone: Stop takes the focus.
  const stopRef = useFocusOnMount<HTMLButtonElement>();
  return (
    <div className="space-y-2 text-sm">
      <p className="font-medium">
        {state.backend && state.model
          ? t("running.withModel", { backend: state.backend, model: state.model })
          : t("running.starting")}
      </p>
      {state.materials !== null ? (
        <p className="text-muted-foreground">{t("running.reading", { count: state.materials })}</p>
      ) : null}
      {state.stage ? (
        <p className="text-muted-foreground" aria-hidden>
          {t(`running.stage.${state.stage}`)}
        </p>
      ) : null}
      <Button
        ref={stopRef}
        type="button"
        size="sm"
        variant="outline"
        onClick={onStop}
        aria-disabled={state.stopping || undefined}
        className="aria-disabled:opacity-50"
      >
        {state.stopping ? t("running.stopping") : t("running.stop")}
      </Button>
    </div>
  );
}

/** The live region: stages and how the run ended (never text); a failure is an alert. */
function ExplainOutcome({ state }: { state: ExplainRunState }) {
  const { t } = useTranslation("explain");
  const errorText = useAiErrorText();
  const statusRef = useRef<HTMLParagraphElement>(null);
  const alertRef = useRef<HTMLDivElement>(null);
  // A stop or a failure brings Explain back: the focus goes to what happened.
  useEffect(() => {
    if (state.phase === "failed") alertRef.current?.focus();
    else if (state.phase === "stopped") statusRef.current?.focus();
  }, [state.phase]);
  const text =
    state.phase === "running"
      ? state.stage
        ? t(`running.stage.${state.stage}`)
        : t("running.writingNow")
      : state.phase === "done"
        ? t("result.done")
        : state.phase === "stopped"
          ? t("result.stopped")
          : "";
  return (
    <>
      <p
        ref={statusRef}
        tabIndex={-1}
        role="status"
        className={state.phase === "stopped" ? "text-sm outline-none" : "sr-only"}
      >
        {text}
      </p>
      {state.phase === "failed" ? (
        <div ref={alertRef} tabIndex={-1} role="alert" className="text-sm outline-none">
          <p className="font-medium">{t("result.failed")}</p>
          <p className="text-muted-foreground">{errorText(state.error)}</p>
        </div>
      ) : null}
    </>
  );
}
