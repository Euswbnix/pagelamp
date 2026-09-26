import { Trash2 } from "lucide-react";
import { type MouseEvent, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type ApiError, toApiError } from "@/api/errors";
import { useRemoveSource } from "@/api/queries";
import type { SourceRecord } from "@/api/types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useReturnFocus } from "@/lib/focus";

/** "Remove" with a confirmation that spells out what goes away (courses, settings, secret). */
export function RemoveSourceButton({
  source,
  disabled,
}: {
  source: SourceRecord;
  disabled?: boolean;
}) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  const remove = useRemoveSource();
  // After a removal the card (and this trigger) is gone, so focus moves to the page heading.
  const returnFocus = useReturnFocus();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  async function confirm(event: MouseEvent) {
    // Keep the dialog open while removing; it closes on success.
    event.preventDefault();
    setError(null);
    try {
      await remove.mutateAsync(source.id);
      setOpen(false);
      toast.success(t("remove.done", { label: source.label }));
    } catch (err) {
      setError(toApiError(err));
    }
  }

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setError(null);
      }}
    >
      <AlertDialogTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          disabled={disabled}
          aria-label={t("actions.removeSource", { label: source.label })}
        >
          <Trash2 aria-hidden />
          {t("actions.remove")}
        </Button>
      </AlertDialogTrigger>
      <AlertDialogContent {...returnFocus}>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("remove.title", { label: source.label })}</AlertDialogTitle>
          <AlertDialogDescription>{t(`remove.description.${source.kind}`)}</AlertDialogDescription>
        </AlertDialogHeader>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {tc(`errors.${error.kind}`)} {error.message}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel>{tc("actions.cancel")}</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={confirm}
            disabled={remove.isPending}
            aria-busy={remove.isPending}
          >
            {remove.isPending ? <Spinner aria-hidden /> : null}
            {remove.isPending ? t("remove.removing") : t("remove.confirm")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
