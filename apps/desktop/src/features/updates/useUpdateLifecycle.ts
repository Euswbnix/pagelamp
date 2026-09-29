import { useEffect, useRef } from "react";
import { useStartupTasks } from "@/api/queries";
import { useCheckForUpdate, useUpdateStore } from "@/stores/updates";

/**
 * Launch-time update work, as the facade's startup_tasks decides (the UI only follows it):
 * remember the version this launch was updated from (post-update banner), and run the automatic
 * check once when it's due, never while an upgrader still has to read "What's new" (that sheet
 * explains the check first). Mount once, in the app shell.
 */
export function useUpdateLifecycle() {
  const tasks = useStartupTasks();
  const check = useCheckForUpdate();
  const checked = useRef(false);
  const data = tasks.data;

  useEffect(() => {
    if (!data) return;
    if (data.updated_from && useUpdateStore.getState().updatedFrom === null) {
      useUpdateStore.setState({ updatedFrom: data.updated_from });
    }
    if (checked.current || data.whats_new || !data.update_check_due) return;
    checked.current = true;
    void check();
  }, [data, check]);
}
