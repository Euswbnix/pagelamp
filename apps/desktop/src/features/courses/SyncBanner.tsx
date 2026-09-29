import { useQueryClient } from "@tanstack/react-query";
import { CircleAlert, LoaderCircle } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { queryKeys } from "@/api/queries";
import { Button } from "@/components/ui/button";
import { useSyncActivity, useSyncStore } from "@/stores/sync";
import { Notice } from "./parts/Notice";

/**
 * Shows that a sync is running in another process, or why the last run could not start. (This
 * window's own runs show in the accessory bar.) The outer element is a live region that always
 * exists, so changes are announced.
 */
export function SyncBanner() {
  const { t } = useTranslation("courses");
  const { t: tc } = useTranslation();
  const queryClient = useQueryClient();
  const { external } = useSyncActivity();
  const runError = useSyncStore((s) => s.runError);

  let content: ReactNode = null;
  // Only the headline is announced (below); the box itself is ordinary content, so its hint
  // doesn't get re-read.
  let announcement = "";
  if (external) {
    announcement = t("banner.externalTitle");
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
  } else if (runError) {
    announcement = tc("sync.failed");
    content = (
      <Notice
        icon={<CircleAlert className="size-4 text-warning" aria-hidden />}
        title={tc("sync.failed")}
        action={
          <Button
            size="sm"
            variant="ghost"
            onClick={() => useSyncStore.getState().dismissRunError()}
          >
            {tc("actions.close")}
          </Button>
        }
      >
        {runError.kind === "busy" ? tc("sync.busy") : tc(`errors.${runError.kind}`)}
      </Notice>
    );
  }

  return (
    <>
      <p role="status" className="sr-only">
        {announcement}
      </p>
      {content ? <div className="mb-6">{content}</div> : null}
    </>
  );
}
