import { CalendarCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useConfirmCourseDates } from "@/api/queries";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useApiErrorText } from "@/lib/useApiErrorText";

/**
 * One-time "Check this course's dates" for dates kept from version 0.1 (anchor origin
 * `legacy`; calendar design §3.2, §7.13). "These dates are right" confirms them; "Edit dates"
 * goes to the form. Saving or clearing the form also ends it (the facade drops the mark).
 */
export function CheckDatesPrompt({
  courseId,
  onEdit,
  onDone,
}: {
  courseId: string;
  /** Moves focus to the dates form. */
  onEdit: () => void;
  /** Where focus goes once the prompt is gone. */
  onDone: () => void;
}) {
  const { t } = useTranslation("calendar");
  const errorText = useApiErrorText();
  const confirm = useConfirmCourseDates();

  async function confirmDates() {
    if (confirm.isPending) return;
    try {
      await confirm.mutateAsync({ courseId });
      toast.success(t("checkDates.confirmed"));
      onDone();
    } catch (error) {
      toast.error(errorText(error));
    }
  }

  return (
    <Alert role="status">
      <CalendarCheck aria-hidden />
      <AlertTitle>{t("checkDates.title")}</AlertTitle>
      <AlertDescription>
        <p>{t("checkDates.body")}</p>
        <div className="mt-2 flex flex-wrap gap-2">
          <Button
            size="sm"
            aria-disabled={confirm.isPending || undefined}
            className="aria-disabled:opacity-50"
            onClick={() => void confirmDates()}
          >
            {t("checkDates.confirm")}
          </Button>
          <Button size="sm" variant="outline" onClick={onEdit}>
            {t("checkDates.edit")}
          </Button>
        </div>
      </AlertDescription>
    </Alert>
  );
}
