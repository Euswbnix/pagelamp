import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useApi } from "@/api/context";
import { queryKeys, useStartupTasks, useStatus } from "@/api/queries";
import type { AutoSyncTrigger, StartupTasks } from "@/api/types";
import {
  ATTENDED_WINDOW_MS,
  AUTO_SYNC_MIN_GAP_MS,
  useStartSync,
  useSyncStore,
} from "@/stores/sync";
import { useUpdateStore } from "@/stores/updates";

/** Coming back to the window asks again once the last answer is this old. */
export const FOCUS_REREAD_MS = 10 * 60 * 1000;

/** How long to wait to hear whether this page load is the launch, before taking it for none. */
export const LAUNCH_REPLY_MS = 5000;

/** How long after the student's input the question is asked: time for a press to become a click. */
export const INPUT_ASK_DELAY_MS = 300;

/** An input this soon before the window gained focus is the one that brought it to the front. */
export const INPUT_BEFORE_FOCUS_MS = 1000;

function dialogOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null;
}

/**
 * PageLamp syncs by itself while it is open. The facade decides when one is due
 * (`startup_tasks.sync_due`: the setting, how old the data is, retries); this hook only asks and
 * starts the same sync as the Sync button, marked with what triggered it:
 *
 * - attended: the student just did something here (opened PageLamp, pressed, typed or scrolled
 *   in its window after coming back to it, closed "What's new", changed the setting). Each is
 *   noted where it happens, never inferred from what an answer says;
 * - unattended: the hourly re-read, always, also with the window in front; a launch the
 *   student didn't see (the window started hidden); a page that was loaded again, which the
 *   system does by itself; and a window that gained focus, which it can with nobody there
 *   (another app quits at night and this window comes to the front). Those wait for the
 *   window to gain focus and then for the student's first input in it.
 *
 * Whether the window is visible, in front or focused is never taken for the student being here:
 * it can be all three for a night with nobody there.
 *
 * It never starts while something else is going on (a sync, an update being installed, a dialog,
 * "What's new"); it asks again when that is over and lets the new answer decide. It never opens
 * anything, shows no message and takes no focus; a run that goes wrong stays quiet (stores/sync).
 * Nothing else may start a sync without the student: no link, argument or event.
 *
 * Mount once, in the app shell (so never during onboarding, which runs the first sync itself).
 */
export function useAutoSync() {
  const api = useApi();
  const client = useQueryClient();
  // Keeps the question asked: at launch, hourly, after each acknowledgement.
  useStartupTasks();
  const status = useStatus();
  const startSync = useStartSync();
  const running = useSyncStore((s) => s.running);
  const installing = useUpdateStore(
    (s) => s.install.phase !== "idle" && s.install.phase !== "failed",
  );
  const loaded = status.data !== undefined;
  const noSources = status.data?.sources.length === 0;
  const otherProcess = status.data?.sync_in_progress === true;

  // Answers are told apart by their number, not by what they say or when they came: an answer
  // equal to the one before it keeps its identity, and two can arrive in the same millisecond.
  const answers = useCallback(
    () => client.getQueryState(queryKeys.startupTasks())?.dataUpdateCount ?? 0,
    [client],
  );
  const [answer, setAnswer] = useState(answers);
  useEffect(() => {
    const [key] = queryKeys.startupTasks();
    const unsubscribe = client.getQueryCache().subscribe((event) => {
      if (
        event.type === "updated" &&
        event.action.type === "success" &&
        event.query.queryKey[0] === key
      ) {
        setAnswer(event.query.state.dataUpdateCount);
      }
    });
    setAnswer(answers());
    return unsubscribe;
  }, [client, answers]);

  // An answer already in the cache when the shell mounts (back from /welcome) was dealt with by
  // the shell that read it. With none, this page has just loaded: the launch, when the student
  // opened PageLamp, or a reload, which needs nobody (see `launch` below).
  const [atMount] = useState(answer);
  const handled = useRef(atMount);
  // A due answer found something in the way; ask again when it is gone.
  const waiting = useRef(false);
  const dialogs = useRef<MutationObserver | null>(null);
  // A dialog closed: look at what is in the way again.
  const [closed, setClosed] = useState(0);

  const reread = useCallback(
    () => client.invalidateQueries({ queryKey: queryKeys.startupTasks() }),
    [client],
  );

  // Whether this page load is the launch: null until Rust has said (it knows whether the process
  // loaded the page before). No answer is acted on before that, so a reload can't slip through
  // as "the student just opened PageLamp" and run an attended sync with nobody there.
  const [launch, setLaunch] = useState<boolean | null>(atMount === 0 ? null : false);
  useEffect(() => {
    if (atMount !== 0) return;
    let settled = false;
    const settle = (first: boolean) => {
      if (settled) return;
      settled = true;
      // The launch of a window the student sees. Not one started hidden (at login), and never
      // a reload; no reply, a failed one or a late one counts as a reload too. The student's
      // action is the launch itself, when the page loaded: a shell that only appears long after
      // (a start page that got through by itself hours later) finds that moment long past.
      if (first && !api.startedHidden()) {
        useSyncStore.getState().noteStudentAction(api.pageLoadedAt());
      }
      setLaunch(first);
    };
    const timer = setTimeout(() => settle(false), LAUNCH_REPLY_MS);
    api.firstPageLoad().then(settle, () => settle(false));
    return () => {
      settled = true;
      clearTimeout(timer);
    };
  }, [atMount, api]);

  useEffect(() => () => dialogs.current?.disconnect(), []);

  // The first input after the window gained focus is the student being here; later ones say
  // nothing new (the hourly re-read stays unattended however long the student works here).
  const awaitingInput = useRef(false);
  const lastInputAt = useRef(0);
  const askSoon = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What is in the way right now, for the handlers below.
  const busy = useRef(false);
  busy.current = running || otherProcess || installing;

  /** Asks again if the answer is old or said a sync was due. */
  const ask = useCallback(
    (unattendedToo: boolean) => {
      const state = client.getQueryState<StartupTasks>(queryKeys.startupTasks());
      const due = state?.data?.sync_due;
      if (
        !state?.dataUpdatedAt ||
        Date.now() - state.dataUpdatedAt > FOCUS_REREAD_MS ||
        due?.attended ||
        (unattendedToo && due?.unattended)
      ) {
        void reread();
      }
    },
    [client, reread],
  );

  /** The student did something in the window, now or at `at`: they are known to be here. */
  const studentIsHere = useCallback(
    (unattendedToo: boolean, at?: number) => {
      // With something in the way (a sync, an update being installed, a dialog) this input
      // may be 30 seconds old before anything can start: then their next one counts again.
      awaitingInput.current = busy.current || dialogOpen();
      useSyncStore.getState().noteStudentAction(at);
      // Asked a moment later, so that a full sync that is due starts now, yet after what the
      // student pressed has acted: a press reaches the page before the click it makes, and
      // "Sync" must find no automatic run in its way.
      if (askSoon.current) clearTimeout(askSoon.current);
      askSoon.current = setTimeout(() => {
        askSoon.current = null;
        ask(unattendedToo);
      }, INPUT_ASK_DELAY_MS);
    },
    [ask],
  );
  useEffect(
    () => () => {
      if (askSoon.current) clearTimeout(askSoon.current);
    },
    [],
  );

  // The window gains focus: ask again if the answer is old, or if it said a sync was due (the
  // student's own sync may have changed that since). Whether the student is here is not known
  // yet: an answer that comes before their first input can only start an unattended sync.
  useEffect(
    () =>
      api.onWindowFocus(() => {
        // The press that brought the window to the front can reach the page a moment before
        // this event does (Windows, Linux): it is the student all the same.
        const sinceInput = Date.now() - lastInputAt.current;
        if (sinceInput >= 0 && sinceInput <= INPUT_BEFORE_FOCUS_MS) {
          studentIsHere(true, lastInputAt.current);
        } else {
          awaitingInput.current = true;
          ask(true);
        }
        // The setting may have been changed elsewhere (the command line).
        void client.invalidateQueries({ queryKey: queryKeys.syncPrefs() });
      }),
    [api, client, ask, studentIsHere],
  );

  useEffect(
    () =>
      api.onStudentInput(() => {
        lastInputAt.current = Date.now();
        if (awaitingInput.current) studentIsHere(false);
      }),
    [api, studentIsHere],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: `closed` only re-runs the check.
  useEffect(() => {
    const data = client.getQueryData<StartupTasks>(queryKeys.startupTasks());
    // "No sources" comes from the status, and whether this load is the launch from Rust: wait
    // for both.
    if (!data || !loaded || launch === null) return;
    const blocked = running || otherProcess || installing || dialogOpen();
    // Dialogs are portalled into <body>; nothing else says when the last one closes.
    const watchDialogs = () => {
      if (!dialogOpen() || dialogs.current) return;
      const observer = new MutationObserver(() => {
        if (dialogOpen()) return;
        observer.disconnect();
        dialogs.current = null;
        setClosed((n) => n + 1);
      });
      observer.observe(document.body, { childList: true });
      dialogs.current = observer;
    };
    if (handled.current === answer) {
      // Nothing new was answered; at most, what was in the way is gone, or something else is
      // in the way now (a dialog opened while a sync was running).
      if (waiting.current && !blocked) {
        waiting.current = false;
        void reread();
      } else if (waiting.current) {
        watchDialogs();
      }
      return;
    }
    handled.current = answer;
    waiting.current = false;
    const store = useSyncStore.getState();
    if (data.whats_new || noSources) return;

    // Bounded both ways: a clock set back after the student's action mustn't keep it "just now".
    const left = useSyncStore.getState().attendedUntil - Date.now();
    const attended = left > 0 && left <= ATTENDED_WINDOW_MS;
    const trigger: AutoSyncTrigger | null =
      attended && data.sync_due.attended
        ? "attended"
        : data.sync_due.unattended
          ? "unattended"
          : null;
    if (!trigger) {
      // While any sync holds the lock the facade answers "not due", whatever is stale: ask
      // again when it has ended (a sync of one source leaves the others as old as they were).
      if (running || otherProcess) waiting.current = true;
      return;
    }
    if (blocked) {
      waiting.current = true;
      watchDialogs();
      return;
    }
    // A backstop next to the facade's own clock: not so soon after the last automatic start
    // of this kind or the last attended one, nor right after the student stopped a sync.
    // (Bounded like the window above.)
    const hold = store.noAutomaticBefore[trigger] - Date.now();
    if (hold > 0 && hold <= AUTO_SYNC_MIN_GAP_MS) return;
    // Afterwards the cached answer must stop saying "due".
    void startSync(undefined, { automatic: trigger }).then(reread);
  }, [
    answer,
    loaded,
    launch,
    noSources,
    running,
    otherProcess,
    installing,
    closed,
    client,
    startSync,
    reread,
  ]);
}
