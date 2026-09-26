import { useQueryClient } from "@tanstack/react-query";
import { CircleAlert, LoaderCircle } from "lucide-react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ApiError } from "@/api/errors";
import { queryKeys } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { useSyncCounts, useSyncStore } from "@/stores/sync";
import { Notice } from "./parts/Notice";
import { useSyncActivity } from "./parts/useSyncActivity";

/**
 * Shows that a sync is running (here or in another process), or why the last run could not
 * start. The outer element is a live region that always exists, so changes are announced.
 */
export function SyncBanner() {
  const { t } = useTranslation("courses");
  const { t: tc } = useTranslation();
  const queryClient = useQueryClient();
  const { running, external } = useSyncActivity();
  const order = useSyncStore((s) => s.order);
  const bySource = useSyncStore((s) => s.bySource);
  const runError = useSyncStore((s) => s.runError);
  const counts = useSyncCounts();
  const [dismissed, setDismissed] = useState<ApiError | null>(null);

  let content: ReactNode = null;
  if (running) {
    const current = order.map((id) => bySource[id]).find((p) => p && !p.result);
    content = (
      <Notice
        icon={<LoaderCircle className="size-4 animate-spin text-muted-foreground" aria-hidden />}
        title={
          counts.total
            ? tc("sync.syncingProgress", { done: counts.done, total: counts.total })
            : tc("sync.syncing")
        }
      >
        <p>{t("banner.runningHint")}</p>
        {current?.message ? (
          // Progress changes every step; keep it out of the announcements.
          <p aria-live="off" className="text-xs">
            {t("banner.sourceProgress", { source: current.label, message: current.message })}
          </p>
        ) : null}
      </Notice>
    );
  } else if (external) {
    content = (
      <Notice
        icon={<LoaderCircle className="size-4 animate-spin text-muted-foreground" aria-hidden />}
        title={t("banner.externalTitle")}
        action={
          <Button
            size="sm"
            variant="outline"
            onClick={() => void queryClient.invalidateQueries({ queryKey: queryKeys.all })}
          >
            {t("banner.checkAgain")}
          </Button>
        }
      >
        {t("banner.externalHint")}
      </Notice>
    );
  } else if (runError && runError !== dismissed) {
    content = (
      <Notice
        icon={<CircleAlert className="size-4 text-warning" aria-hidden />}
        title={tc("sync.failed")}
        action={
          <Button size="sm" variant="ghost" onClick={() => setDismissed(runError)}>
            {tc("actions.close")}
          </Button>
        }
      >
        {runError.kind === "busy" ? tc("sync.busy") : tc(`errors.${runError.kind}`)}
      </Notice>
    );
  }

  return (
    <div role="status" aria-live="polite">
      {content ? <div className="mb-6">{content}</div> : null}
    </div>
  );
}
