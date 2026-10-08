import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import type { StudentAction } from "@/api/client";
import { useApi } from "@/api/context";
import { queryKeys, useStartupTasks, useStatus } from "@/api/queries";
import type { AutoSyncTrigger, StartupTasks } from "@/api/types";
import {
  AUTO_SYNC_MIN_GAP_MS,
  steadyNow,
  studentKnownHere,
  useStartSync,
  useSyncStore,
} from "@/stores/sync";
import { useUpdateStore } from "@/stores/updates";

/** Coming back to the window asks again once the last answer is this old. */
export const FOCUS_REREAD_MS = 10 * 60 * 1000;

/** How long to wait to hear whether this page load is the launch, before taking it for none. */
export const LAUNCH_REPLY_MS = 5000;

/**
 * How long after the student's input it counts and the question is asked: time for a press to
 * become a click, and for the window to go if the input was what sent it away.
 */
export const INPUT_ASK_DELAY_MS = 300;

/** A press this soon before the window gained focus is the one that brought it to the front. */
export const INPUT_BEFORE_FOCUS_MS = 1000;

/**
 * A press or a key held for longer than this is taken for one whose release was never heard (a
 * menu of the system's can take it), so that it can't keep every automatic sync from starting.
 */
export const HELD_MS = 60 * 1000;

function dialogOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null;
}

/**
 * PageLamp syncs by itself while it is open. The facade decides when one is due
 * (`startup_tasks.sync_due`: the setting, how old the data is, retries); this hook only asks and
 * starts the same sync as the Sync button, marked with what triggered it:
 *
 * - attended: the student just did something here (opened PageLamp, pressed or typed in its
 *   window after coming back to it, closed "What's new", changed the setting). Each is noted
 *   where it happens, never inferred from what an answer says. Scrolling is no such thing: the
 *   system sends it to whichever window is under the pointer;
 * - unattended: the hourly re-read, always, also with the window in front; a launch the
 *   student didn't see (the window started hidden); a page that was loaded again, which the
 *   system does by itself; and a window that gained focus, which it can with nobody there
 *   (another app quits at night and this window comes to the front). Those wait for the
 *   window to gain focus and then for the student's first input in it, while it has focus.
 *   The input counts a moment later, if the window still has focus then: what is pressed to
 *   send the window away is not the student at work in it.
 *
 * Whether the window is visible, in front or focused is never taken for the student being here:
 * it can be all three for a night with nobody there.
 *
 * It never starts while something else is going on (a sync, an update being installed, a dialog,
 * "What's new", a button or a key the student is holding down: what they press must find no run
 * of ours in its way); it asks again when that is over and lets the new answer decide. It never
 * opens anything, shows no message and takes no focus; a run that goes wrong stays quiet
 * (stores/sync). Nothing else may start a sync without the student: no link, argument or event.
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
  // A dialog closed, or what the student held down came up: look at what is in the way again.
  const [again, setAgain] = useState(0);

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
        useSyncStore.getState().noteStudentAction("launch", api.pageLoadedAt());
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

  // Whether the window has focus, as far as its events have said (null: none came yet).
  const focused = useRef<boolean | null>(null);
  // The first input after the window gained focus is the student being here; later ones say
  // nothing new (the hourly re-read stays unattended however long the student works here). The
  // wait ends when the window loses focus again.
  const awaitingInput = useRef(false);
  // The last press in the window while it had no focus, on both clocks: see the focus below.
  const pressedWithoutFocus = useRef<{ wall: number; steady: number } | null>(null);
  // Since when (steady clock) the student has held a button or a key down; null: they don't.
  const down = useRef<{ pointer: number | null; key: number | null }>({ pointer: null, key: null });
  const askSoon = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lookSoon = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What is in the way right now, for the handlers below.
  const busy = useRef(false);
  busy.current = running || otherProcess || installing;

  /** Asks again if the answer is old or said a sync was due. */
  const ask = useCallback(
    (unattendedToo: boolean) => {
      const state = client.getQueryState<StartupTasks>(queryKeys.startupTasks());
      const due = state?.data?.sync_due;
      // An answer dated after now (the clock was set back since) is as good as an old one.
      const age = state?.dataUpdatedAt ? Date.now() - state.dataUpdatedAt : null;
      if (
        age === null ||
        age > FOCUS_REREAD_MS ||
        age < 0 ||
        due?.attended ||
        (unattendedToo && due?.unattended)
      ) {
        void reread();
      }
    },
    [client, reread],
  );

  /** Whether the student holds a button or a key down right now, as far as was heard. */
  const holding = useCallback(() => {
    const now = steadyNow();
    return [down.current.pointer, down.current.key].some(
      (since) => since !== null && now - since <= HELD_MS,
    );
  }, []);

  /**
   * The student did something in the window, now or at `since` (both clocks): a moment later,
   * if the window still has focus then, they are known to be here.
   */
  const studentIsHere = useCallback(
    (unattendedToo: boolean, by: StudentAction, since?: { wall: number; steady: number }) => {
      // With something in the way (a sync, an update being installed, a dialog) this input
      // may be 30 seconds old before anything can start: then their next one counts again.
      awaitingInput.current = busy.current || dialogOpen();
      const at = since ?? { wall: Date.now(), steady: steadyNow() };
      // Counted and asked a moment later, for two reasons. What the student pressed to send
      // the window away (hide it, close it) is not them at work in it: the window has lost
      // focus by then, and the input counts for nothing (see the blur below). And what they
      // pressed must act first: a press reaches the page before the click it makes, and
      // "Sync" must find no automatic run in its way. (A press held for longer than that
      // moment keeps the start waiting by itself: see `holding`.)
      if (askSoon.current) clearTimeout(askSoon.current);
      askSoon.current = setTimeout(() => {
        askSoon.current = null;
        // For when it happened: by the clock that says longer ago.
        const ago = Math.max(Date.now() - at.wall, steadyNow() - at.steady);
        useSyncStore.getState().noteStudentAction(by, Date.now() - ago);
        ask(unattendedToo);
      }, INPUT_ASK_DELAY_MS);
    },
    [ask],
  );
  useEffect(
    () => () => {
      if (askSoon.current) clearTimeout(askSoon.current);
      if (lookSoon.current) clearTimeout(lookSoon.current);
    },
    [],
  );

  /**
   * What the student held down came up (or is no longer this window's to hear): a moment
   * later, when a release has become its click, what waited for it is looked at again.
   */
  const released = useCallback(() => {
    if (lookSoon.current) clearTimeout(lookSoon.current);
    lookSoon.current = setTimeout(() => {
      lookSoon.current = null;
      if (waiting.current) setAgain((n) => n + 1);
    }, INPUT_ASK_DELAY_MS);
  }, []);

  // The window gains focus: ask again if the answer is old, or if it said a sync was due (the
  // student's own sync may have changed that since). Whether the student is here is not known
  // yet: an answer that comes before their first input can only start an unattended sync.
  useEffect(
    () =>
      api.onWindowFocus(() => {
        focused.current = true;
        // The press that brought the window to the front can reach the page a moment before
        // this event does (Windows, Linux): it is the student all the same. Only a press or
        // a click made while the window had no focus is that, once, and on both clocks: a
        // clock set back must not bring an old press round again.
        const pressed = pressedWithoutFocus.current;
        pressedWithoutFocus.current = null;
        const justBefore = (since: number) => since >= 0 && since <= INPUT_BEFORE_FOCUS_MS;
        if (
          pressed &&
          justBefore(Date.now() - pressed.wall) &&
          justBefore(steadyNow() - pressed.steady)
        ) {
          studentIsHere(true, "press_before_focus", pressed);
        } else {
          awaitingInput.current = true;
          ask(true);
        }
        // The setting may have been changed elsewhere (the command line).
        void client.invalidateQueries({ queryKey: queryKeys.syncPrefs() });
      }),
    [api, client, ask, studentIsHere],
  );

  // The window loses focus: whatever comes next in it isn't the student at work here. The wait
  // for their first input is over until the next focus, and nothing counts as held any more.
  // An input from a moment ago that hasn't counted yet never will: it may be what sent the
  // window away.
  useEffect(
    () =>
      api.onWindowBlur(() => {
        focused.current = false;
        awaitingInput.current = false;
        if (askSoon.current) clearTimeout(askSoon.current);
        askSoon.current = null;
        down.current = { pointer: null, key: null };
        released();
      }),
    [api, released],
  );

  useEffect(
    () =>
      api.onStudentInput(
        (kind) => {
          if (kind === "press") down.current.pointer = steadyNow();
          if (kind === "key") down.current.key = steadyNow();
          // In a window without focus an input says nothing by itself (no wait is on: it
          // ended with the focus). A press or a click is kept for a second, for the focus it
          // may be bringing; a key brings none.
          if (focused.current !== true && kind !== "key") {
            pressedWithoutFocus.current = { wall: Date.now(), steady: steadyNow() };
          }
          if (awaitingInput.current) studentIsHere(false, kind);
        },
        (what) => {
          down.current[what] = null;
          released();
        },
      ),
    [api, studentIsHere, released],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: `again` only re-runs the check.
  useEffect(() => {
    const data = client.getQueryData<StartupTasks>(queryKeys.startupTasks());
    // "No sources" comes from the status, and whether this load is the launch from Rust: wait
    // for both.
    if (!data || !loaded || launch === null) return;
    // (What the student holds down is in the way too: the click it becomes may be their own
    // "Sync now", which an automatic run started under the finger would turn away.)
    const blocked = running || otherProcess || installing || dialogOpen() || holding();
    // Dialogs are portalled into <body>; nothing else says when the last one closes.
    const watchDialogs = () => {
      if (!dialogOpen() || dialogs.current) return;
      const observer = new MutationObserver(() => {
        if (dialogOpen()) return;
        observer.disconnect();
        dialogs.current = null;
        setAgain((n) => n + 1);
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

    const attended = studentKnownHere();
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
      // Kept waiting by what the student holds down, the input that noted them may be too
      // old by the time it comes up: then their next one counts again, as with a sync or a
      // dialog in the way.
      if (trigger === "attended" && holding() && focused.current !== false) {
        awaitingInput.current = true;
      }
      watchDialogs();
      return;
    }
    // A backstop next to the facade's own clock: not so soon after the last automatic start
    // of this kind or the last attended one, nor right after the student stopped a sync.
    // (On the wall clock alone, bounded both ways: a clock set back by more than the half
    // hour ends the hold early.)
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
    again,
    client,
    startSync,
    reread,
    holding,
  ]);
}
