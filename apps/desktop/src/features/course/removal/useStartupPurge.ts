import { useEffect, useRef } from "react";
import { useStartupTasks } from "@/api/queries";
import { usePurgeRemovedCourses } from "@/api/removalQueries";
import { useAfterAutoSync } from "@/features/sources/useAutoSync";
import { REMOVAL_UI } from "./availability";

/**
 * The app-start purge (calendar design §8.3): when the facade says removed courses wait for it
 * (`startup_tasks.purge_due`: due, or a Trash move left files), run it once per launch. It goes
 * through the install gate like any work; a failure leaves it due for the next launch. Mount
 * once, in the app shell.
 *
 * It waits for the automatic sync's turn (`useAfterAutoSync`): both take the sync lock, and a
 * sync that gets the lock does the purge itself as it starts. One that is refused, or isn't due
 * after all, doesn't: then the answer after it still says the purge is due, and this hook runs
 * it.
 */
export function useStartupPurge(enabled: boolean = REMOVAL_UI) {
  const tasks = useStartupTasks();
  const purge = usePurgeRemovedCourses();
  const turn = useAfterAutoSync();
  const started = useRef(false);
  const due = tasks.data?.purge_due === true;
  const { mutate } = purge;
  useEffect(() => {
    if (!enabled || !due || started.current || turn === null) return;
    started.current = true;
    mutate({ removedIds: null, permanentIfNoTrash: false });
  }, [enabled, due, turn, mutate]);
}
