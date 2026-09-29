import { CircleAlert, CircleCheck, CircleDashed, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { useStatus } from "@/api/queries";
import { formatRelative } from "@/lib/format";
import { paths } from "@/lib/routes";
import { cn } from "@/lib/utils";
import { useSyncStore } from "@/stores/sync";

/**
 * Always-visible sync status at the bottom of the sidebar: "Synced 2h ago", "Syncing…" (another
 * process, e.g. the CLI) or "Needs attention". Links to Sources & sync. This window's own runs
 * show their progress in the accessory bar; meanwhile the pill keeps the last state.
 */
export function SyncPill() {
  const { t, i18n } = useTranslation();
  const status = useStatus();
  const running = useSyncStore((s) => s.running);

  const sources = status.data?.sources ?? [];
  const failing = sources.some((s) => s.last_error_kind);
  const externalSync = !running && (status.data?.sync_in_progress ?? false);

  let icon = <CircleDashed className="size-4" aria-hidden />;
  let text = t("sync.never");
  let tone = "text-muted-foreground";

  if (externalSync) {
    icon = <LoaderCircle className="size-4 animate-spin" aria-hidden />;
    text = t("sync.syncing");
    tone = "text-foreground";
  } else if (failing) {
    icon = <CircleAlert className="size-4" aria-hidden />;
    text = t("sync.needsAttention");
    tone = "text-destructive";
  } else if (status.data?.last_synced_at) {
    icon = <CircleCheck className="size-4" aria-hidden />;
    text = t("sync.syncedAgo", { when: formatRelative(status.data.last_synced_at, i18n.language) });
  }

  return (
    // No live region here: the accessory bar announces this window's runs, the Courses banner
    // another process's. The link's name says both the status and where it goes.
    <Link
      to={paths.sources}
      aria-label={t("sync.pillLabel", { status: text })}
      className={cn(
        "flex items-center gap-2 rounded-md px-3 py-2 text-xs transition-colors hover:bg-sidebar-accent",
        tone,
      )}
    >
      {icon}
      <span>{text}</span>
    </Link>
  );
}
