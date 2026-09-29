import { type FormEvent, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { toApiError } from "@/api/errors";
import { useClearKeepCourseCurrent, useKeepCourseCurrent } from "@/api/queries";
import type { CourseLifecycle } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { formatIsoDate } from "@/lib/format";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { useToday } from "@/lib/useToday";

/**
 * "I'm still taking this" (calendar design §8.1 rule 1, §8.7): offered for courses under Past;
 * once set, says until when, with "Change date" and "Undo". The default date is the facade's.
 * Focus stays on this control's status line when the buttons change under it.
 */
export function KeepCurrentControl({
  courseId,
  lifecycle,
}: {
  courseId: string;
  lifecycle: CourseLifecycle;
}) {
  const { t, i18n } = useTranslation("calendar");
  const { t: tc } = useTranslation();
  const errorText = useApiErrorText();
  const today = useToday();
  const keep = useKeepCourseCurrent();
  const clear = useClearKeepCourseCurrent();
  const statusRef = useRef<HTMLDivElement>(null);
  const [editing, setEditing] = useState(false);
  const ids = { until: useId(), error: useId(), hint: useId() };
  const until = lifecycle.kept_current_until ?? null;
  const [value, setValue] = useState(until ?? "");
  const busy = keep.isPending || clear.isPending;

  const focusStatus = () => requestAnimationFrame(() => statusRef.current?.focus());

  async function run(action: () => Promise<unknown>, message: string) {
    try {
      await action();
      toast.success(message);
      setEditing(false);
      focusStatus();
    } catch (error) {
      if (toApiError(error).kind !== "invalid") toast.error(errorText(error));
    }
  }

  function saveDate(event: FormEvent) {
    event.preventDefault();
    if (busy || !value) return;
    void run(() => keep.mutateAsync({ courseId, until: value }), t("keep.kept"));
  }

  const invalid = keep.error ? toApiError(keep.error).kind === "invalid" : false;

  if (!until && lifecycle.group !== "past") {
    // Nothing to offer; keep the focus target so focus survives an Undo that ends here.
    return <div ref={statusRef} tabIndex={-1} className="outline-none" />;
  }

  return (
    <div ref={statusRef} tabIndex={-1} className="space-y-2 outline-none">
      {until ? (
        <>
          <p className="text-sm">
            {t("keep.keptUntil", { date: formatIsoDate(until, i18n.language) })}
          </p>
          {editing ? (
            <form onSubmit={saveDate} noValidate className="flex flex-wrap items-end gap-2">
              <Field data-invalid={invalid || undefined} className="w-auto">
                <FieldLabel htmlFor={ids.until}>{t("keep.untilLabel")}</FieldLabel>
                <Input
                  id={ids.until}
                  type="date"
                  min={today}
                  value={value}
                  onChange={(event) => {
                    if (keep.isError) keep.reset();
                    setValue(event.target.value);
                  }}
                  aria-invalid={invalid || undefined}
                  aria-describedby={invalid ? ids.error : undefined}
                  // Opened by the student's own click, so focus moves into the field.
                  autoFocus
                />
              </Field>
              <Button
                type="submit"
                size="sm"
                aria-disabled={busy || !value || undefined}
                className="aria-disabled:opacity-50"
              >
                {t("keep.saveDate")}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => {
                  setEditing(false);
                  keep.reset();
                  focusStatus();
                }}
              >
                {tc("actions.cancel")}
              </Button>
              {invalid ? <FieldError id={ids.error}>{t("keep.invalid")}</FieldError> : null}
            </form>
          ) : (
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="outline"
                aria-disabled={busy || undefined}
                className="aria-disabled:opacity-50"
                onClick={() => {
                  if (busy) return;
                  setValue(until);
                  setEditing(true);
                }}
              >
                {t("keep.change")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                aria-disabled={busy || undefined}
                className="aria-disabled:opacity-50"
                onClick={() => {
                  if (!busy) void run(() => clear.mutateAsync({ courseId }), t("keep.cleared"));
                }}
              >
                {t("keep.undo")}
              </Button>
            </div>
          )}
        </>
      ) : (
        <>
          <Button
            size="sm"
            variant="outline"
            aria-describedby={ids.hint}
            aria-disabled={busy || undefined}
            className="aria-disabled:opacity-50"
            onClick={() => {
              if (!busy)
                void run(() => keep.mutateAsync({ courseId, until: null }), t("keep.kept"));
            }}
          >
            {t("keep.button")}
          </Button>
          <p id={ids.hint} className="text-sm text-muted-foreground">
            {t("keep.hint")}
          </p>
        </>
      )}
    </div>
  );
}
