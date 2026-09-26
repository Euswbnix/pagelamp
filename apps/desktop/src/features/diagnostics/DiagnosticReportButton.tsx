import { ClipboardCopy } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useDiagnosticReport } from "@/api/queries";
import { CopyButton } from "@/components/common/CopyButton";
import { ErrorState } from "@/components/common/ErrorState";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";

interface DiagnosticReportButtonProps {
  variant?: "default" | "outline" | "secondary";
  size?: "sm" | "default";
}

/**
 * "Copy diagnostic report…": opens the whole report in a read-only preview first. Nothing is
 * copied until the student has seen it and presses Copy there.
 */
export function DiagnosticReportButton({
  variant = "outline",
  size = "sm",
}: DiagnosticReportButtonProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button type="button" variant={variant} size={size}>
          <ClipboardCopy aria-hidden />
          {t("diagnostics.copyReport")}
        </Button>
      </DialogTrigger>
      {/* One "Close" (in the footer, next to Copy); Escape closes it too. */}
      <DialogContent className="sm:max-w-2xl" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t("diagnostics.previewTitle")}</DialogTitle>
          {/* The description is announced when the dialog opens: lead with the privacy check. */}
          <DialogDescription className="font-medium text-foreground">
            {t("diagnostics.previewNote")}
          </DialogDescription>
          <p className="text-muted-foreground">{t("diagnostics.previewContents")}</p>
        </DialogHeader>
        {/* Mounted only while open, so the report is fetched fresh each time. */}
        {open ? <ReportPreview /> : null}
      </DialogContent>
    </Dialog>
  );
}

function ReportPreview() {
  const { t } = useTranslation();
  const report = useDiagnosticReport(true);
  return (
    <>
      {report.isPending ? (
        <div aria-busy="true">
          <span className="sr-only" role="status">
            {t("states.loading")}
          </span>
          <Skeleton className="h-80 w-full" aria-hidden />
        </div>
      ) : report.isError ? (
        <ErrorState error={report.error} onRetry={() => void report.refetch()} />
      ) : (
        <Textarea
          readOnly
          value={report.data}
          aria-label={t("diagnostics.reportLabel")}
          spellCheck={false}
          className="h-80 resize-none font-mono text-xs field-sizing-fixed md:text-xs"
        />
      )}
      <DialogFooter>
        <DialogClose asChild>
          <Button type="button" variant="outline">
            {t("actions.close")}
          </Button>
        </DialogClose>
        {report.isSuccess ? (
          <CopyButton
            text={report.data}
            label={t("diagnostics.copyAria")}
            size="default"
            variant="default"
          />
        ) : null}
      </DialogFooter>
    </>
  );
}
