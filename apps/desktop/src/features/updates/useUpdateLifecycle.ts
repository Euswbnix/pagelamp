import { useEffect, useRef } from "react";
import type { StartupTasks } from "@/api/provisional";
import { useStartupTasks } from "@/api/queries";
import { useCheckForUpdate, useUpdateStore } from "@/stores/updates";

/**
 * Update work the facade's startup_tasks asks for (the UI only follows it), on every new answer:
 * at launch, hourly, and after an acknowledgement. Remembers the version this launch was updated
 * from (post-update banner), and runs the automatic check when it's due, never while an
 * upgrader still has to read "What's new" (that sheet explains the check first). Once a check
 * is recorded, the facade stops reporting it as due. Mount once, in the app shell.
 */
export function useUpdateLifecycle() {
  const tasks = useStartupTasks();
  const check = useCheckForUpdate();
  const handled = useRef<StartupTasks | null>(null);
  const data = tasks.data;

  useEffect(() => {
    // Query results keep their identity when nothing changed, so each answer is handled once.
    if (!data || handled.current === data) return;
    handled.current = data;
    if (data.updated_from && useUpdateStore.getState().updatedFrom === null) {
      useUpdateStore.setState({ updatedFrom: data.updated_from });
    }
    if (data.whats_new || !data.update_check_due) return;
    void check();
  }, [data, check]);
}
