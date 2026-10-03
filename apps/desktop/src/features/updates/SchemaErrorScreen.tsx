import { ArrowDownToLine, DatabaseZap, Download, LoaderCircle, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useApi } from "@/api/context";
import type { ApiError } from "@/api/errors";
import { useUpdaterStatus } from "@/api/queries";
import { useOpenExternal } from "@/components/common/useOpenExternal";
import { Button } from "@/components/ui/button";
import { ProblemActions } from "@/features/diagnostics/ProblemActions";
import { RevealFolderButton } from "@/features/settings/RevealFolderButton";
import { isHttpUrl } from "@/lib/url";
import { useCheckForUpdate, useUpdateStore } from "@/stores/updates";
import { InstallUpdateDialog } from "./InstallUpdateDialog";

/**
 * Start screen when the database was written by another version of PageLamp (the core can't
 * open it). Too new: the fix is updating, so "Check for updates" is right here (it works without
 * the database). Too old for this version to upgrade: restore one of the backups the core writes
 * before every migration. Nothing was changed either way.
 */
export function SchemaErrorScreen({ error }: { error: ApiError }) {
  const { t } = useTranslation();
  const tooNew = error.kind === "schema_too_new";
  return (
    <div className="mx-auto max-w-lg space-y-6 p-8">
      <div className="space-y-3">
        <DatabaseZap className="size-6 text-muted-foreground" aria-hidden />
        <h1 className="font-heading text-xl font-semibold">
          {tooNew ? t("schema.tooNewTitle") : t("schema.tooOldTitle")}
        </h1>
        <p className="text-muted-foreground">
          {tooNew ? t("schema.tooNewBody") : t("schema.tooOldBody")}
        </p>
        {error.message ? (
          <p lang="en" className="text-xs text-muted-foreground">
            {error.message}
          </p>
        ) : null}
      </div>
      {tooNew ? <CheckForUpdates /> : <ShowDataFolder />}
      <ProblemActions />
    </div>
  );
}

function CheckForUpdates() {
  const { t } = useTranslation();
  const { t: tu } = useTranslation("updates");
  const check = useCheckForUpdate();
  const checking = useUpdateStore((s) => s.checking);
  const checked = useUpdateStore((s) => s.checked);
  const available = useUpdateStore((s) => s.available);
  const checkError = useUpdateStore((s) => s.checkError);
  const status = useUpdaterStatus();
  const openExternal = useOpenExternal();
  const [open, setOpen] = useState(false);
  const downloadOnly = status.data?.install === "download_only";

  return (
    <div className="space-y-3">
      <Button type="button" disabled={checking} onClick={() => void check()}>
        {checking ? (
          <LoaderCircle className="animate-spin" aria-hidden />
        ) : (
          <RefreshCw aria-hidden />
        )}
        {checking ? tu("settings.checking") : t("schema.checkForUpdates")}
      </Button>
      <div role="status" className="space-y-2 text-sm">
        {checking ? null : checkError?.kind === "not_found" ? (
          // The channel has no release with update information yet.
          <p>{t("schema.noUpdate")}</p>
        ) : checkError ? (
          <p className="text-destructive">{tu("settings.checkFailed")}</p>
        ) : available ? (
          <>
            <p className="font-medium">
              {tu("settings.available", { version: available.version })}
            </p>
            {downloadOnly ? (
              isHttpUrl(available.download_url) ? (
                <Button size="sm" onClick={() => openExternal(available.download_url ?? "")}>
                  <Download aria-hidden />
                  {tu("settings.download", { version: available.version })}
                </Button>
              ) : null
            ) : (
              <>
                <Button size="sm" onClick={() => setOpen(true)}>
                  <ArrowDownToLine aria-hidden />
                  {tu("settings.install")}
                </Button>
                <InstallUpdateDialog update={available} open={open} onOpenChange={setOpen} />
              </>
            )}
          </>
        ) : checked ? (
          <p>{t("schema.noUpdate")}</p>
        ) : null}
      </div>
    </div>
  );
}

function ShowDataFolder() {
  const { t } = useTranslation();
  const api = useApi();
  return <RevealFolderButton label={t("schema.showFolder")} reveal={() => api.revealDataDir()} />;
}
