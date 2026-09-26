import { CircleDot, Info } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSetCoursePolicy } from "@/api/queries";
import { AI_POLICIES, type AiPolicy, type Course } from "@/api/types";
import { brand, localized } from "@/brand";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup } from "@/components/ui/radio-group";
import { Textarea } from "@/components/ui/textarea";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { PolicyOption } from "./PolicyOption";

function isAiPolicy(value: string): value is AiPolicy {
  return AI_POLICIES.includes(value as AiPolicy);
}

/** The course's AI-use rule, which the student's AI app reads and follows. */
export function PolicyTab({ course }: { course: Course }) {
  // Re-create the form (fresh draft) whenever the saved policy changes.
  return <PolicyForm key={`${course.ai_policy}|${course.ai_policy_note ?? ""}`} course={course} />;
}

function PolicyForm({ course }: { course: Course }) {
  const { t, i18n } = useTranslation("course");
  const { t: tc } = useTranslation();
  const errorText = useApiErrorText();
  const ids = { heading: useId(), options: useId(), note: useId(), noteHelp: useId() };
  const savedNote = course.ai_policy_note ?? "";
  const [policy, setPolicy] = useState<AiPolicy>(course.ai_policy);
  const [note, setNote] = useState(savedNote);
  const mutation = useSetCoursePolicy();

  const changed = policy !== course.ai_policy || note.trim() !== savedNote.trim();

  // Editing clears the message of a failed save.
  function edit<T>(set: (value: T) => void, value: T) {
    if (mutation.isError) mutation.reset();
    set(value);
  }

  function discard() {
    setPolicy(course.ai_policy);
    setNote(savedNote);
    mutation.reset();
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    // mutateAsync: the form re-mounts when the saved policy changes (see `key` above).
    try {
      await mutation.mutateAsync({ courseId: course.id, policy, note: note.trim() || null });
      toast.success(t("policy.saved"));
    } catch {
      // Shown inline below (mutation.error).
    }
  }

  return (
    <form onSubmit={submit} aria-labelledby={ids.heading} className="max-w-2xl space-y-6">
      <div className="space-y-1">
        <h2 id={ids.heading} className="font-heading text-base font-semibold tracking-tight">
          {t("policy.title")}
        </h2>
        <p className="text-sm text-muted-foreground">
          {t("policy.explanation", {
            prohibited: tc("policy.prohibited"),
            unknown: tc("policy.unknown"),
          })}
        </p>
      </div>

      <Alert role="note">
        <Info aria-hidden />
        <AlertDescription>{localized(brand.aiPolicyHint, i18n.language)}</AlertDescription>
      </Alert>

      <RadioGroup
        aria-labelledby={ids.heading}
        value={policy}
        onValueChange={(value) => isAiPolicy(value) && edit(setPolicy, value)}
      >
        {AI_POLICIES.map((option) => (
          <PolicyOption key={option} policy={option} idPrefix={ids.options} />
        ))}
      </RadioGroup>

      <div className="grid gap-2">
        <Label htmlFor={ids.note}>{t("policy.noteLabel")}</Label>
        <Textarea
          id={ids.note}
          value={note}
          onChange={(event) => edit(setNote, event.target.value)}
          aria-describedby={ids.noteHelp}
          rows={3}
        />
        <p id={ids.noteHelp} className="text-sm text-muted-foreground">
          {t("policy.noteHelp")}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <Button type="submit" disabled={!changed || mutation.isPending}>
          {mutation.isPending ? tc("actions.saving") : t("policy.save")}
        </Button>
        {changed ? (
          <Button type="button" variant="ghost" onClick={discard} disabled={mutation.isPending}>
            {t("policy.discard")}
          </Button>
        ) : null}
        <span aria-live="polite" className="text-sm text-muted-foreground">
          {changed ? (
            <span className="inline-flex items-center gap-1.5">
              <CircleDot className="size-4 text-warning" aria-hidden />
              {t("policy.unsaved")}
            </span>
          ) : null}
        </span>
      </div>

      {mutation.error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(mutation.error)}
        </p>
      ) : null}
    </form>
  );
}
