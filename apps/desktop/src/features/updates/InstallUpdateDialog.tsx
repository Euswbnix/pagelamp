import { LoaderCircle } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { AvailableUpdate } from "@/api/client";
import { useActivity, useUpdaterStatus } from "@/api/queries";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useStopSync, useSyncActivity, useSyncStore } from "@/stores/sync";
import { type InstallState, useInstallUpdate, useUpdateStore } from "@/stores/updates";

/**
 * "Install PageLamp x.y.z?": the only way an update gets installed (decision D2: always ask).
 * Waits while anything runs that the restart would kill: a sync (the app's or the CLI's), an AI
 * reading or a Codex download (App::activity). On Windows it says PageLamp will close.
 * Once installing, it can't be dismissed: the app restarts at the end.
 */
export function InstallUpdateDialog({
  update,
  open,
  onOpenChange,
}: {
  update: AvailableUpdate;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation("updates");
  const { t: tc } = useTranslation();
  const status = useUpdaterStatus();
  const install = useUpdateStore((s) => s.install);
  const start = useInstallUpdate();
  const sync = useSyncActivity();
  const activity = useActivity(open);
  const running = activity.data?.items ?? [];
  const generating = running.some((item) => item.kind === "generation");
  const codexInstalling = running.some((item) => item.kind === "codex_install");
  const busy = sync.busy || generating || codexInstalling;
  const stopping = useSyncStore((s) => s.stopping);
  const stopSync = useStopSync();
  const hintId = useId();
  const working =
    install.phase === "downloading" ||
    install.phase === "installing" ||
    install.phase === "restarting";
  const blocked = busy || working;

  return (
    <AlertDialog open={open} onOpenChange={(next) => (working ? undefined : onOpenChange(next))}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t("install.title", { version: update.version })}</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="space-y-2">
              <p>{t("install.body")}</p>
              {status.data?.platform === "windows" ? (
                <p className="font-medium text-foreground">{t("install.windows")}</p>
              ) : null}
              <p>{t("install.aiApp")}</p>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        {install.phase === "idle" ? null : <InstallProgress install={install} />}
        <AlertDialogFooter className="items-center">
          {busy && !working ? (
            <span id={hintId} className="text-xs text-muted-foreground sm:mr-auto">
              {sync.external
                ? t("install.availableAfterOtherSync")
                : sync.busy
                  ? t("install.availableAfterSync")
                  : generating
                    ? t("install.availableAfterGeneration")
                    : t("install.availableAfterCodexInstall")}
            </span>
          ) : null}
          {/* This window's sync can be stopped from here (design §7); the CLI's can't. */}
          {sync.busy && !sync.external && !working ? (
            <Button
              type="button"
              variant="outline"
              onClick={() => void stopSync()}
              aria-disabled={stopping || undefined}
              className="aria-disabled:opacity-50"
            >
              {stopping ? tc("sync.stopping") : t("install.stopSync")}
            </Button>
          ) : null}
          <AlertDialogCancel disabled={working}>{tc("actions.cancel")}</AlertDialogCancel>
          <Button
            type="button"
            aria-disabled={blocked || undefined}
            aria-describedby={busy && !working ? hintId : undefined}
            className="aria-disabled:opacity-50"
            onClick={() => {
              if (!blocked) void start();
            }}
          >
            {working ? <LoaderCircle className="animate-spin" aria-hidden /> : null}
            {install.phase === "failed" ? t("install.tryAgain") : t("install.confirm")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function InstallProgress({ install }: { install: InstallState }) {
  const { t } = useTranslation("updates");
  const { t: tc } = useTranslation();
  if (install.phase === "failed") {
    return (
      <div role="alert" className="space-y-1 text-sm text-destructive">
        <p className="font-medium">{t("install.failed")}</p>
        <p>{tc(`errors.${install.error.kind}`)}</p>
        {install.error.message ? (
          <p lang="en" className="text-xs opacity-80">
            {install.error.message}
          </p>
        ) : null}
      </div>
    );
  }
  const percent =
    install.phase === "downloading" && install.total
      ? Math.min(100, Math.round((install.downloaded / install.total) * 100))
      : null;
  const text =
    install.phase === "downloading"
      ? percent === null
        ? t("install.downloadingUnknown")
        : t("install.downloading", { percent })
      : install.phase === "installing"
        ? t("install.installing")
        : t("install.restarting");
  return (
    <div className="space-y-2">
      <Progress
        aria-label={t("install.progressLabel")}
        value={install.phase === "downloading" ? (percent ?? 0) : 100}
      />
      <p role="status" className="text-sm text-muted-foreground">
        {text}
      </p>
    </div>
  );
}
