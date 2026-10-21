import { CircleAlert, CircleCheck, CircleMinus, FileSearch } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { toast } from "sonner";
import { type EstimateRequest, materialSharing } from "@/api/ai";
import { useCostEstimate } from "@/api/ai-queries";
import {
  useAcceptPassingProposals,
  useSnoozeCalendarOffers,
  useSyllabusReadingOffers,
} from "@/api/proposalQueries";
import { useCourses } from "@/api/queries";
import type { CalendarRunOutcome, CourseSummary } from "@/api/types";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { GenerateButton, GenerateSteps } from "@/features/ai/GenerateButton";
import { MaterialSharingReminder } from "@/features/ai/MaterialSharingNotices";
import { useAiErrorText } from "@/features/ai/useAiErrorText";
import { focusPageHeading } from "@/lib/focus";
import { paths } from "@/lib/routes";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { translateWithText } from "../timeline/evidence";
import { useBlockText } from "./blockText";
import { type BatchState, useCalendarBatch } from "./useCalendarBatch";

/**
 * "Read syllabi for N courses" on the Courses page (calendar design §7.3): the courses the facade
 * offers (Current, Upcoming or Unknown, without an accepted calendar), the total "≈ $x" first,
 * then one course after another with Stop, and each course's outcome with a link to check it.
 * Proposals that passed every check can be accepted together; the rest wait on their course.
 * "Not now" hides the offers for 14 days, like the lifecycle banner.
 */
export function ReadSyllabiBatch() {
  const { t } = useTranslation("proposals");
  const offers = useSyllabusReadingOffers();
  const courses = useCourses();
  const batch = useCalendarBatch();
  const snooze = useSnoozeCalendarOffers();
  const errorText = useApiErrorText();
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const byId = new Map((courses.data ?? []).map((c) => [c.course.id, c]));
  const { state, reminders, settle } = batch;
  // Question (b) answered meanwhile for a course (from outside, as the courses are read
  // again): its reminder is done. A course not listed yet is still open.
  useEffect(() => {
    for (const r of reminders) {
      const course = byId.get(r.courseId)?.course;
      const answer = course ? materialSharing(course) : "unanswered";
      if (answer === "allowed" || answer === "not_allowed") settle(r.courseId);
    }
  });

  const ids = offers.data?.map((o) => o.course_id) ?? [];
  const request: EstimateRequest | null =
    ids.length > 0 ? { feature: "course_calendar", courses: ids } : null;
  const estimate = useCostEstimate(request);
  // A reminder still to answer keeps the card.
  const quiet = state.phase === "idle" && reminders.length === 0;
  if (quiet && ids.length === 0) return null;
  // §7.12: no model, no nagging on the home screen (each Timeline tab says how to set it up).
  if (quiet && estimate.data?.would_block === "no_model_chosen") return null;

  async function notNow() {
    if (snooze.isPending) return;
    try {
      await snooze.mutateAsync();
      toast.success(t("batch.snoozed"));
      // The button goes, and the card with it unless reminders keep it: continue from the card's
      // heading then, else from the page heading.
      if (reminders.length > 0) heading.current?.focus();
      else focusPageHeading();
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return (
    <Card aria-labelledby={headingId} role="region">
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FileSearch className="size-4 text-muted-foreground" aria-hidden />
          <h2 id={headingId} ref={heading} tabIndex={-1} className="outline-none">
            {t("batch.title")}
          </h2>
        </CardTitle>
        {state.phase === "idle" && ids.length > 0 ? (
          <CardDescription>{t("batch.description", { count: ids.length })}</CardDescription>
        ) : null}
        {/* Nothing to snooze on a card that only its reminders keep. */}
        {state.phase === "idle" && ids.length > 0 ? (
          <CardAction>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              // Starts with the visible text; tells it apart from the lifecycle banner's "Not now".
              aria-label={t("batch.notNowLabel")}
              aria-disabled={snooze.isPending || undefined}
              className="aria-disabled:opacity-50"
              onClick={() => void notNow()}
            >
              {t("batch.notNow")}
            </Button>
          </CardAction>
        ) : null}
      </CardHeader>
      <CardContent className="space-y-4">
        {/* First: while idle it is only its (out of flow) status line, so the steps' box below
            is the last one and adds no gap under Generate. */}
        <BatchProgress state={state} byId={byId} onStop={() => void batch.stop()} />
        <GenerateSteps fallbackFocus={() => heading.current}>
          {state.phase === "idle" && ids.length > 0 ? (
            <GenerateButton
              request={request}
              label={t("batch.read", { count: ids.length })}
              onGenerate={({ overrideBudget }) => void batch.start(ids, overrideBudget)}
            />
          ) : null}
        </GenerateSteps>
        {state.phase === "finished" ? (
          <BatchResults
            outcomes={state.outcomes}
            stopped={state.stopped}
            byId={byId}
            onDone={batch.reset}
          />
        ) : null}
        <BatchFailure state={state} />
        {reminders.map((r) => (
          <MaterialSharingReminder
            key={r.courseId}
            courseId={r.courseId}
            courseName={courseLabel(byId, r.courseId)}
            service={r.service}
            onClose={() => {
              settle(r.courseId);
              // Not to the page: back to the card, from where the next one is a Tab away; to the
              // page's main region when the card went with it.
              requestAnimationFrame(() =>
                (heading.current ?? document.getElementById("main"))?.focus(),
              );
            }}
          />
        ))}
      </CardContent>
    </Card>
  );
}

function courseLabel(byId: Map<string, CourseSummary>, id: string | null): string {
  if (!id) return "";
  const c = byId.get(id)?.course;
  return c ? (c.code ?? c.name) : id;
}

/** "Reading 2 of 5 · DEM332", the stage and Stop; announced when it starts, not per stage. */
function BatchProgress({
  state,
  byId,
  onStop,
}: {
  state: BatchState;
  byId: Map<string, CourseSummary>;
  onStop: () => void;
}) {
  const { t } = useTranslation("proposals");
  const running = state.phase === "running" ? state : null;
  return (
    <>
      <p role="status" className="sr-only">
        {running ? t("batch.started", { count: running.total }) : ""}
      </p>
      {running ? (
        <div className="space-y-2 text-sm">
          <p className="font-medium">
            {running.courseId
              ? // Course codes come from the source: plain text, never interpolated.
                translateWithText(
                  t,
                  "batch.progress",
                  { done: running.index + 1, total: running.total },
                  { course: courseLabel(byId, running.courseId) },
                )
              : t("reading.starting")}
          </p>
          {running.stage ? (
            <p className="text-muted-foreground">{t(`reading.stage.${running.stage}`)}</p>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={onStop}
            aria-disabled={running.stopping || undefined}
            className="aria-disabled:opacity-50"
          >
            {running.stopping ? t("reading.stopping") : t("batch.stop")}
          </Button>
        </div>
      ) : null}
    </>
  );
}

/** Each course's outcome, a link to check it, and "Accept N that passed every check". */
function BatchResults({
  outcomes,
  stopped,
  byId,
  onDone,
}: {
  outcomes: CalendarRunOutcome[];
  stopped: boolean;
  byId: Map<string, CourseSummary>;
  onDone: () => void;
}) {
  const { t } = useTranslation("proposals");
  const blockText = useBlockText();
  const accept = useAcceptPassingProposals();
  const errorText = useApiErrorText();
  const [accepted, setAccepted] = useState<Set<number>>(new Set());
  const passing = outcomes.filter(
    (o): o is CalendarRunOutcome & { proposal_id: number } =>
      o.passing && o.proposal_id != null && !accepted.has(o.proposal_id),
  );
  const made = outcomes.filter((o) => o.proposal_id != null).length;

  async function acceptAll() {
    if (accept.isPending || passing.length === 0) return;
    const ids = passing.map((o) => o.proposal_id);
    try {
      await accept.mutateAsync({ proposalIds: ids });
      setAccepted((s) => new Set([...s, ...ids]));
    } catch {
      // Shown below (accept.error).
    }
  }

  return (
    <div className="space-y-3 text-sm">
      <p role="status">
        {stopped ? t("batch.stopped", { count: made }) : t("batch.finished", { count: made })}
      </p>
      <ul className="divide-y border-t">
        {outcomes.map((o) => {
          const done = o.proposal_id != null && accepted.has(o.proposal_id);
          let icon = <CircleAlert className="size-4 text-warning" aria-hidden />;
          let text: string;
          if (done) {
            icon = <CircleCheck className="size-4 text-success" aria-hidden />;
            text = t("batch.outcome.accepted");
          } else if (o.proposal_id != null) {
            icon = <CircleCheck className="size-4 text-success" aria-hidden />;
            text = o.passing ? t("batch.outcome.passing") : t("batch.outcome.choose");
          } else if (o.blocked) {
            icon = <CircleMinus className="size-4 text-muted-foreground" aria-hidden />;
            text = blockText(o.blocked);
          } else if (o.error === "cancelled") {
            icon = <CircleMinus className="size-4 text-muted-foreground" aria-hidden />;
            text = t("batch.outcome.stopped");
          } else if (o.error) {
            text = t("batch.outcome.failed");
          } else {
            icon = <CircleMinus className="size-4 text-muted-foreground" aria-hidden />;
            text = t("batch.outcome.nothing");
          }
          return (
            <li key={o.course_id} className="flex items-start gap-3 py-2.5">
              <span className="mt-0.5 shrink-0">{icon}</span>
              <div className="min-w-0 flex-1">
                <Link
                  to={`${paths.course(o.course_id)}?tab=timeline`}
                  className="font-medium underline-offset-4 hover:underline"
                >
                  {courseLabel(byId, o.course_id)}
                </Link>
                <p className="text-muted-foreground">{text}</p>
              </div>
            </li>
          );
        })}
      </ul>
      <div className="flex flex-wrap gap-2">
        {passing.length > 0 ? (
          <Button
            type="button"
            onClick={() => void acceptAll()}
            aria-disabled={accept.isPending || undefined}
            className="aria-disabled:opacity-50"
          >
            {t("batch.acceptPassing", { count: passing.length })}
          </Button>
        ) : null}
        <Button type="button" variant="ghost" onClick={onDone}>
          {t("batch.done")}
        </Button>
      </div>
      {accepted.size > 0 ? (
        <p role="status">{t("batch.accepted", { count: accepted.size })}</p>
      ) : null}
      {accept.error ? <p role="alert">{errorText(accept.error)}</p> : null}
    </div>
  );
}

function BatchFailure({ state }: { state: BatchState }) {
  const { t } = useTranslation("proposals");
  const errorText = useAiErrorText();
  if (state.phase !== "failed") return null;
  return (
    <div role="alert" className="text-sm">
      <p className="font-medium">{t("batch.failed")}</p>
      <p className="text-muted-foreground">{errorText(state.error)}</p>
    </div>
  );
}
