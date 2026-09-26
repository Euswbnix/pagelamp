import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";

/** Back/Cancel on the left, submit (with a spinner while working) on the right. */
export function FormFooter({
  start,
  pending,
  submitLabel,
  pendingLabel,
}: {
  start?: ReactNode;
  pending: boolean;
  submitLabel: string;
  pendingLabel: string;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 pt-2">
      <div>{start}</div>
      <Button type="submit" disabled={pending} aria-busy={pending}>
        {pending ? <Spinner aria-hidden /> : null}
        {pending ? pendingLabel : submitLabel}
      </Button>
      {/* Screen readers hear "Checking your token…" etc. while the button is busy. */}
      <span className="sr-only" aria-live="polite">
        {pending ? pendingLabel : ""}
      </span>
    </div>
  );
}
