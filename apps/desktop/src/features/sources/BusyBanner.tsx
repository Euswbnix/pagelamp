import { Hourglass, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";

/** Another process (usually the command line) holds the sync lock. */
export function BusyBanner({
  onCheckAgain,
  checking,
}: {
  onCheckAgain: () => void;
  checking: boolean;
}) {
  const { t } = useTranslation("sources");
  const { t: tc } = useTranslation();
  return (
    // role="status": polite, it is shown on load rather than in reaction to an action.
    <Alert role="status">
      <Hourglass aria-hidden />
      <AlertTitle>{t("busy.title")}</AlertTitle>
      <AlertDescription>{tc("sync.busy")}</AlertDescription>
      <div className="col-start-2 mt-2">
        <Button size="sm" variant="outline" onClick={onCheckAgain} disabled={checking}>
          <RefreshCw className={checking ? "animate-spin" : undefined} aria-hidden />
          {t("busy.checkAgain")}
        </Button>
      </div>
    </Alert>
  );
}
