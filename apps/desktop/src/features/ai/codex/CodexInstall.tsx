import { useTranslation } from "react-i18next";
import type { CodexStatus } from "@/api/provisional/codex";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useAiErrorText } from "@/features/ai/useAiErrorText";
import {
  isInstalling,
  useCancelCodexInstall,
  useCodexStore,
  useInstallCodex,
} from "@/stores/codex";

/** Codex unpacks to about this much (design §2.3); the facade reports only the download. */
const DISK_BYTES = 250_000_000;

export function formatMegabytes(bytes: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit: "megabyte",
    maximumFractionDigits: 0,
  }).format(bytes / 1_000_000);
}

/**
 * Not installed yet: what the download is (size, source, checked before installing) and the
 * button; while it runs, the progress with Cancel; if it failed, why and Try again.
 */
export function CodexDownload({ status }: { status: CodexStatus }) {
  const { t, i18n } = useTranslation("ai");
  const install = useCodexStore((s) => s.install);
  const start = useInstallCodex();
  if (isInstalling(install)) return <CodexInstallProgress />;
  return (
    <div className="space-y-3">
      <p className="text-sm text-muted-foreground">
        {t("codex.download.size", {
          download: formatMegabytes(status.runtime.download_bytes, i18n.language),
          disk: formatMegabytes(DISK_BYTES, i18n.language),
        })}
      </p>
      {install.phase === "failed" ? <InstallFailed /> : null}
      <Button type="button" onClick={() => void start()}>
        {install.phase === "failed" ? t("codex.download.tryAgain") : t("codex.download.button")}
      </Button>
    </div>
  );
}

function InstallFailed() {
  const { t } = useTranslation("ai");
  const errorText = useAiErrorText();
  const install = useCodexStore((s) => s.install);
  if (install.phase !== "failed") return null;
  return (
    <div role="alert" className="space-y-1 text-sm text-destructive">
      <p className="font-medium">{t("codex.download.failed")}</p>
      <p>{errorText(install.error)}</p>
    </div>
  );
}

/** The running download → verify → install, with Cancel (the partial file is deleted). */
export function CodexInstallProgress() {
  const { t } = useTranslation("ai");
  const install = useCodexStore((s) => s.install);
  const cancel = useCancelCodexInstall();
  if (install.phase === "failed") return <InstallFailed />;
  if (!isInstalling(install)) return null;
  const percent =
    install.phase === "downloading" && install.total
      ? Math.min(100, Math.round((install.downloaded / install.total) * 100))
      : null;
  const text =
    install.phase === "downloading"
      ? percent === null
        ? t("codex.download.downloadingUnknown")
        : t("codex.download.downloading", { percent })
      : install.phase === "verifying"
        ? t("codex.download.verifying")
        : t("codex.download.installing");
  return (
    <div className="space-y-2">
      <Progress
        aria-label={t("codex.download.progress")}
        value={install.phase === "downloading" ? (percent ?? 0) : 100}
        className="max-w-sm"
      />
      <div className="flex flex-wrap items-center gap-3">
        <p role="status" className="text-sm text-muted-foreground">
          {text}
        </p>
        {install.phase === "downloading" ? (
          <Button type="button" size="sm" variant="outline" onClick={() => void cancel()}>
            {t("codex.download.cancel")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}
