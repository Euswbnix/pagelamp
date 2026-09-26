import { CircleAlert, CircleCheck, CircleDashed, LoaderCircle } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useStatus } from "@/api/queries";
import { SentenceWithTime, WHEN } from "@/components/common/SentenceWithTime";
import { paths } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { useSyncCounts, useSyncStore } from "@/stores/sync";

/**
 * Always-visible sync status at the bottom of the sidebar: "Synced 2h ago", "Syncing 2/3…"
 * or "Needs attention". Links to Sources & sync.
 */
export function SyncPill() {
  const { t } = useTranslation();
  const status = useStatus();
  const running = useSyncStore((s) => s.running);
  const { done, total } = useSyncCounts();

  const sources = status.data?.sources ?? [];
  const failing = sources.some((s) => s.last_error_kind);
  const externalSync = status.data?.sync_in_progress ?? false;

  let icon = <CircleDashed className="size-4" aria-hidden />;
  let text: ReactNode = t("sync.never");
  let tone = "text-muted-foreground";

  if (running || externalSync) {
    icon = <LoaderCircle className="size-4 animate-spin" aria-hidden />;
    text = running && total ? t("sync.syncingProgress", { done, total }) : t("sync.syncing");
    tone = "text-foreground";
  } else if (failing) {
    icon = <CircleAlert className="size-4" aria-hidden />;
    text = t("sync.needsAttention");
    tone = "text-destructive";
  } else if (status.data?.last_synced_at) {
    icon = <CircleCheck className="size-4" aria-hidden />;
    text = (
      <SentenceWithTime
        text={t("sync.syncedAgo", { when: WHEN })}
        iso={status.data.last_synced_at}
      />
    );
  }

  return (
    <Link
      to={paths.sources}
      className={cn(
        "flex items-center gap-2 rounded-md px-3 py-2 text-xs transition-colors hover:bg-sidebar-accent",
        tone,
      )}
    >
      {icon}
      <span aria-live="polite">{text}</span>
    </Link>
  );
}
