import { useIsMutating, useQueryClient } from "@tanstack/react-query";
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
 * become a click, and for the window to go if the input was a key that hides it at once.
 */
export const INPUT_ASK_DELAY_MS = 300;

/** A press this soon before the window gained focus is taken for the one that brought it there. */
export const INPUT_BEFORE_FOCUS_MS = 1000;

/**
 * A press or a key held for longer than this is taken for one whose release was never heard.
 * Nothing looks again when that time is over: a start it kept waiting comes with the next
 * release, loss of focus, focus or hourly answer.
 */
export const HELD_MS = 60 * 1000;

/**
 * The shell's tick makes the page ask again only when its last question is older than this. The
 * page's own hourly timer comes first whenever it runs on time: the tick is for when it doesn't.
 * Where only the tick asks, the question comes 65 to 80 minutes after the last one at first
 * (the first quarter-hour tick past the 65), and every 75 minutes from then on, not every hour.
 */
export const TICK_REREAD_MS = 65 * 60 * 1000;

/**
 * A dialog in a window where nothing was pressed or typed, and that didn't come to the front,
 * for this long was left open (scrolling in it is not heard, and doesn't count). Nothing looks at it when the time is up: the first answer that finds it
 * untouched for this long starts a light sync under it (the page's hour, or the shell's tick).
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
 * - attended: the student just did something here: opened PageLamp, closed "What's new",
 *   changed the setting, or pressed or typed in its window. Attended needs something the
 *   student did: opening PageLamp in a window that is shown, or a real input in the window;
 *   after a hidden start or a focus, the first input counts (the ones after it say nothing
 *   new). Each is noted where it happens, never inferred from what an answer says. Scrolling
 *   is no such thing: it takes no key and no button, and a window can get it without being
 *   in front (lib/studentInput);
 * - unattended: the regular re-read, always, also with the window in front (asked by this
 *   page's timer once an hour, or at the shell's tick when that timer is late, which then
 *   comes to every 75 minutes: `TICK_REREAD_MS`); a launch the student didn't see
 *   (the window started hidden); a page that was loaded again, which the system does by
 *   itself; and a window that gained focus, which it can with nobody there (another app quits
 *   at night and this window comes to the front). Those wait for the student's first input
 *   in the window, while it has focus: the wait ends when the window loses focus.
 *   A window that started hidden waits for it from the start, not from a focus: the focus that
 *   comes with showing such a window may never be heard. Once a loss of focus was heard, a
 *   focus comes first there too (its events do reach this page).
 *   The input counts a moment later, if the window still has focus then: a key that hides
 *   the window at once (Command+H, Command+M, Command+W where closing hides it) notes
 *   nothing. A press whose click takes the focus elsewhere later than that moment (a link
 *   that opens the browser) does count: it was a deliberate press in this window.
 *
 * Whether the window is visible, in front or focused is never taken for the student being here:
 * it can be all three for a night with nobody there.
 *
 * It never starts while something else is going on (a sync, an update being installed, a dialog,
 * "What's new", a button or a key the student is holding down: what they press must find no run
 * of ours in its way); it asks again when that is over and lets the new answer decide. One thing in
 * the way gives way: a dialog that was plainly left open (nothing pressed in the window, and
 * the window not come to the front, for a quarter of an hour). The first answer that finds it
 * so starts a light sync under it; a full sync waits for every dialog. It never opens
 * anything, shows no message and takes no focus; a run that goes wrong stays quiet
 * (stores/sync).
 * Nothing else may start a sync without the student: no link, argument or event. An event from
 * the shell can make this hook ask the facade again (the ticker's does, as this page's own
 * timer does); only what the student does makes a run attended.
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
  // That answer came while something held the sync lock, so it says "not due" whatever is true;
  // or while the student held something down, so nothing was done about it. Either way it is
  // asked for again once the lock is free, whatever else is in the way by then.
  const underLock = useRef(false);
  const dialogs = useRef<MutationObserver | null>(null);
  // A dialog closed, or what the student held down came up: look at what is in the way again.
  const [again, setAgain] = useState(0);

  const reread = useCallback(
    () => client.invalidateQueries({ queryKey: queryKeys.startupTasks() }),
    [client],
  );

  // Whether the window has focus, as far as its events have said (null: none came yet).
  const focused = useRef<boolean | null>(null);
  // The first input after the window gained focus is the student being here; later ones say
  // nothing new (the hourly re-read stays unattended however long the student works here). The
  // wait ends when the window loses focus again.
  const awaitingInput = useRef(false);
  // When something was last pressed or typed in the window, whatever its focus, and when it
  // last came to the front, on the steady clock (null: not yet): for a dialog that was left.
  const lastInputSteady = useRef<number | null>(null);
  const lastFocusSteady = useRef<number | null>(null);
  // The last press in the window while it was not known to have focus (none of its focus
  // events came yet, or the last one was a loss), on both clocks: see the focus below.
  const pressedWithoutFocus = useRef<{ wall: number; steady: number } | null>(null);
  // The input that waits to count (see `studentIsHere`); null once the window lost focus.
  const noteSoon = useRef<{ by: StudentAction; wall: number; steady: number } | null>(null);
  // Since when (steady clock) the student has held a button or a key down; null: they don't.
  const down = useRef<{ pointer: number | null; key: number | null }>({ pointer: null, key: null });

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
        // How long ago that was: the longer of what the two clocks say. The wall clock counts
        // a sleep, which the steady one may not; the steady one isn't moved when the clock is
        // set back, which on the wall clock can make a load from hours ago look recent. A
        // load that lies ahead on the wall clock can't be dated at all (the clock was set
        // back by more than the time that passed, and the steady clock may have slept
        // through most of it): nothing is noted, and the launch starts unattended.
        const now = Date.now();
        const wallAgo = now - api.pageLoadedAt();
        if (wallAgo >= 0) {
          const ago = Math.max(wallAgo, steadyNow() - api.pageLoadedSteady());
          // From the reading the age was taken at: a second reading could be a millisecond
          // on, and the mark that much late.
          useSyncStore.getState().noteStudentAction("launch", now - ago);
        }
      } else if (first) {
        // Started hidden (at login): nobody saw this launch, so nothing is noted. But the
        // student's first input in this window is their coming, also when the focus that came
        // with showing it was never heard: it can come before this page listens, or not at
        // all. A window that isn't shown gets no input, and the proof is still the input, never
        // the showing. Not for a page that was loaded again: its window may have been in use
        // for hours. (Like every wait, this one ends when a loss of focus is heard.)
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
  const lookSoon = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What is in the way right now, for the handlers below.
  const busy = useRef(false);
  busy.current = running || otherProcess || installing || writing;

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
      noteSoon.current = { by, ...(since ?? { wall: Date.now(), steady: steadyNow() }) };
      // Counted and asked a moment later, for two reasons. A key that hides the window at
      // once is not the student at work in it: on macOS Command+H and Command+M reach the
      // page first (wry leaves Command+key to the web view), and when the window has lost
      // focus by then, the input counts for nothing (see the blur below; that the loss
      // always comes within this moment has not been measured). And what they pressed must
      // act first: a press reaches the page before the click it makes, and "Sync" must find
      // no automatic run in its way. (A press held for longer than that moment keeps the
      // start waiting by itself: see `holding`.)
      if (askSoon.current) clearTimeout(askSoon.current);
      askSoon.current = setTimeout(() => {
        askSoon.current = null;
        const input = noteSoon.current;
        noteSoon.current = null;
        if (input) {
          // For when it happened: by the clock that says longer ago.
          const ago = Math.max(Date.now() - input.wall, steadyNow() - input.steady);
          useSyncStore.getState().noteStudentAction(input.by, Date.now() - ago);
          // A sync may have begun since the input (the light one that the focus's answer
          // started). While it runs the facade answers "not due", so nothing further down
          // sees a start that is kept waiting: the next input counts again from here. (A
          // dialog that opened since, on this very press's click, is seen there.)
          if (busy.current) awaitingInput.current = true;
        }
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
        const now = { wall: Date.now(), steady: steadyNow() };
        focused.current = true;
        lastFocusSteady.current = now.steady;
        // The press that brought the window to the front can reach the page a moment before
        // this event does (Windows, Linux): it is the student all the same. Only a press or
        // a click made while the window was not known to have focus is that, once, and "a
        // moment before" on both clocks: a press from before a sleep is no such press (the
        // steady clock may not have moved since), and neither is an old one that the wall
        // clock, set back, shows as a second ago.
        const pressed = pressedWithoutFocus.current;
        pressedWithoutFocus.current = null;
        const justBefore = (since: number) => since >= 0 && since <= INPUT_BEFORE_FOCUS_MS;
        if (
          pressed &&
          justBefore(now.wall - pressed.wall) &&
          justBefore(now.steady - pressed.steady)
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
  // for their first input is over until the next focus (also the one a hidden start began),
  // and nothing counts as held any more. An input from a moment ago that hasn't counted yet
  // never will: it may be what hid the window. The question that waits with it is still asked
  // (it may be the focus's own).
  useEffect(
    () =>
      api.onWindowBlur(() => {
        focused.current = false;
        awaitingInput.current = false;
        noteSoon.current = null;
        down.current = { pointer: null, key: null };
        released();
      }),
    [api, released],
  );

  // The shell's ticker, every quarter of an hour: this page's own hourly timer may not run on
  // time in a window that is out of sight. A tick only ever asks again, and only when the last
  // question is old; what the answer starts is decided below, as for any answer. It says
  // nothing about the student: it notes nothing and arms nothing.
  useEffect(
    () =>
      api.onStartupCheck(() => {
        const state = client.getQueryState<StartupTasks>(queryKeys.startupTasks());
        // No question was asked yet (the launch asks), or one is on its way. The shell sends
        // one tick a quarter of an hour at most; they pile up when this page was suspended,
        // and then ask once.
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
      api.onStudentInput(
        (kind) => {
          const now = { wall: Date.now(), steady: steadyNow() };
          if (kind === "press") down.current.pointer = now.steady;
          if (kind === "key") down.current.key = now.steady;
          // Something was pressed or typed in the window, whatever its focus: a dialog in it
          // isn't left, and a run that started with nobody here is watched like any other
          // again. (Neither says the student is here for a full sync: that is the wait's.)
          lastInputSteady.current = now.steady;
          if (useSyncStore.getState().unwatched) useSyncStore.setState({ unwatched: false });
          // A press or a click in a window not known to have focus is kept for a second, for
          // the focus it may be bringing; a key brings none. (After a loss of focus no wait
          // is on, and the input says nothing by itself.)
          if (focused.current !== true && kind !== "key") pressedWithoutFocus.current = now;
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
    const blocked = running || otherProcess || installing || writing || dialogOpen();
    // What the student holds down is in the way too, for a moment: the click it becomes may
    // be their own "Sync now", which an automatic run started under the finger would turn
    // away.
    const held = holding();
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
    // What holds the sync lock right now: a sync, here or in another process, or a write.
    const locked = running || otherProcess || writing;
    if (handled.current === answer) {
      // Nothing new was answered; at most, what was in the way is gone, or something else is
      // in the way now (a dialog opened while a sync was running). When the lock is free
      // again but a dialog still keeps a sync waiting, the answer that was taken under the
      // lock is asked for again all the same: it said nothing, and the others wait on it.
      // (Not while the student still holds something down: its release looks again.)
      if (waiting.current && !held && (!blocked || (underLock.current && !locked))) {
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
    // didn't come to the front for a quarter of an hour, a light sync goes ahead under it. A
    // full sync waits for every dialog, and a light one for whatever else is in its way.
    // Counted on the steady clock: on a wall clock that was set back every dialog would look
    // left, also one the student is typing in, for as long as the clock is behind. (Through a
    // sleep the steady clock may stand still: the dialog then counts as touched for longer,
    // and the sync waits.)
    // Scrolling is not heard, on purpose (lib/studentInput): someone who only scrolls in a
    // dialog for a quarter of an hour counts as having left it, and a light sync may start
    // under it, unwatched. And what the student holds down was pressed within the last
    // minute, so a dialog is never "left" under a hold: this start never passes over one.
    const untouched =
      steadyNow() -
      Math.max(lastInputSteady.current ?? 0, lastFocusSteady.current ?? 0, api.pageLoadedSteady());
    const leftDialog =
      trigger === "unattended" && !locked && !installing && untouched >= DIALOG_LEFT_MS;
    // A start for the student that is kept waiting (by what they hold down, by a dialog that
    // opened since their input counted): that input may be too old by the time the way is
    // free. Then their next one counts again. Never for the timer's own start: the hourly
    // re-read stays unattended whoever is typing.
    if ((held || blocked) && trigger === "attended" && focused.current !== false) {
      awaitingInput.current = true;
    }
    if (held && !blocked) {
      // Only what they hold down is in the way, and that is over in a moment: the release
      // looks again. Unlike a dialog it doesn't let the app's other automatic work go ahead
      // (no `clear()`): the sync that is due goes first. The question is asked again then,
      // also when a dialog is in the way by then (the release may have opened it): the new
      // answer is dealt with as any answer under a dialog, and the others get their turn.
      waiting.current = true;
      underLock.current = true;
      return;
    }
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
    // (On the wall clock alone, bounded both ways: a clock set back by more than the half
    // hour ends the hold early.)
    const hold = store.noAutomaticBefore[trigger] - Date.now();
    if (hold > 0 && hold <= AUTO_SYNC_MIN_GAP_MS) {
      clear();
      return;
    }
    // Afterwards the cached answer must stop saying "due" (and the others get their turn on
    // the answer that follows, however this run ends). Under a dialog that was left, nobody is
    // watching this run, whatever is open: the capsule's own details are such a dialog, and
    // would be taken for the student looking on. The run carries that mark (`unwatched`) until
    // the student's next input.
    void startSync(undefined, { automatic: trigger, unwatched: blocked && leftDialog }).then(
      reread,
    );
  }, [
    answer,
    loaded,
    launch,
    noSources,
    running,
    otherProcess,
    installing,
    writing,
    again,
    client,
    startSync,
    reread,
    holding,
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
