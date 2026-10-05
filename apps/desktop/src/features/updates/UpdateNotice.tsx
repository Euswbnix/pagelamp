import { ArrowDownToLine, Download, X } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useUpdaterStatus } from "@/api/queries";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Alert, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { isHttpUrl } from "@/lib/url";
import { useUpdateStore } from "@/stores/updates";
import { InstallUpdateDialog } from "./InstallUpdateDialog";

/**
 * "PageLamp x.y.z is available" above every screen, after a check found an update. Nothing is
 * installed without the student's click (D2). deb/rpm installs get a download link instead.
 */
export function UpdateNotice() {
  const { t } = useTranslation("updates");
  const available = useUpdateStore((s) => s.available);
  const dismissed = useUpdateStore((s) => s.noticeDismissed);
  const dismiss = useUpdateStore((s) => s.dismissNotice);
  const install = useUpdateStore((s) => s.install);
  const status = useUpdaterStatus();
  const openExternal = useOpenExternal();
  const [open, setOpen] = useState(false);
  const titleId = useId();

  // An install under way hides the notice; one that failed or is held back doesn't (its dialog
  // may be gone without having been closed). Keep the dialog mounted while it is open.
  const installing =
    install.phase === "downloading" ||
    install.phase === "installing" ||
    install.phase === "restarting";
  if (!available || ((dismissed || installing) && !open)) return null;
  const downloadOnly = status.data?.install === "download_only";

  return (
    <>
      <Alert role="region" aria-labelledby={titleId} className="mb-6 px-4 py-3">
        <ArrowDownToLine aria-hidden />
        <AlertTitle id={titleId}>
          {t("notice.available", { version: available.version })}
        </AlertTitle>
        <div className="col-start-2 mt-3 flex flex-wrap items-center gap-2">
          {downloadOnly ? (
            isHttpUrl(available.download_url) ? (
              <Button size="sm" onClick={() => openExternal(available.download_url ?? "")}>
                <Download aria-hidden />
                {t("notice.download")}
              </Button>
            ) : null
          ) : (
            <Button size="sm" onClick={() => setOpen(true)}>
              <ArrowDownToLine aria-hidden />
              {t("notice.install")}
            </Button>
          )}
          <Button type="button" variant="ghost" size="sm" onClick={dismiss}>
            <X aria-hidden />
            {t("notice.later")}
          </Button>
        </div>
      </Alert>
      {downloadOnly ? null : (
        <InstallUpdateDialog update={available} open={open} onOpenChange={setOpen} />
      )}
    </>
  );
}
