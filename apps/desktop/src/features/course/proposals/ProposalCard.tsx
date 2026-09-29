import { TriangleAlert } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useAcceptCalendarProposal, useDismissCalendarProposal } from "@/api/proposalQueries";
import type {
  AlternativeDate,
  CalendarProposal,
  Course,
  CourseDatesInput,
  CourseTimeline,
  ProposedDate,
} from "@/api/types";
import { AiGeneratedLabel } from "@/components/common/AiGeneratedLabel";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { applyChoice, calendarAsTerm, calendarToInput } from "@/lib/calendarInput";
import { formatDate, formatIsoDate } from "@/lib/format";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { useToday } from "@/lib/useToday";
import { CourseDatesForm } from "../timeline/CourseDatesForm";
import { CalendarStrip } from "./CalendarStrip";
import { changeText } from "./changes";
import { QuoteCard } from "./QuoteCard";

/** A choice between dates for one field: the proposal's own date first, then the others. */
interface Question {
  key: string;
  kind: ProposedDate["kind"];
  segment: number;
  legend: string;
  options: AlternativeDate[];
  /** Conflicts have no default: the student must choose. */
  required: boolean;
}

/**
 * One calendar proposal (scan, AI or the student's AI app; calendar design §7.10): a picture of
 * the term, every date with the words it comes from, the choices where materials disagree or
 * dates conflict, what couldn't be checked, what accepting changes, and Accept / Edit / Dismiss.
 * Proposals never take effect by themselves.
 */
export function ProposalCard({
  proposal,
  course,
  timeline,
  onGone,
}: {
  proposal: CalendarProposal;
  course: Course;
  timeline: CourseTimeline;
  /** After accepting or dismissing (the card goes away): where focus continues. */
  onGone: () => void;
}) {
  const { t, i18n } = useTranslation("proposals");
  const { t: tcal } = useTranslation("calendar");
  const errorText = useApiErrorText();
  const today = useToday();
  const headingId = useId();
  const accept = useAcceptCalendarProposal();
  const dismiss = useDismissCalendarProposal();
  const [editing, setEditing] = useState(false);
  const [answers, setAnswers] = useState<Record<string, number>>({});
  const busy = accept.isPending || dismiss.isPending;
  const date = (iso: string) => formatIsoDate(iso, i18n.language);
  const kindLabel = (d: { kind: ProposedDate["kind"]; segment: number; week?: number | null }) => {
    const kind = t(`card.kind.${d.kind}`, { week: d.week ?? "" });
    return d.segment > 0 ? t("card.secondPart", { kind }) : kind;
  };
  const when = (d: { date: string; end?: string | null }) =>
    d.end ? t("card.span", { start: date(d.date), end: date(d.end) }) : date(d.date);

  const questions: Question[] = [
    ...proposal.dates
      .filter((d) => d.alternatives.length > 0)
      .map((d) => ({
        key: `alt-${d.kind}-${d.segment}-${d.date}`,
        kind: d.kind,
        segment: d.segment,
        legend: `${kindLabel(d)}: ${t("card.choose")}`,
        options: [
          { date: d.date, end: d.end, label: d.label, evidence: d.evidence },
          ...d.alternatives,
        ],
        required: false,
      })),
    // A conflict without options (a date no year fits, too many breaks) is only a notice.
    ...proposal.conflicts
      .filter((c) => c.options.length > 0)
      .map((c, i) => ({
        key: `conflict-${i}`,
        kind: c.kind,
        segment: c.segment,
        legend: `${kindLabel(c)}: ${t(`conflict.${c.code}`)} ${t("card.chooseRight")}`,
        options: c.options,
        required: true,
      })),
  ];
  const unanswered = questions.some((q) => q.required && answers[q.key] === undefined);
  const chose = questions.some(
    (q) => answers[q.key] !== undefined && (q.required || answers[q.key] !== 0),
  );

  /** The proposal with the student's choices applied (null = exactly as proposed). */
  function edits(): CourseDatesInput | null {
    if (!chose) return null;
    let input = calendarToInput(proposal.calendar);
    for (const q of questions) {
      const choice = answers[q.key];
      const option = choice === undefined ? undefined : q.options[choice];
      if (option) input = applyChoice(input, q.kind, q.segment, option);
    }
    return input;
  }

  async function acceptWith(dates: CourseDatesInput | null) {
    await accept.mutateAsync({ proposalId: proposal.id, edits: dates });
    onGone();
  }

  async function onAccept() {
    if (busy || unanswered) return;
    try {
      await acceptWith(edits());
      toast.success(t("card.accepted"));
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  async function onDismiss() {
    if (busy) return;
    try {
      await dismiss.mutateAsync({ proposalId: proposal.id });
      toast.success(t("card.dismissed"));
      onGone();
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  const title =
    proposal.origin === "scan" || proposal.origin === "ai" || proposal.origin === "ai_app"
      ? t(`card.title.${proposal.origin}`)
      : t("card.title.other");
  const label = proposal.ai_label;
  const afterLabel =
    proposal.resulting_phase === "teaching" && proposal.resulting_week_today
      ? tcal("phase.teaching", { week: proposal.resulting_week_today })
      : t(`card.phase.${proposal.resulting_phase}`);
  const dropped = proposal.dropped.reduce((n, d) => n + d.count, 0);

  return (
    <article aria-labelledby={headingId} className="pl-callout space-y-4 p-5 text-card-foreground">
      <header className="space-y-1">
        <h3 id={headingId} className="font-heading text-base font-semibold tracking-tight">
          {title}
        </h3>
        <p className="text-xs text-muted-foreground">
          {t("card.created", { date: formatDate(proposal.created_at, i18n.language) })}
        </p>
        {label ? (
          <>
            <AiGeneratedLabel meta={label} />
            {label.on_device ? (
              <p className="text-xs text-muted-foreground">{t("card.onDevice")}</p>
            ) : null}
          </>
        ) : proposal.origin === "scan" ? (
          <p className="text-xs text-muted-foreground">{t("card.scanNote")}</p>
        ) : null}
      </header>

      <CalendarStrip calendar={proposal.calendar} today={today} />

      {proposal.low_quality ? (
        <Alert role="status">
          <TriangleAlert aria-hidden />
          <AlertDescription>{t("card.lowQuality")}</AlertDescription>
        </Alert>
      ) : null}

      {editing ? (
        <CourseDatesForm
          course={course}
          timeline={timeline}
          edit={{
            term: calendarAsTerm(proposal.calendar),
            title: t("card.editTitle"),
            saveLabel: t("card.editSave"),
            savedMessage: t("card.accepted"),
            save: acceptWith,
            onCancel: () => setEditing(false),
          }}
        />
      ) : (
        <>
          <section className="space-y-2" aria-label={t("card.datesTitle")}>
            <h4 className="text-sm font-medium">{t("card.datesTitle")}</h4>
            <ul className="space-y-3">
              {proposal.dates.map((d) => (
                <li key={`${d.kind}-${d.segment}-${d.date}`} className="space-y-1.5">
                  <p className="text-sm">
                    <span className="font-medium">{kindLabel(d)}</span>
                    {d.kind === "break_span" && d.break_kind ? (
                      <span className="text-muted-foreground">
                        {" "}
                        · {tcal(`breakKind.${d.break_kind}`)}
                      </span>
                    ) : null}
                    {" — "}
                    {when(d)}
                  </p>
                  {d.evidence.map((e) => (
                    <QuoteCard key={`${e.material_id}-${e.quote}`} evidence={e} />
                  ))}
                </li>
              ))}
            </ul>
          </section>

          {proposal.conflicts
            .filter((c) => c.options.length === 0)
            .map((c) => (
              <Alert key={`${c.code}-${c.kind}-${c.segment}`} role="status">
                <TriangleAlert aria-hidden />
                <AlertDescription>
                  {kindLabel(c)}: {t(`conflict.${c.code}`)} {t("card.checkOrEdit")}
                </AlertDescription>
              </Alert>
            ))}

          {questions.map((q) => (
            <ChoiceQuestion
              key={q.key}
              question={q}
              value={answers[q.key] ?? (q.required ? undefined : 0)}
              onChange={(index) => setAnswers((a) => ({ ...a, [q.key]: index }))}
              when={when}
            />
          ))}

          {dropped > 0 ? (
            <div className="space-y-1 text-sm text-muted-foreground">
              <p>{t("card.dropped", { count: dropped })}</p>
              <ul className="list-disc pl-5">
                {proposal.dropped.map((d) => (
                  <li key={d.reason}>{t(`card.droppedReason.${d.reason}`, { count: d.count })}</li>
                ))}
              </ul>
            </div>
          ) : null}

          <section className="space-y-1 text-sm" aria-label={t("card.afterTitle")}>
            <h4 className="font-medium">{t("card.afterTitle")}</h4>
            <p>{t("card.after", { label: afterLabel })}</p>
            {proposal.changes.length > 0 ? (
              <ul className="list-disc pl-5 text-muted-foreground">
                {proposal.changes.map((c) => (
                  <li key={`${c.code}-${c.params.map((p) => p.value).join("|")}`}>
                    {changeText(c, t, tcal, i18n.language)}
                  </li>
                ))}
              </ul>
            ) : null}
          </section>

          <div className="space-y-2">
            {unanswered ? (
              <p id={`${headingId}-choose`} className="text-sm text-muted-foreground">
                {t("card.chooseFirst")}
              </p>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <Button
                aria-disabled={busy || unanswered || undefined}
                aria-describedby={unanswered ? `${headingId}-choose` : undefined}
                className="aria-disabled:opacity-50"
                onClick={() => void onAccept()}
              >
                {chose ? t("card.acceptChoices") : t("card.accept")}
              </Button>
              <Button variant="outline" onClick={() => setEditing(true)}>
                {t("card.edit")}
              </Button>
              <Button
                variant="ghost"
                aria-disabled={busy || undefined}
                onClick={() => void onDismiss()}
              >
                {t("card.dismiss")}
              </Button>
            </div>
          </div>
        </>
      )}
    </article>
  );
}

function ChoiceQuestion({
  question,
  value,
  onChange,
  when,
}: {
  question: Question;
  value: number | undefined;
  onChange: (index: number) => void;
  when: (d: { date: string; end?: string | null }) => string;
}) {
  const { t } = useTranslation("proposals");
  const legendId = useId();
  return (
    <fieldset className="space-y-2 rounded-row bg-muted p-3">
      <legend id={legendId} className="px-1 text-sm font-medium">
        {question.legend}
      </legend>
      <RadioGroup
        aria-labelledby={legendId}
        value={value === undefined ? "" : String(value)}
        onValueChange={(v) => onChange(Number(v))}
      >
        {question.options.map((option, i) => {
          const id = `${legendId}-${i}`;
          const from = option.evidence[0]?.title ?? "";
          return (
            <div key={id} className="space-y-1.5">
              <div className="flex items-center gap-2">
                <RadioGroupItem id={id} value={String(i)} />
                <Label htmlFor={id} className="font-normal">
                  {from
                    ? t("card.optionFrom", { date: when(option), title: from })
                    : t("card.optionOwn", { date: when(option) })}
                </Label>
              </div>
              <div className="pl-6">
                {option.evidence.map((e) => (
                  <QuoteCard key={`${e.material_id}-${e.quote}`} evidence={e} />
                ))}
              </div>
            </div>
          );
        })}
      </RadioGroup>
    </fieldset>
  );
}
