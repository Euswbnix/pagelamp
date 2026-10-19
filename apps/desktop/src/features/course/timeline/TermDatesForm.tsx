import { type FormEvent, type Ref, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { toApiError } from "@/api/errors";
import { useSetCourseTerm } from "@/api/queries";
import type { Course, CourseTimeline } from "@/api/types";
import { CODE, SentenceWithCode } from "@/components/common/SentenceWithCode";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SameSessionCourses, useApplyToSession, useSameSession } from "./SameSessionCourses";

/**
 * "Course dates": the first and last day of classes, saved per course (calendar design §7.10;
 * alpha.1 form). Prefilled from the dates the resolver uses now — never from a Canvas term it
 * set aside. Only the fields the student changes are saved: an untouched field keeps the
 * student's own earlier date (or none), so the prefill never becomes an override by itself.
 * The last day of classes is an end-only anchor: the exam period follows it.
 *
 * With other courses of the same session ticked ("also use these dates in"), this course's own
 * dates, as they are once saved, are copied to those courses: never a prefilled date the student
 * didn't touch, here or there.
 *
 * A course with a calendar of its own (accepted, or saved with the dates form v2) counts its
 * weeks from that calendar, which this form can't change: it shows the calendar's first and
 * last day of classes, read-only, says where the calendar is changed, and offers no Save.
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
  const ids = {
    heading: useId(),
    start: useId(),
    end: useId(),
    endHint: useId(),
    error: useId(),
    calendar: useId(),
  };
  const { term } = timeline;
  // From the course's own timeline, so the form never waits for the session listing to know.
  const ownCalendar = term.own_calendar;
  const session = useSameSession(course.id, { ownCalendar });
  const own = { start: term.student_start ?? null, end: term.student_end ?? null };
  // With a calendar of its own, the calendar's days of classes (the term dates there only
  // mirror its span, which can end with the exams).
  const initial = ownCalendar
    ? {
        start: term.teaching[0]?.first_class ?? "",
        end: term.teaching.at(-1)?.last_class ?? "",
      }
    : {
        start: own.start ?? term.teaching[0]?.first_class ?? term.week_one_monday ?? "",
        end: own.end ?? term.teaching.at(-1)?.last_class ?? "",
      };
  const prefilled =
    !ownCalendar && ((!own.start && !!initial.start) || (!own.end && !!initial.end));
  const [start, setStart] = useState(initial.start);
  const [end, setEnd] = useState(initial.end);
  const mutation = useSetCourseTerm();
  const saveRef = useRef<HTMLButtonElement>(null);
  const applyToSession = useApplyToSession(course.id);
  const [applyFailed, setApplyFailed] = useState(false);
  // The save here and the copy to the others, from the first to the last: one Save at a time.
  const [applyPending, setApplyPending] = useState(false);
  const busy = mutation.isPending || applyPending;
  // Dates kept from version 0.1 that the student hasn't confirmed (a last day alone too): given
  // to the others only once saved here, which confirms them here as any Save does.
  const unconfirmed = !term.student_dates_confirmed;

  // Checked here too: with only one field saved, the facade can't see the other one on screen.
  const [endsBeforeStart, setEndsBeforeStart] = useState(false);

  // When the dates in force change (after saving or clearing), start again from them. Done
  // here instead of re-keying the form, so focus stays in the date field after Enter. Turning
  // read-only (a calendar of its own) or back also drops what was typed and its messages.
  const [shown, setShown] = useState({ ...initial, ownCalendar });
  if (
    shown.start !== initial.start ||
    shown.end !== initial.end ||
    shown.ownCalendar !== ownCalendar
  ) {
    if (shown.ownCalendar !== ownCalendar) {
      setEndsBeforeStart(false);
      setApplyFailed(false);
    }
    setShown({ ...initial, ownCalendar });
    setStart(initial.start);
    setEnd(initial.end);
  }

  const changed = start !== initial.start || end !== initial.end;
  // Each field as it is saved: a changed one as typed, an untouched one as the student had it.
  const dates = {
    start: start !== initial.start ? start || null : own.start,
    end: end !== initial.end ? end || null : own.end,
  };
  // What goes to the courses ticked: this course's own dates once saved (none: nothing to give).
  const applying = session.ticked.length > 0 && !!(dates.start || dates.end);
  // Without a change, Save gives the dates the student already has here to the courses ticked.
  const canSave = changed || applying;
  // Read-only, nothing is saved here: no message of an earlier save stays.
  const errorKind = ownCalendar
    ? null
    : endsBeforeStart
      ? "invalid"
      : mutation.error
        ? toApiError(mutation.error).kind
        : applyFailed
          ? "applyFailed"
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

  // Saved here, then given to the courses ticked; one toast says what happened to both.
  async function saveAndApply() {
    const saving = changed || unconfirmed;
    setApplyPending(true);
    try {
      if (saving) await mutation.mutateAsync({ courseId: course.id, ...dates });
      const applied = await applyToSession(session.ticked, {
        saved: saving,
        afterUndo: session.offerAgain,
      });
      if (applied) session.untick(applied);
      else setApplyFailed(true);
    } catch {
      // The save failed: shown inline below (mutation.error), and nothing was given away.
    } finally {
      setApplyPending(false);
    }
  }

  // Editing a date clears the previous error message.
  function edit(set: (value: string) => void, value: string) {
    if (mutation.isError) mutation.reset();
    setEndsBeforeStart(false);
    setApplyFailed(false);
    set(value);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (ownCalendar || !canSave || busy) return;
    if (start && end && end < start) {
      setEndsBeforeStart(true);
      return;
    }
    setApplyFailed(false);
    if (applying) void saveAndApply();
    else void save(dates, t("form.saved"));
  }

  const describedBy = (extra?: string) =>
    [extra, ownCalendar ? ids.calendar : null, errorKind ? ids.error : null]
      .filter(Boolean)
      .join(" ") || undefined;

  return (
    <form onSubmit={submit} aria-labelledby={ids.heading} noValidate className="space-y-4">
      <div className="space-y-1">
        <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
          {t("form.title")}
        </h2>
        {ownCalendar ? null : (
          <p className="text-sm text-muted-foreground">{t("form.description")}</p>
        )}
        {ownCalendar ? (
          <p id={ids.calendar} className="text-sm text-muted-foreground">
            <SentenceWithCode
              text={t("form.ownCalendar", { command: CODE })}
              code="pagelamp course calendar"
            />
          </p>
        ) : (
          <>
            {prefilled ? (
              <p className="text-sm text-muted-foreground">{t("form.prefilled")}</p>
            ) : null}
            <p className="text-sm text-muted-foreground">{t("form.breaksNote")}</p>
          </>
        )}
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.start}>{t("form.start")}</FieldLabel>
          <Input
            ref={startRef}
            id={ids.start}
            type="date"
            value={start}
            readOnly={ownCalendar}
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
            readOnly={ownCalendar}
            onChange={(event) => edit(setEnd, event.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={describedBy(ownCalendar ? undefined : ids.endHint)}
          />
          {/* The calendar has its own end of exams: the three weeks after don't apply there. */}
          {ownCalendar ? null : (
            <FieldDescription id={ids.endHint}>{t("form.endHint")}</FieldDescription>
          )}
        </Field>
      </div>

      <SameSessionCourses choice={session} disabled={busy} />

      {errorKind ? (
        <FieldError id={ids.error}>
          {invalid
            ? t("form.invalid")
            : errorKind === "applyFailed"
              ? t("sameSession.failed")
              : tc(`errors.${errorKind}`)}
        </FieldError>
      ) : null}

      {ownCalendar ? null : (
        <div className="flex flex-wrap gap-2">
          <Button
            ref={saveRef}
            type="submit"
            aria-disabled={!canSave || busy}
            className="aria-disabled:opacity-50"
          >
            {busy ? tc("actions.saving") : t("form.save")}
          </Button>
          {own.start || own.end ? (
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => {
                // The button disappears once the student's dates are gone; focus moves to Save.
                saveRef.current?.focus();
                setApplyFailed(false);
                void save({ start: null, end: null }, t("form.cleared"));
              }}
            >
              {t("form.clear")}
            </Button>
          ) : null}
        </div>
      )}
    </form>
  );
}
