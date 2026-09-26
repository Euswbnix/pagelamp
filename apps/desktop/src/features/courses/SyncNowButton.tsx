import { LoaderCircle, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useStartSync } from "@/stores/sync";
import { useSyncActivity } from "./parts/useSyncActivity";

/** "Sync now" for every source. Hidden until a source exists; disabled while any sync runs. */
export function SyncNowButton() {
  const { t: tc } = useTranslation();
  const { running, busy, sources } = useSyncActivity();
  const startSync = useStartSync();

  if (sources.length === 0) return null;
  return (
    <Button disabled={busy} onClick={() => void startSync()}>
      {running ? <LoaderCircle className="animate-spin" aria-hidden /> : <RefreshCw aria-hidden />}
      {tc("actions.syncNow")}
    </Button>
  );
}
