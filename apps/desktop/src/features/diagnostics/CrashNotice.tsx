import { TriangleAlert, X } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useClearLastCrash, useLastCrash } from "@/api/queries";
import type { CrashReport } from "@/api/types";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { focusPageHeading } from "@/lib/focus";
import { useApiErrorText } from "@/lib/useApiErrorText";
import { DiagnosticReportButton } from "./DiagnosticReportButton";
import { ReportProblemButton } from "./ReportProblemButton";

/**
 * Shown above every screen when the panic hook recorded a crash (desktop app or the MCP server
 * an AI app started). Stays until dismissed, which clears the record.
 */
export function CrashNotice() {
  const crash = useLastCrash();
  return crash.data ? <Notice crash={crash.data} /> : null;
}

function Notice({ crash }: { crash: CrashReport }) {
  const { t } = useTranslation();
  const titleId = useId();
  const errorText = useApiErrorText();
  const clear = useClearLastCrash();

  async function dismiss() {
    if (clear.isPending) return;
    try {
      await clear.mutateAsync();
      // The notice (and the focused button) is gone; continue from the page heading.
      focusPageHeading();
    } catch (error) {
      toast.error(t("diagnostics.crash.dismissFailed"), { description: errorText(error) });
    }
  }

  return (
    <Alert role="region" aria-labelledby={titleId} className="mb-6 px-4 py-3">
      <TriangleAlert aria-hidden />
      <AlertTitle id={titleId}>
        {crash.process === "mcp"
          ? t("diagnostics.crash.titleMcp")
          : t("diagnostics.crash.titleApp")}
      </AlertTitle>
      <AlertDescription>
        <SentenceWithTime text={t("diagnostics.crash.body", { when: WHEN })} iso={crash.time} />
      </AlertDescription>
      {/* Outside the description, which underlines and mutes everything in it. */}
      <div className="col-start-2 mt-3 flex flex-wrap items-center gap-2">
        <DiagnosticReportButton variant="default" />
        <ReportProblemButton />
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-disabled={clear.isPending || undefined}
          className="aria-disabled:opacity-50"
          onClick={dismiss}
        >
          <X aria-hidden />
          {t("diagnostics.crash.dismiss")}
        </Button>
      </div>
    </Alert>
  );
}
