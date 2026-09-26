import { CircleAlert, CircleCheck, CircleDashed, LoaderCircle } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useStatus } from "@/api/queries";
import { RelativeTime } from "@/components/common/RelativeTime";
import { paths } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { useSyncStore } from "@/stores/sync";

/**
 * Always-visible sync status at the bottom of the sidebar: "Synced 2h ago", "Syncing 2/3…"
 * or "Needs attention". Links to Sources & sync.
 */
export function SyncPill() {
  const { t } = useTranslation();
  const status = useStatus();
  const running = useSyncStore((s) => s.running);
  const order = useSyncStore((s) => s.order);
  const bySource = useSyncStore((s) => s.bySource);

  const sources = status.data?.sources ?? [];
  const failing = sources.some((s) => s.last_error_kind);
  const externalSync = status.data?.sync_in_progress ?? false;

  let icon = <CircleDashed className="size-4" aria-hidden />;
  let text: ReactNode = t("sync.never");
  let tone = "text-muted-foreground";

  if (running || externalSync) {
    const done = order.filter((id) => bySource[id]?.result).length;
    icon = <LoaderCircle className="size-4 animate-spin" aria-hidden />;
    text =
      running && sources.length > 0
        ? t("sync.syncingProgress", { done, total: sources.length })
        : t("sync.syncing");
    tone = "text-foreground";
  } else if (failing) {
    icon = <CircleAlert className="size-4" aria-hidden />;
    text = t("sync.needsAttention");
    tone = "text-destructive";
  } else if (status.data?.last_synced_at) {
    icon = <CircleCheck className="size-4" aria-hidden />;
    text = <SyncedAgo iso={status.data.last_synced_at} />;
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

/** "Synced 3 hours ago" with the relative part as a <time>. */
function SyncedAgo({ iso }: { iso: string }) {
  const { t } = useTranslation();
  // Split the translated sentence around {{when}} so the time stays a semantic <time> element
  // in any word order ("Synced 3h ago" / "3 小时前同步").
  const [before, after] = t("sync.syncedAgo", { when: "\u0000" }).split("\u0000");
  return (
    <>
      {before}
      <RelativeTime iso={iso} />
      {after}
    </>
  );
}
