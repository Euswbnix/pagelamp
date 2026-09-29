import { ScanSearch } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { EstimateRequest } from "@/api/ai";
import { useCostEstimate } from "@/api/ai-queries";
import { useCourseCalendar, useScanCourseCalendar } from "@/api/proposalQueries";
import type { Course } from "@/api/types";
import { Button } from "@/components/ui/button";
import { GenerateButton } from "@/features/ai/GenerateButton";
import {
  MaterialSharingReminder,
  SharingNotAllowedNotice,
} from "@/features/ai/MaterialSharingNotices";
import { useAiErrorText } from "@/features/ai/useAiErrorText";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { translateWithText } from "../timeline/evidence";
import { COURSE_BLOCKS, useBlockText } from "./blockText";
import { type ReadingState, useCalendarReading } from "./useCalendarReading";

/**
 * The two ways to read dates from the candidates (calendar design §7.2, §7.3): the scan (no
 * model, nothing sent) and "Read the syllabus with AI" with its cost, the gate's reasons, the
 * progress with Stop, and the question (b) reminder after a course's first cloud run. Both only
 * propose: the proposal card at the top of the tab is where the student decides.
 */
export function ReadSyllabus({ course }: { course: Course }) {
  const { t } = useTranslation("proposals");
  const view = useCourseCalendar(course.id);
  const blockText = useBlockText();
  const reading = useCalendarReading(course.id);
  const request: EstimateRequest = { feature: "course_calendar", courses: [course.id] };
  const estimate = useCostEstimate(request);
  const aiHeadingId = useId();
  const name = course.code ?? course.name;

  const block = view.data?.blocked ?? null;
  const courseBlock = block !== null && COURSE_BLOCKS.has(block) ? block : null;
  const sharingNotAllowed = estimate.data?.would_block === "material_sharing_not_allowed";
  const { state } = reading;

  return (
    <div className="space-y-6">
      <ScanForDates courseId={course.id} unavailable={block === "no_readable_materials"} />

      <section aria-labelledby={aiHeadingId} className="space-y-3">
        <div className="space-y-1">
          <h3 id={aiHeadingId} className="text-sm font-medium">
            {t("reading.aiTitle")}
          </h3>
          <p className="text-sm text-muted-foreground">{t("reading.aiHint")}</p>
        </div>
        {courseBlock ? (
          <p className="text-sm">{blockText(courseBlock)}</p>
        ) : sharingNotAllowed ? (
          <SharingNotAllowedNotice courseId={course.id} courseName={name} />
        ) : state.phase === "running" ? (
          <ReadingProgress state={state} onStop={() => void reading.stop()} />
        ) : (
          <GenerateButton
            request={request}
            label={t("reading.readWithAi")}
            onGenerate={({ overrideBudget }) => void reading.start(overrideBudget)}
          />
        )}
        <ReadingOutcome state={state} />
        {state.phase === "done" && state.reminder ? (
          <MaterialSharingReminder
            courseId={course.id}
            courseName={name}
            service={state.backend}
            onClose={reading.settle}
          />
        ) : null}
      </section>
    </div>
  );
}

/** "Reading with <backend> · <model>", the stage, and Stop. */
function ReadingProgress({
  state,
  onStop,
}: {
  state: Extract<ReadingState, { phase: "running" }>;
  onStop: () => void;
}) {
  const { t } = useTranslation("proposals");
  return (
    <div className="space-y-2 text-sm">
      <p className="font-medium">
        {state.backend && state.model
          ? // The backend label and model id come from outside the translations: plain text.
            translateWithText(
              t,
              "reading.running",
              {},
              { backend: state.backend, model: state.model },
            )
          : t("reading.starting")}
      </p>
      {state.stage ? (
        <p className="text-muted-foreground">{t(`reading.stage.${state.stage}`)}</p>
      ) : null}
      <Button
        type="button"
        size="sm"
        variant="outline"
        onClick={onStop}
        aria-disabled={state.stopping || undefined}
        className="aria-disabled:opacity-50"
      >
        {state.stopping ? t("reading.stopping") : t("reading.stop")}
      </Button>
    </div>
  );
}

/**
 * How the run ended. A live region that always exists, so screen readers hear the start and the
 * end (not every stage); a failure is an alert.
 */
function ReadingOutcome({ state }: { state: ReadingState }) {
  const { t } = useTranslation("proposals");
  const errorText = useAiErrorText();
  let text = "";
  if (state.phase === "running") text = t("reading.readingNow");
  else if (state.phase === "done") text = t("reading.done");
  else if (state.phase === "stopped") text = t("reading.stopped");
  return (
    <>
      <p role="status" className={state.phase === "running" ? "sr-only" : "text-sm"}>
        {text}
      </p>
      {state.phase === "failed" ? (
        <div role="alert" className="text-sm">
          <p className="font-medium">{t("reading.failed")}</p>
          <p className="text-muted-foreground">{errorText(state.error)}</p>
        </div>
      ) : null}
    </>
  );
}

/** "Find dates without AI": the deterministic scan (design §7.2). */
function ScanForDates({ courseId, unavailable }: { courseId: string; unavailable: boolean }) {
  const { t } = useTranslation("proposals");
  const scan = useScanCourseCalendar();
  const errorText = useApiErrorText();
  const hintId = useId();
  const [result, setResult] = useState<"found" | "nothing" | null>(null);

  async function run() {
    if (scan.isPending || unavailable) return;
    setResult(null);
    try {
      const proposal = await scan.mutateAsync({ courseId });
      setResult(proposal ? "found" : "nothing");
    } catch {
      // Shown below (scan.error).
    }
  }

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          type="button"
          variant="outline"
          onClick={() => void run()}
          aria-disabled={unavailable || scan.isPending || undefined}
          aria-describedby={hintId}
          className="aria-disabled:opacity-50"
        >
          <ScanSearch aria-hidden />
          {scan.isPending ? t("reading.scanning") : t("reading.scan")}
        </Button>
      </div>
      <p id={hintId} className="text-sm text-muted-foreground">
        {t("reading.scanHint")}
      </p>
      <p role="status" className="text-sm">
        {result === "found"
          ? t("reading.scanFound")
          : result === "nothing"
            ? t("reading.scanNothing")
            : ""}
      </p>
      {scan.error ? (
        <p role="alert" className="text-sm">
          {errorText(scan.error)}
        </p>
      ) : null}
    </div>
  );
}
