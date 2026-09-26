import { useStatus } from "@/api/queries";
import { useSyncStore } from "@/stores/sync";

/**
 * Is a sync running? `running` = started from this window; `external` = another process
 * (e.g. the CLI) holds the sync lock, which the status query reports.
 */
export function useSyncActivity() {
  const status = useStatus();
  const running = useSyncStore((s) => s.running);
  const external = !running && status.data?.sync_in_progress === true;
  return {
    running,
    external,
    busy: running || external,
    sources: status.data?.sources ?? [],
  };
}
