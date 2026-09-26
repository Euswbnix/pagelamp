import { type FormEvent, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { toApiError } from "@/api/errors";
import { useSetCourseTerm } from "@/api/queries";
import type { Course } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

/** "Wrong week? Set this course's term dates": start/end override, saved per course. */
export function TermDatesForm({ course }: { course: Course }) {
  const { t } = useTranslation("course");
  const { t: tc } = useTranslation();
  const ids = { heading: useId(), start: useId(), end: useId(), error: useId() };
  const savedStart = course.term_start ?? "";
  const savedEnd = course.term_end ?? "";
  const [start, setStart] = useState(savedStart);
  const [end, setEnd] = useState(savedEnd);
  const mutation = useSetCourseTerm();
  const saveRef = useRef<HTMLButtonElement>(null);

  // When the saved dates change (after saving or clearing), start again from them. This is
  // done here instead of re-keying the form, so focus stays in the date field after Enter.
  const [shown, setShown] = useState({ start: savedStart, end: savedEnd });
  if (shown.start !== savedStart || shown.end !== savedEnd) {
    setShown({ start: savedStart, end: savedEnd });
    setStart(savedStart);
    setEnd(savedEnd);
  }

  const changed = start !== savedStart || end !== savedEnd;
  // Only a student's own override can be undone; the backend then falls back to the dates
  // the course source reported (term_source "synced"), or to none.
  const overridden = course.term_source === "user";
  const errorKind = mutation.error ? toApiError(mutation.error).kind : null;
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
    set(value);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!changed || mutation.isPending) return;
    void save({ start: start || null, end: end || null }, t("term.saved"));
  }

  return (
    <form onSubmit={submit} aria-labelledby={ids.heading} noValidate className="space-y-4">
      <div className="space-y-1">
        <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
          {t("term.title")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("term.description")}</p>
        <p className="text-sm text-muted-foreground">{t("term.breaksNote")}</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.start}>{t("term.start")}</FieldLabel>
          <Input
            id={ids.start}
            type="date"
            value={start}
            onChange={(event) => edit(setStart, event.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={errorKind ? ids.error : undefined}
          />
        </Field>
        <Field data-invalid={invalid || undefined}>
          <FieldLabel htmlFor={ids.end}>{t("term.end")}</FieldLabel>
          <Input
            id={ids.end}
            type="date"
            value={end}
            min={start || undefined}
            onChange={(event) => edit(setEnd, event.target.value)}
            aria-invalid={invalid || undefined}
            aria-describedby={errorKind ? ids.error : undefined}
          />
        </Field>
      </div>

      {errorKind ? (
        <FieldError id={ids.error}>
          {invalid ? t("term.invalid") : tc(`errors.${errorKind}`)}
        </FieldError>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <Button
          ref={saveRef}
          type="submit"
          aria-disabled={!changed || mutation.isPending}
          className="aria-disabled:opacity-50"
        >
          {mutation.isPending ? tc("actions.saving") : t("term.save")}
        </Button>
        {overridden ? (
          <Button
            type="button"
            variant="outline"
            disabled={mutation.isPending}
            onClick={() => {
              // The button disappears once the override is gone; focus moves to Save.
              saveRef.current?.focus();
              void save({ start: null, end: null }, t("term.usedSynced"));
            }}
          >
            {t("term.useSynced")}
          </Button>
        ) : null}
      </div>
    </form>
  );
}
