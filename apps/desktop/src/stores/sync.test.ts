import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import type { SourceErrorKind, SyncSummary } from "@/api/types";
import { afterCurrentRun, studentKnownHere, useSyncStore } from "./sync";

/** A run of one source, "a", that ended with `kind` (null: it synced). */
function summaryOfA(kind: SourceErrorKind | null): SyncSummary {
  return {
    started_at: "2026-10-05T13:00:00Z",
    finished_at: "2026-10-05T13:00:01Z",
    ok: kind === null,
    results: [
      {
        source_id: "a",
        label: "Demo Canvas",
        kind: "canvas",
        ok: kind === null,
        error: kind === null ? null : "Synthetic failure",
        error_kind: kind,
        started_at: "2026-10-05T13:00:00Z",
        finished_at: "2026-10-05T13:00:01Z",
        courses: 1,
        modules: 0,
        materials: 0,
        files_downloaded: 0,
        files_indexed: 0,
        events: 0,
        warnings: [],
        course_summaries: [],
      },
    ],
  };
}

/** An automatic run of source "a" up to its end. */
function automaticRun(kind: SourceErrorKind | null) {
  const store = useSyncStore.getState();
  store.begin(1, null, "unattended");
  store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
  store.apply({
    type: "source_finished",
    source_id: "a",
    ok: kind === null,
    error: kind === null ? null : "Synthetic failure",
    error_kind: kind,
  });
}

const NO_RUN = { running: false, runError: null, lastSummary: null, order: [], automatic: null };

describe("sync store", () => {
  it("folds SyncEvents into per-source progress", () => {
    const store = useSyncStore.getState();
    store.begin(2);
    store.apply({ type: "source_started", source_id: "a", label: "Course folder" });
    store.apply({ type: "progress", source_id: "a", message: "Indexing", current: 2, total: 5 });
    store.apply({ type: "warning", source_id: "a", message: "Skipped a video" });
    store.apply({ type: "source_started", source_id: "b", label: "Demo Canvas" });
    store.apply({
      type: "source_finished",
      source_id: "b",
      ok: false,
      error: "401",
      error_kind: "auth_expired_or_revoked",
    });

    const s = useSyncStore.getState();
    expect(s.running).toBe(true);
    expect(s.total).toBe(2);
    expect(s.order).toEqual(["a", "b"]);
    expect(s.bySource.a).toMatchObject({
      label: "Course folder",
      current: 2,
      total: 5,
      warnings: ["Skipped a video"],
      result: null,
    });
    expect(s.bySource.b?.result).toEqual({
      ok: false,
      error: "401",
      errorKind: "auth_expired_or_revoked",
    });
  });

  it("marks sources that never finished as stopped when the run ends", () => {
    const store = useSyncStore.getState();
    store.begin(2);
    store.apply({ type: "source_started", source_id: "a", label: "Course folder" });
    store.apply({ type: "source_finished", source_id: "a", ok: true });
    store.apply({ type: "source_started", source_id: "b", label: "Demo Canvas" });
    store.finish(null, new ApiError("internal", "boom"));

    const s = useSyncStore.getState();
    expect(s.running).toBe(false);
    expect(s.bySource.a?.stopped).toBe(false);
    expect(s.bySource.b?.stopped).toBe(true);
  });

  it("keeps a dismissed run error dismissed until the next run", () => {
    const store = useSyncStore.getState();
    store.begin(1);
    store.finish(null, new ApiError("network", "offline"));
    store.dismissRunError();
    expect(useSyncStore.getState().runError).toBeNull();
    store.begin(1);
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState().runError?.kind).toBe("busy");
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("whether the student is known to be here", () => {
  const T0 = new Date(2026, 9, 5, 9, 0).getTime();
  const HOUR = 3_600_000;
  // The clock that only runs forward, set by hand like the wall clock.
  let steady = 0;

  function clocks() {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(T0);
    steady = 5_000;
    vi.spyOn(performance, "now").mockImplementation(() => steady);
  }
  /** Time passes, on both clocks. */
  function pass(ms: number) {
    steady += ms;
    vi.setSystemTime(Date.now() + ms);
  }
  /** The computer sleeps: the wall clock goes on, the steady one may stand still. */
  const sleep = (ms: number) => vi.setSystemTime(Date.now() + ms);
  /** The clock is set: only the wall clock moves. */
  const setClock = (to: number) => vi.setSystemTime(to);

  it("is so for half a minute after what they did", () => {
    clocks();
    expect(studentKnownHere()).toBe(false);
    useSyncStore.getState().noteStudentAction();
    expect(studentKnownHere()).toBe(true);
    pass(29_000);
    expect(studentKnownHere()).toBe(true);
    pass(2_000);
    expect(studentKnownHere()).toBe(false);
  });

  it("ends with a sleep, though the steady clock stood still through it", () => {
    clocks();
    useSyncStore.getState().noteStudentAction();
    sleep(10 * 60_000);
    expect(studentKnownHere()).toBe(false);
  });

  it("doesn't come true again when a clock that was set back passes the old mark", () => {
    clocks();
    useSyncStore.getState().noteStudentAction();
    pass(10 * 60_000);
    expect(studentKnownHere()).toBe(false);
    // Back an hour: the mark lies far ahead, which is no "just now" either.
    setClock(Date.now() - HOUR);
    expect(studentKnownHere()).toBe(false);
    // Fifty minutes on, the wall clock shows ten seconds after what the student did, again.
    pass(50 * 60_000 + 10_000);
    expect(Date.now()).toBe(T0 + 10_000);
    expect(studentKnownHere()).toBe(false);
  });

  it("takes a mark far ahead on either clock for no 'just now'", () => {
    clocks();
    useSyncStore.setState({ attendedUntil: T0 + 600_000, attendedUntilSteady: steady + 20_000 });
    expect(studentKnownHere()).toBe(false);
    useSyncStore.setState({ attendedUntil: T0 + 20_000, attendedUntilSteady: steady + 600_000 });
    expect(studentKnownHere()).toBe(false);
    useSyncStore.setState({ attendedUntil: T0 + 20_000, attendedUntilSteady: steady + 20_000 });
    expect(studentKnownHere()).toBe(true);
  });

  it("runs the half minute from a later action that is noted with its time", () => {
    clocks();
    useSyncStore.getState().noteStudentAction();
    pass(10_000);
    // The older mark still counts; the press 200 ms ago came after it.
    useSyncStore.getState().noteStudentAction(Date.now() - 200);
    expect(useSyncStore.getState()).toMatchObject({
      attendedUntil: T0 + 10_000 - 200 + 30_000,
      attendedUntilSteady: steady - 200 + 30_000,
    });
    pass(25_000);
    expect(studentKnownHere()).toBe(true);
  });

  it("replaces a mark that came round on the wall clock and ran out on the steady one", () => {
    clocks();
    useSyncStore.getState().noteStudentAction();
    pass(10 * 60_000);
    // Set back to a tenth of a second after the old action: on the wall clock the old mark
    // has 29.9 s left, and is the larger number next to a press from 200 ms ago.
    setClock(T0 + 100);
    expect(studentKnownHere()).toBe(false);
    useSyncStore.getState().noteStudentAction(Date.now() - 200);
    expect(studentKnownHere()).toBe(true);
    expect(useSyncStore.getState()).toMatchObject({
      attendedUntil: T0 + 100 - 200 + 30_000,
      attendedUntilSteady: steady - 200 + 30_000,
    });
  });

  it("tells before from after on the steady clock when the clock went back within the half minute", () => {
    clocks();
    steady = 100_000;
    // The setting is changed at 09:00:00.000. A second later the clock goes back 20 s.
    useSyncStore.getState().noteStudentAction();
    pass(1_000);
    setClock(Date.now() - 20_000);
    // The press that brings the window to the front, at 119 500 on the steady clock, is
    // noted at the focus 700 ms later. The old mark still counts (9.8 s on the steady clock)
    // and is the larger number on the wall clock; the press came after it all the same.
    pass(18_500);
    expect(steady).toBe(119_500);
    const pressed = Date.now();
    pass(700);
    useSyncStore.getState().noteStudentAction(pressed);
    expect(useSyncStore.getState()).toMatchObject({
      attendedUntil: pressed + 30_000,
      attendedUntilSteady: 149_500,
    });
    // 29.3 s from here, not the 9.8 s the old mark had left.
    pass(20_000);
    expect(studentKnownHere()).toBe(true);
    pass(10_000);
    expect(studentKnownHere()).toBe(false);
  });

  it("notes nothing for a time after now, and leaves a mark that counts alone", () => {
    clocks();
    // A launch from "an hour ahead": the clock was set back since the page loaded.
    useSyncStore.getState().noteStudentAction(Date.now() + HOUR);
    expect(useSyncStore.getState()).toMatchObject({ attendedUntil: 0, attendedUntilSteady: 0 });
    expect(studentKnownHere()).toBe(false);
    // Also by a second, where the mark it would make looks like "just now".
    useSyncStore.getState().noteStudentAction(Date.now() + 1_000);
    expect(studentKnownHere()).toBe(false);

    useSyncStore.getState().noteStudentAction();
    const mark = { ...useSyncStore.getState() };
    useSyncStore.getState().noteStudentAction(Date.now() + 1_000);
    expect(useSyncStore.getState()).toMatchObject({
      attendedUntil: mark.attendedUntil,
      attendedUntilSteady: mark.attendedUntilSteady,
    });
    expect(studentKnownHere()).toBe(true);
  });

  it("holds at the reading it was noted at, though the steady clock gives fractions", () => {
    clocks();
    // (2770.8 + 30000) - 2770.8 is a hair over 30000.
    steady = 2770.8;
    useSyncStore.getState().noteStudentAction();
    expect(studentKnownHere()).toBe(true);
    useSyncStore.getState().noteStudentAction(Date.now());
    expect(studentKnownHere()).toBe(true);
  });

  it("replaces a mark the clock was set back under by what is noted afterwards", () => {
    clocks();
    useSyncStore.getState().noteStudentAction();
    pass(10 * 60_000);
    setClock(Date.now() - HOUR);
    // The press that brought the window to the front, noted a moment later for when it was.
    // The old mark is the larger number, and counts for nothing.
    pass(200);
    useSyncStore.getState().noteStudentAction(Date.now() - 200);
    expect(studentKnownHere()).toBe(true);
    expect(useSyncStore.getState().attendedUntil).toBe(Date.now() - 200 + 30_000);
  });

  it("counts what is noted afterwards from when it was, on both clocks", () => {
    clocks();
    // Twenty seconds ago leaves ten...
    useSyncStore.getState().noteStudentAction(Date.now() - 20_000);
    expect(studentKnownHere()).toBe(true);
    expect(useSyncStore.getState()).toMatchObject({
      attendedUntil: T0 + 10_000,
      attendedUntilSteady: steady + 10_000,
    });
    pass(9_000);
    expect(studentKnownHere()).toBe(true);
    pass(2_000);
    expect(studentKnownHere()).toBe(false);
    // ...and long ago leaves nothing (a launch whose shell only appears an hour later).
    useSyncStore.getState().noteStudentAction(Date.now() - HOUR);
    expect(studentKnownHere()).toBe(false);
  });
});

describe("a sync PageLamp started by itself", () => {
  it("counts the student as here for 30 seconds, and holds the next start for 30 minutes", () => {
    const now = new Date(2026, 9, 5, 9, 0).getTime();
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(now);
    const store = useSyncStore.getState();

    store.noteStudentAction();
    expect(useSyncStore.getState().attendedUntil).toBe(now + 30_000);
    // The launch is noted afterwards, for the moment it happened. It never takes the place of
    // an action after it...
    store.noteStudentAction(now - 3_600_000);
    expect(useSyncStore.getState().attendedUntil).toBe(now + 30_000);
    // ...and by itself it counts from then: ten seconds ago leaves twenty, long ago nothing.
    useSyncStore.setState({ attendedUntil: 0 });
    store.noteStudentAction(now - 10_000);
    expect(useSyncStore.getState().attendedUntil).toBe(now + 20_000);
    useSyncStore.setState({ attendedUntil: 0 });
    store.noteStudentAction(now - 3_600_000);
    expect(useSyncStore.getState().attendedUntil).toBe(now - 3_570_000);
    store.noteStudentAction();
    expect(useSyncStore.getState().attendedUntil).toBe(now + 30_000);

    // The timer's run holds only the next one of its kind: the student coming back ten
    // minutes later still gets a full sync.
    store.begin(1, null, "unattended");
    expect(useSyncStore.getState().noAutomaticBefore).toEqual({
      unattended: now + 1_800_000,
      attended: 0,
    });
    store.finish(null, new ApiError("busy", "locked"));

    // A full sync after the student came back holds both kinds.
    vi.setSystemTime(now + 600_000);
    store.begin(1, null, "attended");
    expect(useSyncStore.getState().noAutomaticBefore).toEqual({
      unattended: now + 600_000 + 1_800_000,
      attended: now + 600_000 + 1_800_000,
    });
    store.finish(null, new ApiError("busy", "locked"));

    // So does the student's Stop, of any sync.
    vi.setSystemTime(now + 3_600_000);
    store.begin(1);
    store.finish(null, new ApiError("cancelled", "Cancelled"));
    expect(useSyncStore.getState().noAutomaticBefore).toEqual({
      unattended: now + 3_600_000 + 1_800_000,
      attended: now + 3_600_000 + 1_800_000,
    });
  });

  it("leaves the last run's result alone until it has something to show", () => {
    const store = useSyncStore.getState();
    // The student's sync failed: a row, and the reason for the whole run.
    store.begin(2);
    store.apply({ type: "source_started", source_id: "b", label: "Course folder" });
    store.finish(null, new ApiError("network", "offline"));
    const before = useSyncStore.getState();
    expect(before.runError?.kind).toBe("network");

    // An automatic run begins and is refused: nothing of the old result is touched.
    store.begin(3, null, "unattended");
    expect(useSyncStore.getState()).toMatchObject({ running: true, started: false });
    expect(useSyncStore.getState().runError).toBe(before.runError);
    expect(useSyncStore.getState().bySource).toBe(before.bySource);
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState()).toMatchObject({ running: false, automatic: null });
    expect(useSyncStore.getState().runError).toBe(before.runError);
    expect(useSyncStore.getState().order).toEqual(["b"]);

    // The next one gets going: from its first event it is the run on screen.
    store.begin(3, null, "unattended");
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    expect(useSyncStore.getState()).toMatchObject({ started: true, runError: null, order: ["a"] });
  });

  it("ends like any other when every source synced", () => {
    automaticRun(null);
    useSyncStore.getState().finish(summaryOfA(null), null);
    const s = useSyncStore.getState();
    expect(s.lastSummary?.ok).toBe(true);
    expect(s.automatic).toBe("unattended");
    expect(s.order).toEqual(["a"]);
    expect(s.noAutomaticBefore.unattended).toBeGreaterThan(Date.now());
  });

  it("leaves nothing behind when it is refused, or wasn't due any more", () => {
    const store = useSyncStore.getState();
    store.begin(3, null, "attended");
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState()).toMatchObject({ ...NO_RUN, automaticProblem: false });
    // The start still counts for "not again so soon".
    expect(useSyncStore.getState().noAutomaticBefore.attended).toBeGreaterThan(Date.now());

    store.begin(3, null, "unattended");
    store.finish(
      {
        started_at: "2026-10-05T13:00:00Z",
        finished_at: "2026-10-05T13:00:00Z",
        ok: true,
        results: [],
      },
      null,
    );
    expect(useSyncStore.getState()).toMatchObject({ ...NO_RUN, automaticProblem: false });
  });

  it("leaves nothing behind when a source failed, and notes what the facade recorded", () => {
    // Which failures are worth telling the student is the facade's call (it records them on
    // the source); the store doesn't judge by the kind of failure.
    automaticRun("auth_expired_or_revoked");
    useSyncStore.getState().finish(summaryOfA("auth_expired_or_revoked"), null);
    expect(useSyncStore.getState()).toMatchObject({ ...NO_RUN, automaticProblem: false });

    automaticRun("other");
    useSyncStore.getState().finish(summaryOfA("other"), null, true);
    expect(useSyncStore.getState()).toMatchObject({ ...NO_RUN, automaticProblem: true });

    // The next run starts clean.
    useSyncStore.getState().begin(1);
    expect(useSyncStore.getState().automaticProblem).toBe(false);
  });

  it("ends like any other when the student watches it, or stops it", () => {
    useSyncStore.setState({ watched: true });
    automaticRun("network");
    useSyncStore.getState().finish(summaryOfA("network"), null);
    expect(useSyncStore.getState().lastSummary?.ok).toBe(false);
    expect(useSyncStore.getState().order).toEqual(["a"]);

    useSyncStore.setState({ watched: false });
    const store = useSyncStore.getState();
    store.begin(1, null, "unattended");
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    store.finish(null, new ApiError("cancelled", "Cancelled"));
    expect(useSyncStore.getState().stoppedByUser).toBe(true);
    expect(useSyncStore.getState().order).toEqual(["a"]);
  });

  it("keeps a manual run's failure, as before", () => {
    const store = useSyncStore.getState();
    store.begin(1);
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState().runError?.kind).toBe("busy");
    expect(useSyncStore.getState().noAutomaticBefore).toEqual({ unattended: 0, attended: 0 });
  });

  it("is quiet when refused even while the student is on an older run's capsule", () => {
    // Watching what is on screen isn't watching a run that never showed anything.
    useSyncStore.setState({ watched: true });
    const store = useSyncStore.getState();
    store.begin(3, null, "unattended");
    store.finish(null, new ApiError("busy", "locked"));
    expect(useSyncStore.getState()).toMatchObject(NO_RUN);
  });

  it("holds automatic starts for a while after the student stopped a sync", () => {
    const store = useSyncStore.getState();
    store.begin(1);
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    store.finish(null, new ApiError("cancelled", "Cancelled"));
    const held = useSyncStore.getState().noAutomaticBefore;
    expect(held.unattended).toBeGreaterThan(Date.now());
    expect(held.attended).toBeGreaterThan(Date.now());
  });

  it("'Hide' while an automatic run hasn't shown anything clears the last result only", () => {
    const store = useSyncStore.getState();
    store.begin(1);
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    store.finish(null, new ApiError("cancelled", "Cancelled"));
    expect(useSyncStore.getState().bySource.a).toMatchObject({ result: null, stopped: true });

    store.begin(null, null, "unattended");
    useSyncStore.getState().hideRun();
    // The call in flight goes on; only what the last run left is gone.
    expect(useSyncStore.getState()).toMatchObject({
      running: true,
      automatic: "unattended",
      started: false,
      order: [],
      bySource: {},
      lastSummary: null,
      stoppedByUser: false,
    });

    // Once it shows its own rows, "Hide" has nothing of an older run to take away.
    store.apply({ type: "source_started", source_id: "b", label: "Course folder" });
    useSyncStore.getState().hideRun();
    expect(useSyncStore.getState()).toMatchObject({ running: true, order: ["b"] });
  });

  it("'Hide' clears the run but not what an automatic sync needs to remember", () => {
    automaticRun(null);
    useSyncStore.getState().finish(summaryOfA(null), null);
    useSyncStore.getState().noteStudentAction();
    useSyncStore.getState().hideRun();
    const s = useSyncStore.getState();
    expect(s).toMatchObject(NO_RUN);
    expect(s.noAutomaticBefore.unattended).toBeGreaterThan(Date.now());
    expect(s.attendedUntil).toBeGreaterThan(Date.now());
  });
});

describe("a run's lines for each course", () => {
  const LINE = { course: "DEMO312", modules: 1, pages: 4, files: 3, events: 1, warnings: 0 };
  const withLines = (kind: SourceErrorKind | null): SyncSummary => {
    const summary = summaryOfA(kind);
    const [a] = summary.results;
    if (!a) throw new Error("no result");
    return {
      ...summary,
      results: [{ ...a, course_summaries: [{ ...LINE, pages_hidden: true, linked_pages: 2 }] }],
    };
  };
  /** The student's own run of source "a" up to its end. */
  function run(kind: SourceErrorKind | null, downloadCourseId: string | null = null) {
    const store = useSyncStore.getState();
    store.begin(1, downloadCourseId);
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    store.apply({
      type: "source_finished",
      source_id: "a",
      ok: kind === null,
      error: kind === null ? null : "Synthetic failure",
      error_kind: kind,
    });
  }

  it("puts them on the source's row when the run has ended, and takes them away with it", () => {
    run(null);
    expect(useSyncStore.getState().bySource.a?.courses).toBeUndefined();
    useSyncStore.getState().finish(withLines(null), null);
    expect(useSyncStore.getState().bySource.a?.courses).toEqual([
      { ...LINE, pages_hidden: true, linked_pages: 2 },
    ]);

    // The next run's rows start without them, while it runs and after a Stop.
    run(null);
    expect(useSyncStore.getState().bySource.a?.courses).toBeUndefined();
    useSyncStore.getState().finish(null, new ApiError("cancelled", "The sync was stopped."));
    expect(useSyncStore.getState().bySource.a?.courses).toBeUndefined();
  });

  it("has them after a sync PageLamp started by itself, when it ended well", () => {
    automaticRun(null);
    useSyncStore.getState().finish(withLines(null), null);
    expect(useSyncStore.getState().bySource.a?.courses).toHaveLength(1);
  });

  it("has none for a source that failed, or for a run without any", () => {
    run("network");
    useSyncStore.getState().finish(withLines("network"), null);
    expect(useSyncStore.getState().bySource.a?.courses).toBeUndefined();

    run(null);
    useSyncStore.getState().finish(summaryOfA(null), null);
    expect(useSyncStore.getState().bySource.a?.courses).toBeUndefined();
  });

  it("forgets them when a source or a course is removed, and keeps the rows", () => {
    run(null);
    useSyncStore.getState().finish(withLines(null), null);
    useSyncStore.getState().forgetCourseLines();
    const state = useSyncStore.getState();
    expect(state.order).toEqual(["a"]);
    expect(state.bySource.a?.result?.ok).toBe(true);
    expect(state.bySource.a).not.toHaveProperty("courses");
  });

  it("has none after a download: its own message says what it did", () => {
    run(null, "canvas:canvas.demo.test/course/312");
    useSyncStore.getState().finish(withLines(null), null);
    const state = useSyncStore.getState();
    expect(state.downloadCourseId).toBeNull();
    expect(state.bySource.a?.result?.ok).toBe(true);
    expect(state.bySource.a?.courses).toBeUndefined();
  });
});

describe("afterCurrentRun", () => {
  it("runs at once when nothing is running, else once after the run has ended", async () => {
    const now = vi.fn();
    afterCurrentRun(now);
    expect(now).toHaveBeenCalledTimes(1);

    const store = useSyncStore.getState();
    store.begin(1);
    // Whoever shows the run sees it end first: the action isn't run inside that notification.
    const running: boolean[] = [];
    const seenByOthers: boolean[] = [];
    afterCurrentRun(() => running.push(useSyncStore.getState().running));
    const unsubscribe = useSyncStore.subscribe((s) => seenByOthers.push(s.running));
    store.apply({ type: "source_started", source_id: "a", label: "Demo Canvas" });
    store.finish(null, null);
    expect(running).toEqual([]);
    expect(seenByOthers.at(-1)).toBe(false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(running).toEqual([false]);

    store.begin(1);
    store.finish(null, null);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(running).toEqual([false]);
    unsubscribe();
  });
});
