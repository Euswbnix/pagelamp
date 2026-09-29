import { RefreshCcw, X } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { useUpdaterStatus } from "@/api/queries";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useUpdateStore } from "@/stores/updates";

/**
 * First launch after an update: AI apps keep running the old `pagelamp mcp` until they restart,
 * so ask the student to quit and reopen theirs.
 */
export function PostUpdateBanner() {
  const { t } = useTranslation("updates");
  const updated = useUpdateStore((s) => s.updated);
  const dismissed = useUpdateStore((s) => s.updatedDismissed);
  const dismiss = useUpdateStore((s) => s.dismissUpdated);
  const status = useUpdaterStatus();
  const titleId = useId();
  if (!updated || dismissed || !status.data) return null;

  return (
    <Alert role="region" aria-labelledby={titleId} className="mb-6 px-4 py-3">
      <RefreshCcw aria-hidden />
      <AlertTitle id={titleId}>
        {t("updated.title", { version: status.data.current_version })}
      </AlertTitle>
      <AlertDescription>{t("updated.body")}</AlertDescription>
      <div className="col-start-2 mt-3">
        <Button type="button" variant="ghost" size="sm" onClick={dismiss}>
          <X aria-hidden />
          {t("updated.dismiss")}
        </Button>
      </div>
    </Alert>
  );
}
