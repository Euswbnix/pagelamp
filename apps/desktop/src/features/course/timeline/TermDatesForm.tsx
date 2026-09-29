import { type FormEvent, type Ref, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { toApiError } from "@/api/errors";
import { useSetCourseTerm } from "@/api/queries";
import type { Course, CourseTimeline } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

/**
 * "Course dates": the first and last day of classes, saved per course (calendar design §7.10;
 * alpha.1 form). Prefilled from the dates the resolver uses now — never from a Canvas term it
 * set aside. Only the fields the student changes are saved: an untouched field keeps the
 * student's own earlier date (or none), so the prefill never becomes an override by itself.
 * The last day of classes is an end-only anchor: the exam period follows it.
 */
export function TermDatesForm({
  course,
  timeline,
  startRef,
}: {
  course: Course;
  timeline: CourseTimeline;
  /** The first date field, for "Edit dates" in the check-dates prompt. */
  startRef?: Ref<HTMLInputElement>;
}) {
  const { t } = useTranslation("calendar");
  const { t: tc } = useTranslation();
  const ids = { heading: useId(), start: useId(), end: useId(), endHint: useId(), error: useId() };
  const { term } = timeline;
  const own = { start: term.student_start ?? null, end: term.student_end ?? null };
  const initial = {
    start: own.start ?? term.teaching[0]?.first_class ?? term.week_one_monday ?? "",
    end: own.end ?? term.teaching.at(-1)?.last_class ?? "",
  };
  const prefilled = (!own.start && !!initial.start) || (!own.end && !!initial.end);
  const [start, setStart] = useState(initial.start);
  const [end, setEnd] = useState(initial.end);
  const mutation = useSetCourseTerm();
  const saveRef = useRef<HTMLButtonElement>(null);

  // When the dates in force change (after saving or clearing), start again from them. Done
  // here instead of re-keying the form, so focus stays in the date field after Enter.
  const [shown, setShown] = useState(initial);
  if (shown.start !== initial.start || shown.end !== initial.end) {
    setShown(initial);
    setStart(initial.start);
    setEnd(initial.end);
  }

  const changed = start !== initial.start || end !== initial.end;
  // Checked here too: with only one field saved, the facade can't see the other one on screen.
  const [endsBeforeStart, setEndsBeforeStart] = useState(false);
  const errorKind = endsBeforeStart
    ? "invalid"
    : mutation.error
      ? toApiError(mutation.error).kind
      : null;
  const invalid = errorKind === "invalid";

  // mutateAsync + try/catch: the toast fires once the refreshed course is back.
  async function save(dates: { start: string | null; end: string | null }, message: string) {
    try {
      await mutation.mutateAsync({ courseId: course.id, ...dates });
      toast.success(message);
    } catch {
      // Shown inline below (mutation.error).
    }
  }

  // Editing a date clears the previous error message.
  function edit(set: (value: string) => void, value: string) {
    if (mutation.isError) mutation.reset();
    setEndsBeforeStart(false);
    set(value);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!changed || mutation.isPending) return;
    if (start && end && end < start) {
      setEndsBeforeStart(true);
      return;
    }
    void save(
      {
        start: start !== initial.start ? start || null : own.start,
        end: end !== initial.end ? end || null : own.end,
      },
      t("form.saved"),
    );
  }

  const describedBy = (extra?: string) =>
    [extra, errorKind ? ids.error : null].filter(Boolean).join(" ") || undefined;

  return (
    <form onSubmit={submit} aria-labelledby={ids.heading} noValidate className="space-y-4">
      <div className="space-y-1">
        <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
          {t("form.title")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("form.description")}</p>
        {prefilled ? <p className="text-sm text-muted-foreground">{t("form.prefilled")}</p> : null}
        <p className="text-sm text-muted-foreground">{t("form.breaksNote")}</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.start}>{t("form.start")}</FieldLabel>
          <Input
            ref={startRef}
            id={ids.start}
            type="date"
            value={start}
            onChange={(event) => edit(setStart, event.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy()}
          />
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.end}>{t("form.end")}</FieldLabel>
          <Input
            id={ids.end}
            type="date"
            value={end}
            min={start || undefined}
            onChange={(event) => edit(setEnd, event.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy(ids.endHint)}
          />
          <FieldDescription id={ids.endHint}>{t("form.endHint")}</FieldDescription>
        </Field>
      </div>

      {errorKind ? (
        <FieldError id={ids.error}>
          {invalid ? t("form.invalid") : tc(`errors.${errorKind}`)}
        </FieldError>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <Button
          ref={saveRef}
          type="submit"
          aria-disabled={!changed || mutation.isPending}
          className="aria-disabled:opacity-50"
        >
          {mutation.isPending ? tc("actions.saving") : t("form.save")}
        </Button>
        {own.start || own.end ? (
          <Button
            type="button"
            variant="outline"
            disabled={mutation.isPending}
            onClick={() => {
              // The button disappears once the student's dates are gone; focus moves to Save.
              saveRef.current?.focus();
              void save({ start: null, end: null }, t("form.cleared"));
            }}
          >
            {t("form.clear")}
          </Button>
        ) : null}
      </div>
    </form>
  );
}
