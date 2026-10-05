import { useIsMutating, useQueryClient } from "@tanstack/react-query";
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

/**
 * The shell's tick makes the page ask again only when its last question is older than this. The
 * page's own hourly timer comes first whenever it runs on time: the tick is for when it doesn't.
 */
export const TICK_REREAD_MS = 65 * 60 * 1000;

/**
 * Whether the student is known to be here right now: something they did was noted in the last
 * half minute. Bounded both ways: a clock set back after their action mustn't keep it "just now".
 */
function studentKnownHere(): boolean {
  const left = useSyncStore.getState().attendedUntil - Date.now();
  return left > 0 && left <= ATTENDED_WINDOW_MS;
}

/**
 * A dialog in a window where nothing was pressed, and that didn't come to the front, for this
 * long was left open. It holds a light sync back no longer.
 */
export const DIALOG_LEFT_MS = 15 * 60 * 1000;

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
 * - unattended: the hourly re-read, always, also with the window in front (asked by this page's
 *   timer, or at the shell's tick when that timer is late); a launch the student didn't see
 *   (the window started hidden); a page that was loaded again, which the system does by
 *   itself; and a window that gained focus, which it can with nobody there (another app quits
 *   at night and this window comes to the front). Those wait for the window to gain focus and
 *   then for the student's first input in it. A window that started hidden waits for that
 *   input from the start: the focus that comes with showing it may never be heard.
 *
 * Whether the window is visible, in front or focused is never taken for the student being here:
 * it can be all three for a night with nobody there.
 *
 * It never starts while something else is going on (a sync, an update being installed, a dialog,
 * "What's new"); it asks again when that is over and lets the new answer decide. One thing in
 * the way gives way: a dialog that was plainly left open (nothing pressed in the window, and
 * the window not come to the front, for a quarter of an hour) holds a light sync back no
 * longer; a full sync waits for every dialog. It never opens anything, shows no message and
 * takes no focus; a run that goes wrong stays quiet (stores/sync).
 * Nothing else may start a sync without the student: no link, argument or event. The shell's
 * tick only makes this hook ask the facade again, as its own timer does.
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
  // Something the app is writing right now (removing or restoring a course, clearing removed
  // ones, a download for the calendar): several of these hold the lock a sync needs. A sync
  // started into one is refused, and the facade counts the refused start as its try. (Not a
  // write that is only waiting to be sent: it holds nothing, and may wait for the window.)
  const writing = useIsMutating({ predicate: (mutation) => !mutation.state.isPaused }) > 0;

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
  // That answer came while something held the sync lock, so it says "not due" whatever is true.
  const underLock = useRef(false);
  const dialogs = useRef<MutationObserver | null>(null);
  // A dialog closed: look at what is in the way again.
  const [closed, setClosed] = useState(0);

  const reread = useCallback(
    () => client.invalidateQueries({ queryKey: queryKeys.startupTasks() }),
    [client],
  );

  // The first input after the window gained focus is the student being here; later ones say
  // nothing new (the hourly re-read stays unattended however long the student works here).
  const awaitingInput = useRef(false);
  const lastInputAt = useRef(0);
  const lastFocusAt = useRef(0);

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
      } else if (first) {
        // Started hidden (at login): nobody saw this launch, so nothing is noted. But the
        // student's first input in this window is their coming, also when the focus that came
        // with showing it was never heard: it can come before this page listens, or not at
        // all. A window that isn't shown gets no input, and the proof is still the input, never
        // the showing. Not for a page that was loaded again: its window may have been in use
        // for hours.
        awaitingInput.current = true;
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

  const askSoon = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What is in the way right now, for the handlers below.
  const busy = useRef(false);
  busy.current = running || otherProcess || installing || writing;

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
        lastFocusAt.current = Date.now();
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

  // The shell's ticker, every quarter of an hour: this page's own hourly timer may not run on
  // time in a window that is out of sight. A tick only ever asks again, and only when the last
  // question is old; what the answer starts is decided below, as for any answer. It says
  // nothing about the student: it notes nothing and arms nothing.
  useEffect(
    () =>
      api.onStartupCheck(() => {
        const state = client.getQueryState<StartupTasks>(queryKeys.startupTasks());
        // No question was asked yet (the launch asks), or one is on its way: several ticks at
        // once, after a sleep, ask once.
        if (state?.fetchStatus !== "idle") return;
        const asked = Math.max(state.dataUpdatedAt, state.errorUpdatedAt);
        if (!asked) return;
        // What the student does asks by itself, and its answer may start a full sync: a tick
        // that falls into that half minute leaves it alone.
        if (studentKnownHere()) return;
        // From the last question, answered or not: one that failed isn't asked again every
        // quarter of an hour. (A clock that went back: ask.)
        const age = Date.now() - asked;
        if (age >= TICK_REREAD_MS || age < 0) void reread();
      }),
    [api, client, reread],
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
    const blocked = running || otherProcess || installing || writing || dialogOpen();
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
    // What holds the sync lock right now: a sync, here or in another process, or a write.
    const locked = running || otherProcess || writing;
    if (handled.current === answer) {
      // Nothing new was answered; at most, what was in the way is gone, or something else is
      // in the way now (a dialog opened while a sync was running). When the lock is free
      // again but a dialog still keeps a sync waiting, the answer that was taken under the
      // lock is asked for again all the same: it said nothing, and the others wait on it.
      if (waiting.current && (!blocked || (underLock.current && !locked))) {
        waiting.current = false;
        underLock.current = false;
        void reread();
      } else if (waiting.current) {
        watchDialogs();
      }
      return;
    }
    handled.current = answer;
    waiting.current = false;
    underLock.current = false;
    const store = useSyncStore.getState();
    // Nothing automatic starts for this answer: the app's other automatic work (clearing
    // removed courses, Monday's note) may go ahead on it. Until then it waits, so that a sync
    // that is due goes first: it needs the same lock, and brings what the note is about.
    const clear = () => useSyncStore.setState({ clearedAnswer: answer });
    if (data.whats_new || noSources) {
      clear();
      return;
    }

    const attended = studentKnownHere();
    const trigger: AutoSyncTrigger | null =
      attended && data.sync_due.attended
        ? "attended"
        : data.sync_due.unattended
          ? "unattended"
          : null;
    if (!trigger) {
      // While anything holds the lock the facade answers "not due", whatever is stale: ask
      // again when it has ended (a sync of one source leaves the others as old as they were;
      // a write hides a sync that is due).
      if (locked) {
        waiting.current = true;
        underLock.current = true;
      } else clear();
      return;
    }
    // A dialog that was left open: in a window that sits in the tray it stays for days. Open,
    // it says nothing about anyone being at it, so once nothing was pressed here and the window
    // didn't come to the front for a quarter of an hour (or the clock went back, and nothing
    // can be told), a light sync goes ahead under it. A full sync waits for every dialog, and
    // a light one for whatever else is in its way.
    const untouched =
      Date.now() - Math.max(lastInputAt.current, lastFocusAt.current, api.pageLoadedAt());
    const leftDialog =
      trigger === "unattended" &&
      !locked &&
      !installing &&
      (untouched >= DIALOG_LEFT_MS || untouched < 0);
    if (blocked && !leftDialog) {
      waiting.current = true;
      underLock.current = locked;
      watchDialogs();
      // Only a dialog or an update being installed is in the way: the sync waits for it, and
      // that can be long (a dialog left open in a window that sits in the tray). The others
      // don't wait that long: they go ahead as they did, and the sync follows when it can.
      if (!locked) clear();
      return;
    }
    // A backstop next to the facade's own clock: not so soon after the last automatic start
    // of this kind or the last attended one, nor right after the student stopped a sync.
    // (Bounded like the window above.)
    const hold = store.noAutomaticBefore[trigger] - Date.now();
    if (hold > 0 && hold <= AUTO_SYNC_MIN_GAP_MS) {
      clear();
      return;
    }
    // Under a dialog that was left, nobody is watching this run, whatever is open: the
    // capsule's own details are such a dialog, and the store would take them for the student
    // looking on and keep a run that goes wrong on screen.
    if (blocked && leftDialog) useSyncStore.setState({ watched: false });
    // Afterwards the cached answer must stop saying "due" (and the others get their turn on
    // the answer that follows, however this run ends).
    void startSync(undefined, { automatic: trigger }).then(reread);
  }, [
    answer,
    loaded,
    launch,
    noSources,
    running,
    otherProcess,
    installing,
    writing,
    closed,
    client,
    startSync,
    reread,
  ]);
}

/**
 * For the app's other automatic work at an answer of `startup_tasks` (clearing removed courses,
 * Monday's note): the number of the answer now in the cache once the automatic sync has looked
 * at it and starts nothing for it, and no sync is running; null until then. A sync that is due
 * goes first: the purge needs the same lock (the sync does it itself as it starts), and the
 * note is about what the sync brings. However that sync ends, the answer asked after it is
 * cleared like any other.
 */
export function useAfterAutoSync(): number | null {
  const client = useQueryClient();
  const cleared = useSyncStore((s) => s.clearedAnswer);
  const running = useSyncStore((s) => s.running);
  const current = client.getQueryState(queryKeys.startupTasks())?.dataUpdateCount ?? 0;
  return cleared > 0 && cleared === current && !running ? cleared : null;
}
