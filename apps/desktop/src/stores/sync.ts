// Live sync progress, shared by the sidebar pill, the Sources screen and onboarding.
// Holds only what SyncEvents carry (labels, progress messages, error kinds) — no secrets.

import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef } from "react";
import { create } from "zustand";
import { useApi } from "@/api/context";
import { type ApiError, toApiError } from "@/api/errors";
import { queryKeys, useStatus } from "@/api/queries";
import type {
  AppStatus,
  AutoSyncTrigger,
  CourseSyncSummary,
  SourceErrorKind,
  SyncEvent,
  SyncStage,
  SyncSummary,
} from "@/api/types";

export interface SourceProgress {
  sourceId: string;
  label: string;
  /** The facade's English step text (CLI, logs); the UI translates `stage` when it is set. */
  message: string | null;
  /** What the step is (translated in the UI), and the course it is about. */
  stage: SyncStage | null;
  course: string | null;
  current: number | null;
  total: number | null;
  warnings: string[];
  /** Set when the source finished. */
  result: { ok: boolean; error: string | null; errorKind: SourceErrorKind | null } | null;
  /**
   * What the run says about each course of the source, once it has ended (a Canvas source that
   * finished without an error). Not after a download, whose own message says what it did, and
   * not after a Stop: a stopped run comes back without a summary.
   */
  courses?: CourseSyncSummary[];
  /** The run ended (e.g. failed as a whole) before this source finished. */
  stopped: boolean;
}

interface SyncState {
  running: boolean;
  /** How many sources this run covers (1 for a single-source sync), when known. */
  total: number | null;
  /** Source ids in the order they started. */
  order: string[];
  bySource: Record<string, SourceProgress>;
  lastSummary: SyncSummary | null;
  /** Error that stopped the whole run (e.g. `busy`). Per-source failures live in bySource. */
  runError: ApiError | null;
  /** The course whose files this run downloads (download_course_files); null for a sync. */
  downloadCourseId: string | null;
  /**
   * What started the current or last run: null = the student, else the trigger of an automatic
   * sync. An automatic run that goes wrong leaves no trace here (see `finish`).
   */
  automatic: AutoSyncTrigger | null;
  /**
   * The current run has something to show. The student's own run has from the moment it begins;
   * an automatic one only from its first event, and until then the last run's result (its
   * progress rows, a failure and its reason) stays exactly as it was.
   */
  started: boolean;
  /**
   * No automatic sync with this trigger starts before this time (ms). A backstop next to the
   * facade's own clocks, kept per trigger: an unattended start holds only the next unattended
   * one for half an hour, so the timer's light sync doesn't cost the student their full sync
   * when they come back ten minutes later. An attended start holds both, and so does the
   * student stopping any sync ("not now").
   */
  noAutomaticBefore: Record<AutoSyncTrigger, number>;
  /**
   * The last automatic run left a problem on a source (the facade decides which failures it
   * records; one that may pass by itself, like no network, it doesn't). The source's card and
   * the pill show it; screen readers hear it once.
   */
  automaticProblem: boolean;
  /** The student is in the capsule or its details: an automatic run's problem stays on screen. */
  watched: boolean;
  /**
   * The automatic run in hand started with nobody at the window, under a dialog that was left
   * open (useAutoSync). Whatever is open then watches nothing, the capsule's own details
   * included: the run ends quietly whatever `watched` says. The mark is this run's: it ends
   * with the run, or with the student's next real input in the window.
   */
  unwatched: boolean;
  /**
   * Until when (ms) an answer to "what's due?" counts as attended: the student just acted. On
   * the wall clock; `attendedUntilSteady` is the same moment on the clock that only runs
   * forward. Read them through `studentKnownHere`: both must hold.
   */
  attendedUntil: number;
  attendedUntilSteady: number;
  /**
   * The number of the `startup_tasks` answer the automatic sync has looked at and starts
   * nothing for (0: none yet). The app's other automatic work waits for it (`useAfterAutoSync`).
   */
  clearedAnswer: number;
  begin: (
    total: number | null,
    downloadCourseId?: string | null,
    automatic?: AutoSyncTrigger | null,
    unwatched?: boolean,
  ) => void;
  apply: (event: SyncEvent) => void;
  /**
   * `recorded`: the facade wrote a problem on a source during this run (its `last_error_kind`
   * is new). Only used for an automatic run, which says it once and shows nothing else.
   */
  finish: (summary: SyncSummary | null, error: ApiError | null, recorded?: boolean) => void;
  /** The student pressed Stop; the run ends at its next file, course or download. */
  stopping: boolean;
  /** The last run ended because the student stopped it (not a failure). */
  stoppedByUser: boolean;
  requestStop: () => void;
  /** Hide the "sync failed" message (it stays hidden until the next run). */
  dismissRunError: () => void;
  /**
   * "Hide" on the last run's result: nothing of it stays on screen. A run on its way that has
   * shown nothing yet (an automatic one) goes on untouched.
   */
  hideRun: () => void;
  /**
   * The student opened the app or did something in its window: just now, or at `at` (ms) when
   * it is noted afterwards (the launch, which happened when the page loaded; the press that
   * brought the window to the front). What is noted afterwards never takes the place of an
   * action after it that still counts; a mark that doesn't count now (it ran out, or the clock
   * was set back under it) is replaced. A time after now notes nothing: the clock was set back
   * since, and how long ago it was can't be told.
   */
  noteStudentAction: (at?: number) => void;
  /** A source or a course was removed: the last run's lines may name what is gone. */
  forgetCourseLines: () => void;
  reset: () => void;
}

/** How long after the student's action an answer to "what's due?" counts as attended. */
export const ATTENDED_WINDOW_MS = 30_000;

/**
 * Whether the student is known to be here right now: something they did was noted within the
 * last half minute. On two clocks, and both must say so:
 * - the wall clock ends it when the computer slept meanwhile (the other clock may stand still
 *   through a sleep);
 * - the steady clock ends it when the wall clock was set back. On the wall clock alone "just
 *   now" is false at first, and true again for half a minute when the clock passes the old
 *   mark: with a full sync due then, it would start as attended with nobody there.
 * Each is bounded both ways: a mark far in the future is no "just now" either.
 */
export function studentKnownHere(): boolean {
  const { attendedUntil, attendedUntilSteady } = useSyncStore.getState();
  const justNow = (left: number) => left > 0 && left <= ATTENDED_WINDOW_MS;
  return justNow(attendedUntil - Date.now()) && justNow(attendedUntilSteady - steadyNow());
}

/**
 * The clock that only runs forward, in whole milliseconds like the wall clock. Its readings
 * can have fractions, and sums of those don't come out exact: a mark noted at 2770.8 would lie
 * 30000.000000000004 ms ahead at that same reading, over its bound.
 */
function steadyNow(): number {
  return Math.floor(performance.now());
}

/**
 * The least time between two automatic starts of one kind, whatever the answers say. An attended
 * start and the student's Stop hold both kinds for as long (`noAutomaticBefore`).
 */
export const AUTO_SYNC_MIN_GAP_MS = 30 * 60 * 1000;

/**
 * The holds after a start (or a Stop) at `now`: an attended one holds both triggers, an
 * unattended one only its own.
 */
function holdFrom(
  now: number,
  trigger: AutoSyncTrigger,
  held: Record<AutoSyncTrigger, number>,
): Record<AutoSyncTrigger, number> {
  const until = now + AUTO_SYNC_MIN_GAP_MS;
  return trigger === "attended"
    ? { unattended: until, attended: until }
    : { ...held, unattended: until };
}

/** No run to show: before the first one, after "Hide", after an automatic run that went wrong. */
const noRun = {
  running: false,
  total: null,
  order: [],
  bySource: {},
  lastSummary: null,
  runError: null,
  downloadCourseId: null,
  stopping: false,
  stoppedByUser: false,
  automatic: null,
  started: false,
  unwatched: false,
} satisfies Partial<SyncState>;

const idle = {
  ...noRun,
  noAutomaticBefore: { unattended: 0, attended: 0 },
  automaticProblem: false,
  watched: false,
  attendedUntil: 0,
  attendedUntilSteady: 0,
  clearedAnswer: 0,
} satisfies Partial<SyncState>;

export const useSyncStore = create<SyncState>()((set) => ({
  ...idle,
  begin: (total, downloadCourseId = null, automatic = null, unwatched = false) =>
    set((state) =>
      automatic
        ? {
            // The last run's result stays until this one has something to show (`apply`).
            running: true,
            total,
            downloadCourseId,
            stopping: false,
            automatic,
            unwatched,
            started: false,
            automaticProblem: false,
            noAutomaticBefore: holdFrom(Date.now(), automatic, state.noAutomaticBefore),
          }
        : {
            running: true,
            total,
            order: [],
            bySource: {},
            runError: null,
            downloadCourseId,
            stopping: false,
            stoppedByUser: false,
            automatic: null,
            unwatched: false,
            started: true,
            automaticProblem: false,
          },
    ),
  apply: (event) =>
    set((state) => {
      // An automatic run's first event: from here on it is the run on screen.
      const first = !state.started;
      const order = first ? [] : state.order;
      const bySource = first ? {} : state.bySource;
      const prev = bySource[event.source_id];
      const base: SourceProgress = prev ?? {
        sourceId: event.source_id,
        label: event.source_id,
        message: null,
        stage: null,
        course: null,
        current: null,
        total: null,
        warnings: [],
        result: null,
        stopped: false,
      };
      let next: SourceProgress;
      switch (event.type) {
        case "source_started":
          next = { ...base, label: event.label };
          break;
        case "progress":
          next = {
            ...base,
            message: event.message,
            stage: event.stage ?? null,
            course: event.course ?? null,
            current: event.current ?? null,
            total: event.total ?? null,
          };
          break;
        case "warning":
          next = { ...base, warnings: [...base.warnings, event.message] };
          break;
        case "source_finished":
          next = {
            ...base,
            result: {
              ok: event.ok,
              error: event.error ?? null,
              errorKind: event.error_kind ?? null,
            },
          };
          break;
      }
      return {
        ...(first ? { started: true, runError: null, stoppedByUser: false } : null),
        order: prev ? order : [...order, event.source_id],
        bySource: { ...bySource, [event.source_id]: next },
      };
    }),
  finish: (summary, error, recorded = false) =>
    set((state) => {
      // Stopped by the student (cancel_sync): not a failure. The source it stopped in reports
      // `ok: false` without an error kind; show it as stopped, like the ones never reached.
      const stoppedByUser = error?.kind === "cancelled";
      // The student said "not now": PageLamp doesn't start one by itself right afterwards.
      const noAutomaticBefore = stoppedByUser
        ? holdFrom(Date.now(), "attended", state.noAutomaticBefore)
        : state.noAutomaticBefore;
      // An automatic sync that never had anything to show (refused: another sync, an update
      // installing; not due any more: an empty answer): the last run's result stays as it was.
      if (state.automatic && !state.started) {
        return {
          running: false,
          total: null,
          downloadCourseId: null,
          stopping: false,
          automatic: null,
          unwatched: false,
          automaticProblem: recorded,
          noAutomaticBefore,
        };
      }
      // An automatic sync the student didn't ask for stays quiet when a source failed too.
      // Nothing of the run stays on screen; what the student must fix is on the source itself.
      // Stopping it, or watching it in the capsule, makes it end like any other run (what was
      // left open when it started watches nothing: `unwatched`).
      if (state.automatic && !stoppedByUser && (!state.watched || state.unwatched)) {
        const sources = Object.values(state.bySource);
        const clean =
          !error &&
          summary !== null &&
          summary.ok &&
          summary.results.length > 0 &&
          sources.every((p) => p.result === null || p.result.ok);
        if (!clean) return { ...noRun, automaticProblem: recorded };
      }
      return {
        running: false,
        downloadCourseId: null,
        lastSummary: summary,
        runError: stoppedByUser ? null : error,
        stopping: false,
        stoppedByUser,
        unwatched: false,
        noAutomaticBefore,
        // Sources that never reported back didn't run to the end: mark them stopped so no
        // spinner keeps going after the run is over.
        bySource: Object.fromEntries(
          Object.entries(state.bySource).map(([id, p]) => {
            if (!p.result || (stoppedByUser && !p.result.ok && !p.result.errorKind)) {
              return [id, { ...p, result: null, stopped: true }];
            }
            // The lines live with the row: hidden with it, gone when the next run starts.
            const courses =
              p.result.ok && state.downloadCourseId === null
                ? summary?.results.find((r) => r.source_id === id)?.course_summaries
                : undefined;
            return [id, courses && courses.length > 0 ? { ...p, courses } : p];
          }),
        ),
      };
    }),
  requestStop: () => set({ stopping: true }),
  dismissRunError: () => set({ runError: null }),
  hideRun: () =>
    set((state) =>
      !state.running
        ? noRun
        : state.started
          ? {}
          : { order: [], bySource: {}, lastSummary: null, runError: null, stoppedByUser: false },
    ),
  noteStudentAction: (at) =>
    set((state) => {
      const now = Date.now();
      // How long ago it happened, for what is noted afterwards. A time after now can't be
      // dated (the clock was set back since it was taken): taking it for "just now" would
      // turn a launch from long ago into the student being here. Nothing is noted.
      const ago = at === undefined ? 0 : now - at;
      if (ago < 0) return {};
      const noted = {
        attendedUntil: now - ago + ATTENDED_WINDOW_MS,
        attendedUntilSteady: steadyNow() - ago + ATTENDED_WINDOW_MS,
      };
      // An action after it stays, while it counts. Which of the two came after is told on
      // the steady clock: on the wall clock the older mark is the larger number once the
      // clock was set back.
      const later = studentKnownHere() && state.attendedUntilSteady >= noted.attendedUntilSteady;
      return later ? {} : noted;
    }),
  forgetCourseLines: () =>
    set((state) => ({
      bySource: Object.fromEntries(
        Object.entries(state.bySource).map(([id, { courses: _, ...row }]) => [id, row]),
      ),
    })),
  reset: () => set(idle),
}));

/** "2 of 3 sources done" for the running sync; total is null when unknown. */
export function useSyncCounts(): { done: number; total: number | null } {
  const total = useSyncStore((s) => s.total);
  const done = useSyncStore((s) => s.order.filter((id) => s.bySource[id]?.result).length);
  return { done, total };
}

/**
 * "Download & index files" for one Canvas course. It is a sync of that course's source, so it
 * shares the sync progress store (and the one-sync-at-a-time rule). Resolves false when another
 * run is already active.
 */
export function useDownloadCourseFiles() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useCallback(
    async (courseId: string): Promise<boolean> => {
      const store = useSyncStore.getState();
      if (store.running) return false;
      store.begin(1, courseId);
      const onEvent = (event: SyncEvent) => useSyncStore.getState().apply(event);
      try {
        const result = await api.downloadCourseFiles(courseId, onEvent);
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(
          {
            started_at: result.started_at,
            finished_at: result.finished_at,
            ok: result.ok,
            results: [result],
          },
          null,
        );
      } catch (error) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(null, toApiError(error));
      }
      return true;
    },
    [api, queryClient],
  );
}

/**
 * Start a sync of every source (or one source). Resolves `true` when this call ran a sync (it
 * never rejects — failures land in the store: `runError`, per-source `result`), or `false`
 * when it did nothing because another run in this window was already active.
 *
 * `automatic` marks a sync of every source that PageLamp starts by itself (useAutoSync is the
 * only caller): the facade is told the trigger, and a run that goes wrong stays quiet.
 */
export function useStartSync() {
  const api = useApi();
  const queryClient = useQueryClient();

  return useCallback(
    async (
      sourceId?: string,
      options?: { automatic?: AutoSyncTrigger; unwatched?: boolean },
    ): Promise<boolean> => {
      const store = useSyncStore.getState();
      if (store.running) return false;
      const automatic = sourceId ? null : (options?.automatic ?? null);
      const before = queryClient.getQueryData<AppStatus>(queryKeys.status());
      // An automatic run syncs only the sources that are due, which only the facade knows: the
      // capsule then names the source without a count.
      const total = sourceId ? 1 : automatic ? null : (before?.sources.length ?? null);
      store.begin(total, null, automatic, automatic !== null && options?.unwatched === true);
      // What an automatic run leaves on a source is the facade's decision, read from the status
      // afterwards (refreshed below), not guessed from the kinds of failure.
      const recorded = () => {
        if (!automatic) return false;
        const was = new Map(before?.sources.map((s) => [s.id, s.last_error_kind ?? null]));
        const after = queryClient.getQueryData<AppStatus>(queryKeys.status());
        return (
          after?.sources.some(
            (s) => !!s.last_error_kind && s.last_error_kind !== (was.get(s.id) ?? null),
          ) ?? false
        );
      };
      const onEvent = (event: SyncEvent) => useSyncStore.getState().apply(event);
      try {
        let summary: SyncSummary;
        if (sourceId) {
          const result = await api.syncSource(sourceId, {}, onEvent);
          summary = {
            started_at: result.started_at,
            finished_at: result.finished_at,
            ok: result.ok,
            results: [result],
          };
        } else {
          summary = await api.syncAll(automatic ? { automatic } : {}, onEvent);
        }
        // Refresh data BEFORE marking the run finished, so no screen briefly mistakes a
        // status fetched during our own run (sync_in_progress: true) for another process.
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(summary, null, recorded());
      } catch (error) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.all });
        useSyncStore.getState().finish(null, toApiError(error), recorded());
      }
      return true;
    },
    [api, queryClient],
  );
}

/**
 * Runs `action` once, when this window's current run has ended (at once if none is running).
 * For work that had to wait for a run the student didn't start. Not from inside the store's
 * notification: whoever shows the run must see it end before the next one begins.
 */
export function afterCurrentRun(action: () => void) {
  if (!useSyncStore.getState().running) {
    action();
    return;
  }
  const unsubscribe = useSyncStore.subscribe((state) => {
    if (state.running) return;
    unsubscribe();
    setTimeout(action, 0);
  });
}

/**
 * Stop this window's running sync or download (cancel_sync). The run ends with the sources it
 * didn't finish marked stopped. A sync in another process (the CLI) can't be stopped from here.
 */
export function useStopSync() {
  const api = useApi();
  return useCallback(async () => {
    const store = useSyncStore.getState();
    if (!store.running || store.stopping) return;
    store.requestStop();
    try {
      await api.cancelSync();
    } catch {
      useSyncStore.setState({ stopping: false });
    }
  }, [api]);
}

/**
 * Is a sync running? `running`: started from this window. `external`: another process (e.g.
 * the CLI) holds the sync lock, as the status query reports. `busy`: either — a new sync would
 * fail with "busy" right now.
 */
export function useSyncActivity() {
  const status = useStatus();
  const running = useSyncStore((s) => s.running);
  const external = !running && status.data?.sync_in_progress === true;
  return { running, external, busy: running || external, sources: status.data?.sources ?? [] };
}

/**
 * When another process's sync ends, the courses, sources and deadlines it wrote are new: refresh
 * everything, not just the status. Mount once, in the app shell.
 */
export function useRefreshAfterExternalSync() {
  const queryClient = useQueryClient();
  const { external } = useSyncActivity();
  const wasExternal = useRef(false);
  useEffect(() => {
    if (wasExternal.current && !external && !useSyncStore.getState().running) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.all });
    }
    wasExternal.current = external;
  }, [external, queryClient]);
}
