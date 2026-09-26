import { LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useStartSync, useSyncActivity } from "@/stores/sync";

/** "Sync now" for every source. Hidden until a source exists; disabled while any sync runs. */
export function SyncNowButton() {
  const { t: tc } = useTranslation();
  const { running, busy, sources } = useSyncActivity();
  const startSync = useStartSync();

  if (sources.length === 0) return null;
  return (
    // aria-disabled keeps focus on the button while it can't be used (disabled would drop it).
    <Button
      aria-disabled={busy || undefined}
      className="aria-disabled:opacity-50"
      onClick={() => {
        if (!busy) void startSync();
      }}
    >
      {running ? <LoaderCircle className="animate-spin" aria-hidden /> : <RefreshCw aria-hidden />}
      {tc("actions.syncNow")}
    </Button>
  );
}
