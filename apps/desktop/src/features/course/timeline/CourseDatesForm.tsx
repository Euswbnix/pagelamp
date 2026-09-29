import { Plus, Trash2 } from "lucide-react";
import { type FormEvent, type Ref, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type ApiError, toApiError } from "@/api/errors";
import { useSetCourseDates } from "@/api/removalQueries";
import type {
  BreakKind,
  Course,
  CourseDatesInput,
  CourseTimeline,
  TermResolution,
} from "@/api/types";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const BREAK_KINDS: BreakKind[] = ["reading_week", "holiday", "winter_break", "other"];

interface BreakRow {
  key: number;
  kind: BreakKind;
  start: string;
  end: string;
  numbered: boolean;
  label: string;
}

interface DatesState {
  first: string;
  last: string;
  examsEnd: string;
  breaks: BreakRow[];
  hasSecond: boolean;
  secondFirst: string;
  secondLast: string;
  restart: boolean;
}

/** The form's starting point: the dates the resolver uses now (never a Canvas window it set aside). */
function initialState(term: TermResolution): DatesState {
  const [first, second] = term.teaching;
  return {
    first: first?.first_class ?? term.week_one_monday ?? "",
    last: first?.last_class ?? "",
    examsEnd: term.exams_end ?? "",
    breaks: term.breaks.map((b, i) => ({
      key: i,
      kind: b.kind,
      start: b.span.start,
      end: b.span.end,
      numbered: b.numbered,
      label: b.label,
    })),
    hasSecond: !!second,
    secondFirst: second?.first_class ?? "",
    secondLast: second?.last_class ?? "",
    restart: second ? second.first_week_number <= 1 : false,
  };
}

/** Dates in order, and every break ending on or after its start (the facade checks the rest). */
function inOrder(s: DatesState): boolean {
  const chain = [
    s.first,
    s.last,
    ...(s.hasSecond ? [s.secondFirst, s.secondLast] : []),
    s.examsEnd,
  ].filter(Boolean);
  const chainOk = chain.every((d, i) => i === 0 || (chain[i - 1] ?? d) <= d);
  const secondOk = !s.hasSecond || !s.last || !s.secondFirst || s.secondFirst > s.last;
  return chainOk && secondOk && s.breaks.every((b) => b.start && b.end && b.start <= b.end);
}

function toInput(s: DatesState): CourseDatesInput {
  return {
    first_class: s.first || null,
    last_class: s.last || null,
    exams_end: s.examsEnd || null,
    breaks: s.breaks.map((b) => ({
      kind: b.kind,
      start: b.start,
      end: b.end,
      numbered: b.numbered,
      label: b.label.trim() || null,
    })),
    second_segment:
      s.hasSecond && s.secondFirst
        ? {
            first_class: s.secondFirst,
            last_class: s.secondLast || null,
            restart_numbering: s.restart,
          }
        : null,
  };
}

/**
 * The course dates form v2 (calendar design §7.10; F2): first and last day of classes, end of
 * exams, breaks, and a second part for full-year courses. Prefilled from the resolver; saving
 * writes the student's calendar (the top authority), "Clear my dates" removes it.
 */
export function CourseDatesForm({
  course,
  timeline,
  startRef,
  edit,
}: {
  course: Course;
  timeline: CourseTimeline;
  startRef?: Ref<HTMLInputElement>;
  /**
   * Edit a proposal instead (F3): start from its dates, and "save" accepts it with the edits.
   * No "Clear my dates" here; Cancel closes the form.
   */
  edit?: {
    term: TermResolution;
    title: string;
    saveLabel: string;
    savedMessage: string;
    save: (dates: CourseDatesInput) => Promise<void>;
    onCancel: () => void;
  };
}) {
  const { t } = useTranslation("calendar");
  const { t: tc } = useTranslation();
  const ids = {
    heading: useId(),
    first: useId(),
    last: useId(),
    exams: useId(),
    examsHint: useId(),
    breaks: useId(),
    second: useId(),
    error: useId(),
  };
  const mutation = useSetCourseDates();
  const saveRef = useRef<HTMLButtonElement>(null);
  const addBreakRef = useRef<HTMLButtonElement>(null);
  const nextKey = useRef(1000);
  const term = edit?.term ?? timeline.term;
  const own = !edit && term.anchor === "student_confirmed";

  const initial = initialState(term);
  const initialKey = JSON.stringify(initial);
  const [state, setState] = useState(initial);
  const [shown, setShown] = useState(initialKey);
  // When the dates in force change (saved, cleared, synced), start again from them.
  if (shown !== initialKey) {
    setShown(initialKey);
    setState(initial);
  }
  const [outOfOrder, setOutOfOrder] = useState(false);

  // A proposal can be accepted as shown; the student's own dates only once changed.
  const changed = !!edit || JSON.stringify({ ...state }) !== initialKey;
  const [saveError, setSaveError] = useState<ApiError | null>(null);
  const [pending, setPending] = useState(false);
  const apiError = saveError;
  const invalid = outOfOrder || apiError?.kind === "invalid";

  function update(patch: Partial<DatesState>) {
    if (saveError) setSaveError(null);
    setOutOfOrder(false);
    setState((s) => ({ ...s, ...patch }));
  }

  function updateBreak(key: number, patch: Partial<BreakRow>) {
    update({ breaks: state.breaks.map((b) => (b.key === key ? { ...b, ...patch } : b)) });
  }

  function addBreak() {
    nextKey.current += 1;
    const key = nextKey.current;
    update({
      breaks: [
        ...state.breaks,
        { key, kind: "reading_week", start: "", end: "", numbered: false, label: "" },
      ],
    });
    // Focus the new row's first date field once it exists.
    requestAnimationFrame(() => document.getElementById(`break-${key}-start`)?.focus());
  }

  function removeBreak(key: number) {
    update({ breaks: state.breaks.filter((b) => b.key !== key) });
    addBreakRef.current?.focus();
  }

  async function save(dates: CourseDatesInput | null, message: string) {
    setPending(true);
    try {
      if (edit && dates) await edit.save(dates);
      else await mutation.mutateAsync({ courseId: course.id, dates });
      toast.success(message);
    } catch (error) {
      // Shown inline below.
      setSaveError(toApiError(error));
    } finally {
      setPending(false);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!changed || pending) return;
    if (!inOrder(state)) {
      setOutOfOrder(true);
      return;
    }
    void save(toInput(state), edit ? edit.savedMessage : t("form.saved"));
  }

  const describedBy = (...extra: (string | false)[]) =>
    [...extra, invalid && ids.error].filter(Boolean).join(" ") || undefined;
  const dateInput = (
    id: string,
    value: string,
    onChange: (value: string) => void,
    options: { min?: string; hint?: string; ref?: Ref<HTMLInputElement> } = {},
  ) => (
    <Input
      ref={options.ref}
      id={id}
      type="date"
      value={value}
      min={options.min || undefined}
      onChange={(event) => onChange(event.target.value)}
      aria-invalid={invalid || undefined}
      aria-describedby={describedBy(options.hint ?? false)}
    />
  );

  return (
    <form onSubmit={submit} aria-labelledby={ids.heading} noValidate className="space-y-5">
      <div className="space-y-1">
        <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
          {edit ? edit.title : t("form.title")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("form2.description")}</p>
        {!own && (initial.first || initial.last) ? (
          <p className="text-sm text-muted-foreground">{t("form.prefilled")}</p>
        ) : null}
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.first}>{t("form.start")}</FieldLabel>
          {dateInput(ids.first, state.first, (first) => update({ first }), { ref: startRef })}
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.last}>{t("form.end")}</FieldLabel>
          {/* No "three weeks after" hint here: the end of exams (next field) says it. */}
          {dateInput(ids.last, state.last, (last) => update({ last }), { min: state.first })}
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.exams}>{t("form2.exams")}</FieldLabel>
          {dateInput(ids.exams, state.examsEnd, (examsEnd) => update({ examsEnd }), {
            min: state.secondLast || state.last,
            hint: ids.examsHint,
          })}
          <FieldDescription id={ids.examsHint}>{t("form2.examsHint")}</FieldDescription>
        </Field>
      </div>

      <section aria-labelledby={ids.breaks} className="space-y-3">
        <div className="space-y-1">
          <h3 id={ids.breaks} className="text-sm font-medium">
            {t("form2.breaks")}
          </h3>
          <p className="text-sm text-muted-foreground">{t("form2.breaksHint")}</p>
        </div>
        {state.breaks.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("form2.noBreaks")}</p>
        ) : (
          state.breaks.map((row, i) => (
            <BreakFields
              key={row.key}
              row={row}
              index={i + 1}
              invalid={invalid}
              describedBy={describedBy()}
              onChange={(patch) => updateBreak(row.key, patch)}
              onRemove={() => removeBreak(row.key)}
            />
          ))
        )}
        <Button ref={addBreakRef} type="button" variant="outline" size="sm" onClick={addBreak}>
          <Plus aria-hidden />
          {t("form2.addBreak")}
        </Button>
      </section>

      <div className="space-y-3">
        <div className="flex items-center gap-3">
          <Checkbox
            id={ids.second}
            checked={state.hasSecond}
            onCheckedChange={(value) => update({ hasSecond: value === true })}
          />
          <Label htmlFor={ids.second}>{t("form2.secondPart")}</Label>
        </div>
        {state.hasSecond ? (
          <SecondPart state={state} invalid={invalid} update={update} dateInput={dateInput} />
        ) : null}
      </div>

      {invalid ? (
        <FieldError id={ids.error}>
          {outOfOrder || !apiError?.message ? (
            t("form2.invalid")
          ) : (
            <>
              {t("form2.invalid")} <span lang="en">{apiError.message}</span>
            </>
          )}
        </FieldError>
      ) : apiError ? (
        <FieldError id={ids.error}>{tc(`errors.${apiError.kind}`)}</FieldError>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <Button
          ref={saveRef}
          type="submit"
          aria-disabled={!changed || pending}
          className="aria-disabled:opacity-50"
        >
          {pending ? tc("actions.saving") : edit ? edit.saveLabel : t("form.save")}
        </Button>
        {edit ? (
          <Button type="button" variant="outline" onClick={edit.onCancel}>
            {tc("actions.cancel")}
          </Button>
        ) : null}
        {own ? (
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={() => {
              // The button disappears once the student's dates are gone; focus moves to Save.
              saveRef.current?.focus();
              void save(null, t("form.cleared"));
            }}
          >
            {t("form.clear")}
          </Button>
        ) : null}
      </div>
    </form>
  );
}

function BreakFields({
  row,
  index,
  invalid,
  describedBy,
  onChange,
  onRemove,
}: {
  row: BreakRow;
  index: number;
  invalid: boolean;
  describedBy?: string;
  onChange: (patch: Partial<BreakRow>) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation("calendar");
  const ids = { kind: useId(), end: useId(), label: useId(), numbered: useId() };
  const startId = `break-${row.key}-start`;
  return (
    <fieldset className="relative space-y-3 rounded-lg border p-3">
      {/* The legend must be the fieldset's first child to name the group. */}
      <legend className="float-left pt-1 text-sm font-medium">
        {t("form2.breakLegend", { index })}
      </legend>
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        className="absolute top-2 right-2"
        aria-label={t("form2.removeBreak", { index })}
        onClick={onRemove}
      >
        <Trash2 aria-hidden />
      </Button>
      <div className="clear-both" />
      <div className="grid gap-3 sm:grid-cols-3">
        <Field>
          <FieldLabel htmlFor={ids.kind}>{t("form2.breakKind")}</FieldLabel>
          <Select value={row.kind} onValueChange={(kind) => onChange({ kind: kind as BreakKind })}>
            <SelectTrigger id={ids.kind}>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {BREAK_KINDS.map((kind) => (
                <SelectItem key={kind} value={kind}>
                  {t(`breakKind.${kind}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={startId}>{t("form2.breakStart")}</FieldLabel>
          <Input
            id={startId}
            type="date"
            value={row.start}
            onChange={(event) => onChange({ start: event.target.value })}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
          />
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.end}>{t("form2.breakEnd")}</FieldLabel>
          <Input
            id={ids.end}
            type="date"
            value={row.end}
            min={row.start || undefined}
            onChange={(event) => onChange({ end: event.target.value })}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy}
          />
        </Field>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 sm:items-end">
        <Field>
          <FieldLabel htmlFor={ids.label}>{t("form2.breakLabel")}</FieldLabel>
          <Input
            id={ids.label}
            value={row.label}
            maxLength={80}
            onChange={(event) => onChange({ label: event.target.value })}
          />
        </Field>
        <div className="flex items-center gap-3 pb-2">
          <Checkbox
            id={ids.numbered}
            checked={row.numbered}
            onCheckedChange={(value) => onChange({ numbered: value === true })}
          />
          <Label htmlFor={ids.numbered}>{t("form2.breakNumbered")}</Label>
        </div>
      </div>
    </fieldset>
  );
}

function SecondPart({
  state,
  invalid,
  update,
  dateInput,
}: {
  state: DatesState;
  invalid: boolean;
  update: (patch: Partial<DatesState>) => void;
  dateInput: (
    id: string,
    value: string,
    onChange: (value: string) => void,
    options?: { min?: string },
  ) => React.ReactNode;
}) {
  const { t } = useTranslation("calendar");
  const ids = { first: useId(), last: useId(), numbering: useId() };
  return (
    <fieldset className="space-y-3 rounded-lg border p-3">
      <legend className="text-sm font-medium">{t("form2.secondLegend")}</legend>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.first}>{t("form.start")}</FieldLabel>
          {dateInput(ids.first, state.secondFirst, (secondFirst) => update({ secondFirst }), {
            min: state.last,
          })}
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.last}>{t("form.end")}</FieldLabel>
          {dateInput(ids.last, state.secondLast, (secondLast) => update({ secondLast }), {
            min: state.secondFirst,
          })}
        </Field>
      </div>
      <div className="space-y-2">
        <p id={ids.numbering} className="text-sm font-medium">
          {t("form2.numbering")}
        </p>
        <RadioGroup
          aria-labelledby={ids.numbering}
          value={state.restart ? "restart" : "continue"}
          onValueChange={(value) => update({ restart: value === "restart" })}
        >
          {(["continue", "restart"] as const).map((value) => (
            <div key={value} className="flex items-center gap-2">
              <RadioGroupItem id={`${ids.numbering}-${value}`} value={value} />
              <Label htmlFor={`${ids.numbering}-${value}`} className="font-normal">
                {t(`form2.${value}`)}
              </Label>
            </div>
          ))}
        </RadioGroup>
      </div>
    </fieldset>
  );
}
