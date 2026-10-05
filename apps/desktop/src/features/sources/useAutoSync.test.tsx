import { onlineManager } from "@tanstack/react-query";
import { act, configure, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PageLampApi } from "@/api/client";
import { ApiError } from "@/api/errors";
import { createMockApi, MOCK_STARTUP_CHECK_EVENT, type MockOptions } from "@/api/mock";
import { queryKeys } from "@/api/queries";
import { createTauriApi } from "@/api/tauri";
import type { SourceErrorKind, SourceSyncResult, SyncEvent, SyncSummary } from "@/api/types";
import { createQueryClient } from "@/app/Providers";
import { studentKnownHere, useSyncStore } from "@/stores/sync";
import { useUpdateStore } from "@/stores/updates";
import { renderRoute } from "@/test/render";
import { DIALOG_LEFT_MS, INPUT_ASK_DELAY_MS, LAUNCH_REPLY_MS, TICK_REREAD_MS } from "./useAutoSync";

// Headroom for slow CI machines: a whole route renders before anything here can happen.
// (Per-file setting: Vitest isolates each test file.)
configure({ asyncUtilTimeout: 3000 });

const HOUR = 60 * 60 * 1000;
const MINUTE = 60 * 1000;
const CANVAS = "canvas:canvas.demo.test";
const DUE = { unattended: true, attended: true };

function mockApi(options: MockOptions = {}) {
  return createMockApi({ latencyMs: 0, syncStepMs: 0, ...options });
}

/** Long enough for an answer to arrive and a sync to start, if one were going to. */
const settle = (ms = 50) => new Promise((resolve) => setTimeout(resolve, ms));

/** The facade keeps saying a sync is due, whatever happened. */
function alwaysDue(api: PageLampApi) {
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({ ...(await tasks()), sync_due: DUE });
}

/**
 * The student's sync of one source, held until `release()`. While it is held the lock is taken,
 * so `startup_tasks` answers "not due" whatever is stale, as the facade does. (Held by hand, not
 * by a slow mock: how long a run takes must not decide a test.)
 */
function holdSourceSync(api: PageLampApi) {
  let held = false;
  let open: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    open = resolve;
  });
  const sync = api.syncSource.bind(api);
  api.syncSource = async (id, req, onEvent) => {
    held = true;
    const result = await sync(id, req, onEvent);
    await gate;
    return result;
  };
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => {
    const answer = await tasks();
    return held ? { ...answer, sync_due: { unattended: false, attended: false } } : answer;
  };
  return {
    release: () => {
      held = false;
      open();
    },
  };
}

/** Both kinds of automatic start held until `time`, as after an attended start or a Stop. */
const heldUntil = (time: number) => ({ unattended: time, attended: time });

/** Only the date is faked: timers, and so `waitFor`, keep working. */
function startClock(): Date {
  const start = new Date(2026, 9, 5, 9, 0);
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(start);
  // The clock that only runs forward is the tests' too, moved along with the date by `later`.
  steady = STEADY_START;
  vi.spyOn(performance, "now").mockImplementation(() => steady);
  return start;
}

/** Where the steady clock stands when `startClock` is called (the page loaded a minute before). */
const STEADY_START = 60_000;
let steady = STEADY_START;

/** Time passes: it is `ms` after `start`, on both clocks. */
function later(start: Date, ms: number) {
  if (ms < 0) throw new Error("time doesn't go back: use clockSetTo for a clock that was set");
  vi.setSystemTime(new Date(start.getTime() + ms));
  steady = STEADY_START + ms;
}

/** Time passes from where both clocks are now (also after `clockSetTo`). */
function pass(ms: number) {
  vi.setSystemTime(new Date(Date.now() + ms));
  steady += ms;
}

/** The clock is set: the wall clock shows `ms` after `start`. No time passes. */
function clockSetTo(start: Date, ms: number) {
  vi.setSystemTime(new Date(start.getTime() + ms));
}

/** The computer sleeps: the wall clock goes on, the steady one stands still. */
function sleep(ms: number) {
  vi.setSystemTime(new Date(Date.now() + ms));
}

/** The window comes to the front. Nobody need be at the computer for that. */
function focusWindow() {
  act(() => {
    window.dispatchEvent(new Event("focus"));
  });
}

/** Something is dispatched in the window, as the browser does for the student's input. */
function input(event: Event) {
  act(() => {
    window.dispatchEvent(event);
  });
}

/** The student comes back: the window gains focus, and they press something in it. */
function comeBack() {
  focusWindow();
  input(new Event("pointerdown"));
}

/** Long enough for the question an input asks (a moment later) to be answered and acted on. */
const afterInput = () => settle(INPUT_ASK_DELAY_MS + 150);

/**
 * An automatic run held until `release()`. While it is held the lock is taken, so
 * `startup_tasks` answers "not due" whatever is stale, as the facade does.
 */
function holdSyncAll(api: PageLampApi) {
  let held = false;
  let open: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    open = resolve;
  });
  const sync = api.syncAll.bind(api);
  api.syncAll = async (req, onEvent) => {
    held = true;
    const result = await sync(req, onEvent);
    await gate;
    return result;
  };
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => {
    const answer = await tasks();
    return held ? { ...answer, sync_due: { unattended: false, attended: false } } : answer;
  };
  return {
    release: () => {
      held = false;
      open();
    },
  };
}

function failedCanvas(kind: SourceErrorKind): SourceSyncResult {
  return {
    source_id: CANVAS,
    label: "Demo Canvas",
    kind: "canvas",
    ok: false,
    error: "Synthetic failure",
    error_kind: kind,
    started_at: "2026-10-05T13:00:00Z",
    finished_at: "2026-10-05T13:00:01Z",
    courses: 0,
    modules: 0,
    materials: 0,
    files_downloaded: 0,
    files_indexed: 0,
    events: 0,
    warnings: [],
    course_summaries: [],
  };
}

/** An automatic run in which Canvas fails with `kind`. */
function failingWith(api: PageLampApi, kind: SourceErrorKind) {
  return vi
    .spyOn(api, "syncAll")
    .mockImplementation(async (_req, onEvent: (event: SyncEvent) => void) => {
      onEvent({ type: "source_started", source_id: CANVAS, label: "Demo Canvas" });
      onEvent({
        type: "source_finished",
        source_id: CANVAS,
        ok: false,
        error: "Synthetic failure",
        error_kind: kind,
      });
      const summary: SyncSummary = {
        started_at: "2026-10-05T13:00:00Z",
        finished_at: "2026-10-05T13:00:01Z",
        ok: false,
        results: [failedCanvas(kind)],
      };
      return summary;
    });
}

const announced = () =>
  screen
    .getAllByRole("status")
    .map((s) => s.textContent)
    .join(" ");

function expectNoTrace() {
  expect(screen.queryByRole("button", { name: "Sync failed" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Sync finished with problems" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Dismiss" })).toBeNull();
  expect(screen.queryByText("Sync failed")).toBeNull();
  expect(document.querySelector("[data-sonner-toast]")).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(useSyncStore.getState()).toMatchObject({
    running: false,
    runError: null,
    lastSummary: null,
    order: [],
  });
}

afterEach(async () => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  // A sync still in flight would end inside the next test, in the store they share.
  await waitFor(() => expect(useSyncStore.getState().running).toBe(false), { timeout: 5000 });
});

describe("automatic sync", () => {
  it("starts one attended sync at launch when one is due, and asks again afterwards", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });

    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
    expect(await screen.findByRole("button", { name: "Sync finished" })).toBeInTheDocument();
    // No dialog, no message, and focus stays where it was.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.querySelector("[data-sonner-toast]")).toBeNull();
    expect(document.body).toHaveFocus();

    // Once at launch, once after the run: the cached answer no longer says "due".
    await waitFor(() => expect(tasks).toHaveBeenCalledTimes(2));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);
    expect((await api.startupTasks()).sync_due).toEqual({ unattended: false, attended: false });
  });

  it("looks like any sync while it runs: the capsule names the source it is on", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const real = api.syncAll;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    api.syncAll = async (req, onEvent: (event: SyncEvent) => void) => {
      onEvent({ type: "source_started", source_id: "folder:demo-courses", label: "Course folder" });
      await gate;
      return real(req, onEvent);
    };
    renderRoute("/courses", { api });

    // No count: only the facade knows which sources this run syncs.
    expect(await screen.findByRole("button", { name: "Syncing · Course folder" })).toBeVisible();
    expect(announced()).toContain("Syncing…");
    release();
    expect(await screen.findByRole("button", { name: "Sync finished" })).toBeInTheDocument();
  });

  it("does nothing when no sync is due", async () => {
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await waitFor(() => expect(tasks).toHaveBeenCalledTimes(1));
    await settle();
    expect(sync).not.toHaveBeenCalled();
    expect(tasks).toHaveBeenCalledTimes(1);
  });

  it("does nothing when automatic sync is off", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    await api.setSyncPrefs({ auto_sync: "off" });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();
  });

  it("starts an unattended sync from the hourly re-read, and no second one an hour later", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // The sources were synced at 07:00; at 20:00 the re-read finds them 13 hours old.
    later(start, 11 * HOUR);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    // Nobody is known to be at the app: the launch was 11 hours ago.
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));

    later(start, 12 * HOUR);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // Half a day on, the next one.
    later(start, 24 * HOUR);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
  });

  it("leaves the full sync for the student's return after the timer's light one", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    later(start, 11 * HOUR);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    // Canvas was only read lightly, so a full sync is still due; the timer never runs that one.
    await settle();
    expect((await api.startupTasks()).sync_due).toEqual({ unattended: false, attended: true });
    expect(sync).toHaveBeenCalledTimes(1);

    // The student comes back ten minutes after that run: the light one doesn't hold the full.
    later(start, 11 * HOUR + 10 * MINUTE);
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("asks again when the student comes back to the window, and that sync is attended", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await waitFor(() => expect(tasks).toHaveBeenCalledTimes(1));

    // A few minutes later the answer is still young: nothing is asked.
    later(start, 5 * MINUTE);
    comeBack();
    await afterInput();
    expect(tasks).toHaveBeenCalledTimes(1);

    later(start, 11 * HOUR);
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("never starts while the student's own sync runs, and asks again when it has ended", async () => {
    const start = startClock();
    const api = mockApi();
    const held = holdSourceSync(api);
    const all = vi.spyOn(api, "syncAll");
    const { user, queryClient } = renderRoute("/sources", { api });
    await screen.findByRole("heading", { level: 2, name: "Course calendar" });

    // 13 hours on, the student syncs one source; the others stay as old as they were.
    later(start, 13 * HOUR);
    await user.click(screen.getByRole("button", { name: "Sync Course calendar" }));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(true));
    // Asked while that sync holds the lock, the facade says "not due", whatever is stale.
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(all).not.toHaveBeenCalled();

    // It ends: PageLamp asks again, and now syncs the rest by itself.
    held.release();
    await waitFor(() => expect(all).toHaveBeenCalledTimes(1));
    expect(all.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("doesn't start while another process syncs", async () => {
    const api = mockApi({ scenario: "busy" });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();
    expect(screen.queryByText("Sync failed")).toBeNull();
  });

  /**
   * Another process syncs until `end()`. While it does, the status says so and, as the facade
   * answers under the lock, nothing is due; afterwards a sync is.
   */
  function syncElsewhere(api: PageLampApi) {
    let elsewhere = true;
    const status = api.status.bind(api);
    api.status = async () => ({ ...(await status()), sync_in_progress: elsewhere });
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: elsewhere ? { unattended: false, attended: false } : DUE,
    });
    return {
      end: () => {
        elsewhere = false;
      },
    };
  }

  /** The window goes out of sight (the tray) until the returned function is called. */
  function outOfSight() {
    Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
    return () => Reflect.deleteProperty(document, "visibilityState");
  }

  async function seenSyncingElsewhere(queryClient: ReturnType<typeof createQueryClient>) {
    await screen.findByRole("heading", { level: 1 });
    await vi.waitFor(() =>
      expect(queryClient.getQueryData(queryKeys.status())).toMatchObject({
        sync_in_progress: true,
      }),
    );
    await settle();
  }

  it("sees another process's sync end with the window out of sight, and syncs then", async () => {
    // The status poll's timer is faked; the waits below use real ones.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const api = mockApi();
    const other = syncElsewhere(api);
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await seenSyncingElsewhere(queryClient);
    expect(sync).not.toHaveBeenCalled();

    const back = outOfSight();
    try {
      other.end();
      await act(() => vi.advanceTimersByTimeAsync(3000));
      await vi.waitFor(() => expect(sync).toHaveBeenCalledTimes(1), { timeout: 3000 });
      expect(sync.mock.calls[0]?.[0]).toMatchObject({ automatic: expect.any(String) });
    } finally {
      back();
    }
  });

  it("keeps hearing of it out of sight when the computer is offline for a moment", async () => {
    // With the app's own client: by its defaults a request made offline would wait, and go
    // on only when the window is visible again.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const api = mockApi();
    const other = syncElsewhere(api);
    const sync = vi.spyOn(api, "syncAll");
    const queryClient = createQueryClient();
    renderRoute("/courses", { api, queryClient });
    await seenSyncingElsewhere(queryClient);

    const back = outOfSight();
    onlineManager.setOnline(false);
    try {
      other.end();
      await act(() => vi.advanceTimersByTimeAsync(3000));
      await vi.waitFor(() => expect(sync).toHaveBeenCalledTimes(1), { timeout: 3000 });
    } finally {
      onlineManager.setOnline(true);
      back();
    }
  });

  it("asks again at the next tick when a read of the status fails out of sight", async () => {
    // With the app's own client: a retry would wait for the window to be visible, and the
    // poll's later ticks would wait with it.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const api = mockApi();
    const other = syncElsewhere(api);
    const sync = vi.spyOn(api, "syncAll");
    const queryClient = createQueryClient();
    renderRoute("/courses", { api, queryClient });
    await seenSyncingElsewhere(queryClient);

    const back = outOfSight();
    try {
      other.end();
      const status = api.status.bind(api);
      let failed = false;
      api.status = async () => {
        if (!failed) {
          failed = true;
          throw new ApiError("internal", "Synthetic failure");
        }
        return status();
      };
      await act(() => vi.advanceTimersByTimeAsync(3000));
      await vi.waitFor(() => expect(failed).toBe(true));
      await settle();
      expect(sync).not.toHaveBeenCalled();

      await act(() => vi.advanceTimersByTimeAsync(3000));
      await vi.waitFor(() => expect(sync).toHaveBeenCalledTimes(1), { timeout: 3000 });
    } finally {
      back();
    }
  });

  it("gives the others their turn when the lock is free again, though a dialog is still open", async () => {
    // The answer taken while the other process synced said "not due" because of the lock. Left
    // at that, clearing removed courses and Monday's note would wait behind the dialog.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    const api = mockApi();
    const other = syncElsewhere(api);
    const sync = vi.spyOn(api, "syncAll");
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    document.body.appendChild(dialog);
    try {
      const { queryClient } = renderRoute("/courses", { api });
      await seenSyncingElsewhere(queryClient);
      const answers = () => queryClient.getQueryState(queryKeys.startupTasks())?.dataUpdateCount;
      expect(answers()).toBe(1);
      expect(useSyncStore.getState().clearedAnswer).toBe(0);

      other.end();
      await act(() => vi.advanceTimersByTimeAsync(3000));
      // Asked again: a sync is due and waits for the dialog; the others needn't.
      await vi.waitFor(() => expect(answers()).toBe(2), { timeout: 3000 });
      await vi.waitFor(() => expect(useSyncStore.getState().clearedAnswer).toBe(2));
      await settle();
      expect(answers()).toBe(2);
      expect(sync).not.toHaveBeenCalled();
    } finally {
      dialog.remove();
    }
    await vi.waitFor(() => expect(sync).toHaveBeenCalledTimes(1), { timeout: 3000 });
  });

  it("starts no second sync within half an hour, even after leaving the shell and coming back", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    const { router, queryClient } = renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // Without the half-hour hold, so only "this answer was dealt with" can stop a second one.
    act(() => useSyncStore.setState({ noAutomaticBefore: { unattended: 0, attended: 0 } }));
    await act(() => router.navigate("/welcome"));
    await screen.findByRole("heading", { level: 1, name: "Welcome to PageLamp" });
    await act(() => router.navigate("/courses"));
    await screen.findByRole("heading", { level: 1, name: "Courses" });
    await settle();
    // The answer in the cache was dealt with by the shell that read it.
    expect(sync).toHaveBeenCalledTimes(1);

    // And with the hold back, a new "due" answer finds the last start too recent.
    act(() => useSyncStore.setState({ noAutomaticBefore: heldUntil(Date.now() + 60_000) }));
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);
  });

  it("keeps half an hour after an attended start: nothing at 10 or 29 minutes, again at 31", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();

    // The student comes back ten minutes later: no second full sync so soon.
    later(start, 10 * MINUTE);
    comeBack();
    await afterInput();
    expect(sync).toHaveBeenCalledTimes(1);

    later(start, 29 * MINUTE);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    later(start, 31 * MINUTE);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
  });

  it.each([
    [29, "attended"],
    [31, "unattended"],
  ] as const)(
    "counts an answer %i seconds after the student came back as %s",
    async (seconds, trigger) => {
      const start = startClock();
      const api = mockApi();
      const sync = vi.spyOn(api, "syncAll");
      const { queryClient } = renderRoute("/courses", { api });
      await screen.findByRole("heading", { level: 1 });
      await settle();

      // An hour on, the student comes back. The answer read then is young and nothing is due.
      later(start, HOUR);
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      comeBack();
      await afterInput();
      expect(sync).not.toHaveBeenCalled();

      alwaysDue(api);
      later(start, HOUR + seconds * 1000);
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: trigger });
    },
  );

  it("doesn't take a clock set back for the student still being here", async () => {
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // As after the clock went back ten minutes: both marks lie far in the future.
    act(() =>
      useSyncStore.setState({
        attendedUntil: Date.now() + 10 * MINUTE,
        noAutomaticBefore: heldUntil(Date.now() + 2 * HOUR),
      }),
    );
    alwaysDue(api);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("starts no full sync when a clock that was set back shows the launch's half minute again", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // Ten minutes after the launch the clock is set back an hour. Fifty minutes on it shows
    // ten seconds after the launch again: an hour has passed, and nobody is here.
    later(start, HOUR);
    clockSetTo(start, 10_000);
    alwaysDue(api);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("notes the press that brings the window to the front after the clock was set back", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // The clock goes back an hour: the launch's mark now lies an hour ahead. Then the student
    // clicks the window to the front; the press reaches the page a moment before the focus,
    // and is noted afterwards for when it was. The old mark is the larger number.
    clockSetTo(start, -HOUR);
    expect(studentKnownHere()).toBe(false);
    alwaysDue(api);
    input(new Event("pointerdown"));
    pass(200);
    focusWindow();
    expect(studentKnownHere()).toBe(true);

    // The last answer is dated an hour after now, which is no fresh answer: the question is
    // asked, and the full sync that is due starts.
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("asks at a focus when the last answer is dated after now: the clock was set back", async () => {
    const start = startClock();
    const api = mockApi();
    const tasks = vi.spyOn(api, "startupTasks");
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(tasks).toHaveBeenCalledTimes(1);

    // A minute after the launch a focus asks nothing: the answer is fresh.
    later(start, MINUTE);
    focusWindow();
    await settle();
    expect(tasks).toHaveBeenCalledTimes(1);

    // The clock goes back an hour. The answer now lies an hour ahead, and says nothing about
    // how old it is: the focus asks. Nobody pressed anything, so what is due starts unattended.
    clockSetTo(start, -HOUR);
    alwaysDue(api);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("asks from the hourly timer itself, also with the window out of sight", async () => {
    const start = new Date(2026, 9, 5, 9, 0);
    // The query's own timer is faked; the waits below use real ones.
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(start);
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    await vi.waitFor(() => expect(tasks).toHaveBeenCalledTimes(1), { timeout: 3000 });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
    try {
      // Ten hours on; then the hour that makes the timer fire, 13 hours after the last sync.
      vi.setSystemTime(new Date(start.getTime() + 10 * HOUR));
      await act(() => vi.advanceTimersByTimeAsync(HOUR));
      // Asked again by the timer (and once more after the run it started).
      await vi.waitFor(() => expect(tasks.mock.calls.length).toBeGreaterThanOrEqual(2), {
        timeout: 3000,
      });
      await vi.waitFor(() => expect(sync).toHaveBeenCalledTimes(1), { timeout: 3000 });
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    } finally {
      Reflect.deleteProperty(document, "visibilityState");
    }
  });

  it("doesn't take a start the student never saw for the student opening PageLamp", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", startedHidden: true });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // The student opens the window and does something in it, minutes later or days.
    later(start, 5 * MINUTE);
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes the first input in a window that started hidden, though no focus was heard", async () => {
    // Showing the window from the tray can give focus before this page listens, or give none.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", startedHidden: true });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();

    // Hours later the student opens the window and presses something. No focus event came.
    later(start, 3 * HOUR);
    // What isn't the student isn't taken for it here either.
    input(new Event("pointermove"));
    input(new KeyboardEvent("keydown", { key: "a", repeat: true }));
    input(new KeyboardEvent("keydown", { key: "F15" }));
    await afterInput();
    expect(sync).toHaveBeenCalledTimes(1);

    input(new Event("pointerdown"));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes only the first: later input in that window says nothing new without a focus", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", startedHidden: true });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    later(start, 3 * HOUR);
    input(new Event("pointerdown"));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));

    // The student works on; an hour later a press is no new coming.
    const noted = useSyncStore.getState().attendedUntil;
    later(start, 4 * HOUR);
    input(new Event("pointerdown"));
    await afterInput();
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
  });

  it("doesn't spend that first input on a run that is in its way", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", startedHidden: true });
    alwaysDue(api);
    const held = holdSyncAll(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    // The hidden launch's light sync is still running when the student opens the window and
    // presses something.
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    input(new Event("pointerdown"));
    await afterInput();

    // The run ends two minutes later: that press is too old to say the student is here.
    later(start, 2 * MINUTE);
    held.release();
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // Their next press counts, with no focus in between.
    input(new Event("pointerdown"));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("gives a page that was loaded again no such first input, hidden start or not", async () => {
    // The flag says how the window started; after a reload it may have been in use for hours.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", reloaded: true, startedHidden: true });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    const noted = useSyncStore.getState().attendedUntil;

    later(start, 5 * MINUTE);
    input(new Event("pointerdown"));
    await afterInput();
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
    expect(sync).toHaveBeenCalledTimes(1);
  });

  it("gives a launch the student saw no input without a focus either", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    const noted = useSyncStore.getState().attendedUntil;

    later(start, HOUR);
    input(new Event("pointerdown"));
    await afterInput();
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
  });

  it("never takes a page that was loaded again for the student opening PageLamp", async () => {
    // The system restarts the page by itself after ending its content process: nobody is here.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", reloaded: true });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // The student comes back to the window: now it is attended.
    later(start, 5 * MINUTE);
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes the launch for the moment the page loaded, not for when the shell appears", async () => {
    // A start page that only got through by itself, hours after the launch: nobody is here.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    later(start, 3 * HOUR);
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("dates a late launch as old though the clock, set back, shows it as recent", async () => {
    // Three hours pass before the shell appears, and the clock was set back by about as much:
    // the wall clock says the page loaded ten seconds ago.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    later(start, 3 * HOUR);
    clockSetTo(start, 10_000);
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(studentKnownHere()).toBe(false);
  });

  it("dates a late launch as old though the computer slept through the wait", async () => {
    // The steady clock may have stood still through the sleep: it says the page just loaded.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    later(start, 5_000);
    sleep(8 * HOUR);
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(studentKnownHere()).toBe(false);
  });

  it("takes 'Try again' on a failed start for the student being here, however old the launch", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const status = api.status.bind(api);
    let failing = true;
    api.status = async () => {
      if (failing) throw new ApiError("internal", "Synthetic failure");
      return status();
    };
    const sync = vi.spyOn(api, "syncAll");
    const { user } = renderRoute("/", { api });
    const retry = await screen.findByRole("button", { name: "Try again" });

    // A minute later it would open, and the student says so.
    later(start, MINUTE);
    failing = false;
    await user.click(retry);
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("still takes a shell that appears a few seconds after the page loaded for the launch", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    later(start, 10_000);
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("acts on no answer before it knows whether this load is the launch", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    let reply: (first: boolean) => void = () => {};
    api.firstPageLoad = () =>
      new Promise<boolean>((resolve) => {
        reply = resolve;
      });
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    await waitFor(() => expect(tasks).toHaveBeenCalledTimes(1));
    await settle();
    // The answer says "due", and nothing starts: it could still be a reload.
    expect(sync).not.toHaveBeenCalled();

    reply(true);
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes the load for a reload when it can't find out", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    api.firstPageLoad = () => Promise.reject(new ApiError("internal", "Synthetic failure"));
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("takes the load for a reload when no reply comes in time", async () => {
    // The hook's own wait is faked. Nothing here may wait with a timer of its own (vi.waitFor
    // would move the faked clock): promises are flushed by hand.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const flush = async () => {
      for (let i = 0; i < 20; i++) await act(async () => Promise.resolve());
    };
    const api = mockApi({ scenario: "auto-sync-due" });
    let reply: (first: boolean) => void = () => {};
    api.firstPageLoad = () =>
      new Promise<boolean>((resolve) => {
        reply = resolve;
      });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    const { queryClient } = renderRoute("/courses", { api });
    await flush();
    expect(tasks).toHaveBeenCalledTimes(1);

    await act(() => vi.advanceTimersByTimeAsync(LAUNCH_REPLY_MS - 1));
    await flush();
    expect(sync).not.toHaveBeenCalled();

    await act(() => vi.advanceTimersByTimeAsync(1));
    await flush();
    expect(sync).toHaveBeenCalledTimes(1);
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });

    // "First" after all, too late: the decision stands, and nothing becomes attended by it.
    reply(true);
    await flush();
    expect(useSyncStore.getState().attendedUntil).toBe(0);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await flush();
    expect(sync).toHaveBeenCalledTimes(1);
  });

  it("starts none of either kind right after the student stopped a sync", async () => {
    const start = startClock();
    // What Stop leaves in the store (stores/sync.test.ts): both kinds held for half an hour.
    useSyncStore.setState({ noAutomaticBefore: heldUntil(start.getTime() + 30 * MINUTE) });
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    // The launch would be attended...
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();
    // ...and a re-read ten minutes on unattended.
    later(start, 10 * MINUTE);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).not.toHaveBeenCalled();
  });

  it("picks the trigger from what is due: unattended at launch when only that is", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: { unattended: true, attended: false },
    });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("takes a window that gains focus for nobody: the full sync waits for the student's first input", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // At night another app quits and this window comes to the front: only the light sync.
    later(start, 11 * HOUR);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // In the morning the student presses a key in it: the full sync starts at that moment.
    later(start, 19 * HOUR);
    input(new KeyboardEvent("keydown", { key: "ArrowDown" }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("doesn't take a pointer that moves or a key that repeats for the student", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // Only a full sync is due, and the window has come to the front.
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: { unattended: false, attended: true },
    });
    later(start, 11 * HOUR);
    focusWindow();
    await settle();
    expect(sync).not.toHaveBeenCalled();
    const noted = useSyncStore.getState().attendedUntil;

    // The engines make up pointer moves themselves, a mouse can drift, and something can rest
    // on the keyboard: none of it needs the student.
    input(new MouseEvent("pointermove", { movementX: 0, movementY: 0 }));
    input(new MouseEvent("pointermove", { movementX: 3, movementY: 1 }));
    input(new MouseEvent("mousemove", { movementX: 3, movementY: 1 }));
    input(new KeyboardEvent("keydown", { key: "j", repeat: true }));
    input(new KeyboardEvent("keyup", { key: "j" }));
    // Nor a key that tools press to keep a computer awake, a headset's button, a modifier.
    input(new KeyboardEvent("keydown", { key: "F15" }));
    input(new KeyboardEvent("keydown", { key: "MediaPlayPause" }));
    input(new KeyboardEvent("keydown", { key: "Shift" }));
    await afterInput();
    expect(sync).not.toHaveBeenCalled();
    expect(useSyncStore.getState().attendedUntil).toBe(noted);

    // The wheel does.
    input(new WheelEvent("wheel", { deltaY: 40 }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes only the first input after a focus: working in the window for hours says nothing new", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    // The student comes to the window and presses something; nothing is due.
    later(start, MINUTE);
    comeBack();
    await afterInput();
    expect(sync).not.toHaveBeenCalled();

    // Hours later they are still typing, and the hourly re-read finds a full sync due: the
    // timer never runs that one, whoever is typing.
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: { unattended: false, attended: true },
    });
    later(start, 11 * HOUR);
    input(new KeyboardEvent("keydown", { key: "a" }));
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await afterInput();
    expect(sync).not.toHaveBeenCalled();

    // Away and back, and a press: now it runs.
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("takes no input for the student coming back unless the window gained focus first", async () => {
    // A press without a focus before it says nothing new, in a page that was loaded again too:
    // what starts by itself there is unattended.
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", reloaded: true });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();

    later(start, 31 * MINUTE);
    input(new Event("pointerdown"));
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "unattended" });
  });

  it("doesn't spend the student's input on a run that is in its way: the next one counts again", async () => {
    // Back at the window with both kinds due: the answer is there before the student has done
    // anything, so the light sync starts first, and their first press falls into it.
    const start = startClock();
    const api = mockApi();
    const held = holdSyncAll(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    later(start, 11 * HOUR);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    input(new Event("pointerdown"));
    await afterInput();

    // The light sync takes its time (a slow network after waking up): by its end that press is
    // too old to count, and nothing more starts by itself.
    later(start, 11 * HOUR + 40_000);
    held.release();
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await afterInput();
    expect(sync).toHaveBeenCalledTimes(1);

    // The student is still here, and their next press says so: the full sync starts.
    input(new Event("pointerdown"));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(2));
    expect(sync.mock.calls[1]?.[0]).toEqual({ automatic: "attended" });
  });

  it("lets what the student pressed act first: their own Sync isn't replaced by an automatic one", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { user } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // Only a full sync is due (until one has run), and the window has come to the front.
    let due = true;
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: { unattended: false, attended: due },
    });
    later(start, 11 * HOUR);
    focusWindow();
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // The first thing the student does is press "Sync now": that is the sync that runs.
    await user.click(screen.getByRole("button", { name: "Sync now" }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    due = false;
    expect(sync.mock.calls[0]?.[0]?.automatic ?? null).toBeNull();
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    expect(useSyncStore.getState().automatic).toBeNull();
    await afterInput();
    expect(sync).toHaveBeenCalledTimes(1);
  });

  it("takes the press that brought the window to the front, though it came just before the focus", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // The click on the window reaches the page a moment before the window says it has focus.
    later(start, 11 * HOUR);
    input(new Event("pointerdown"));
    later(start, 11 * HOUR + 200);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("keeps no input for a focus that comes later", async () => {
    // The student's last press of the evening, then the window gains focus at night by itself.
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    later(start, HOUR);
    input(new Event("pointerdown"));
    await afterInput();
    const noted = useSyncStore.getState().attendedUntil;

    later(start, 11 * HOUR);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
  });

  it("doesn't take a press from before a sleep for the one that woke the window", async () => {
    // The lid is closed right after a press. Opened hours later, the window gains focus by
    // itself; on the steady clock, which may have stood still, the press is a moment old.
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    later(start, HOUR);
    input(new Event("pointerdown"));
    await afterInput();
    const noted = useSyncStore.getState().attendedUntil;

    pass(300);
    sleep(8 * HOUR);
    alwaysDue(api);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
  });

  it("doesn't take an old press for the one that raised the window when the clock is set back", async () => {
    // The wall clock, set back, shows the evening's last press as half a second ago.
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    later(start, HOUR);
    input(new Event("pointerdown"));
    await afterInput();
    const noted = useSyncStore.getState().attendedUntil;

    later(start, 11 * HOUR);
    clockSetTo(start, HOUR + 500);
    alwaysDue(api);
    focusWindow();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(useSyncStore.getState().attendedUntil).toBe(noted);
  });

  it("asks at a focus when the last answer said a sync was due, and at an input when it is old", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "auto-sync-due", reloaded: true });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    const tasks = vi.spyOn(api, "startupTasks");
    renderRoute("/courses", { api });
    // The page's own light sync, and the question after it: still due, and held for now.
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    const asked = tasks.mock.calls.length;

    // A focus a minute later: the answer is young, but it said "due", so it is asked again.
    later(start, MINUTE);
    focusWindow();
    await waitFor(() => expect(tasks.mock.calls.length).toBe(asked + 1));
    await settle();
    expect(sync).toHaveBeenCalledTimes(1);

    // Nothing is due any more, and the student's input comes when that answer is old.
    const answer = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await answer()),
      whats_new: null,
      update_check_due: false,
      sync_due: { unattended: false, attended: false },
    });
    const quiet = vi.spyOn(api, "startupTasks");
    later(start, 2 * MINUTE);
    focusWindow();
    await waitFor(() => expect(quiet).toHaveBeenCalledTimes(1));
    later(start, 20 * MINUTE);
    input(new WheelEvent("wheel", { deltaY: 40 }));
    await waitFor(() => expect(quiet).toHaveBeenCalledTimes(2));
  });

  it("never runs an attended sync from the hourly re-read", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();

    // Only a full sync is due, and nobody is known to be here.
    const tasks = api.startupTasks.bind(api);
    api.startupTasks = async () => ({
      ...(await tasks()),
      sync_due: { unattended: false, attended: true },
    });
    later(start, 11 * HOUR);
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // The student comes back: now it runs, as attended.
    comeBack();
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("leaves no trace when the start is refused", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    let refuse: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      refuse = resolve;
    });
    const sync = vi.spyOn(api, "syncAll").mockImplementation(async () => {
      await gate;
      throw new ApiError("busy", "Another PageLamp window is syncing.");
    });
    const { router } = renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    // The start is in flight and has shown nothing yet: no capsule, nothing announced.
    await settle();
    expect(useSyncStore.getState().running).toBe(true);
    expect(screen.queryByRole("button", { name: /^Syncing/ })).toBeNull();
    expect(announced()).not.toContain("Syncing");

    refuse();
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expectNoTrace();
    expect(announced()).not.toContain("Syncing");

    await act(() => router.navigate("/sources"));
    await screen.findByRole("heading", { level: 1, name: "Sources & sync" });
    expect(screen.queryByRole("heading", { name: "Sync failed" })).toBeNull();
  });

  it("leaves no trace when the facade says it isn't due any more", async () => {
    // The answer said "due", but by the time the run asks, the mock's data is fresh.
    const api = mockApi();
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expectNoTrace();
    expect(screen.queryByRole("button", { name: "Sync finished" })).toBeNull();
    expect(announced()).not.toContain("Syncing");
    expect(announced()).not.toContain("Sync finished");
  });

  it("stays quiet when a source can't be reached", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = failingWith(api, "network");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expectNoTrace();
    expect(announced()).not.toContain("problems");
    expect(document.body).toHaveFocus();
  });

  it("says once to screen readers when the run left a problem on a source", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = failingWith(api, "auth_expired_or_revoked");
    // What counts is what the facade recorded on the source: here, Canvas's expired token.
    const status = api.status.bind(api);
    api.status = async () => {
      const now = await status();
      if (sync.mock.calls.length === 0) return now;
      return {
        ...now,
        sources: now.sources.map((source) =>
          source.id === CANVAS
            ? {
                ...source,
                last_error: "Synthetic failure",
                last_error_kind: "auth_expired_or_revoked" as const,
              }
            : source,
        ),
      };
    };
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(announced()).toContain("Sync finished with problems"));
    // Heard, not shown: no capsule to dismiss, nothing opened, focus unmoved.
    expectNoTrace();
    expect(document.body).toHaveFocus();
  });

  it("says nothing when the facade recorded nothing, whatever the run reported", async () => {
    // The same failed result, but the source's row is as it was: the facade kept it quiet.
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = failingWith(api, "auth_expired_or_revoked");
    renderRoute("/courses", { api });
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expectNoTrace();
    expect(announced()).not.toContain("problems");
  });

  it("waits for 'Got it' on What's new, then syncs as attended", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "upgrader" });
    const sync = vi.spyOn(api, "syncAll");
    // Opened 13 hours after the last sync.
    later(start, 11 * HOUR);
    const { user } = renderRoute("/courses", { api });

    const sheet = await screen.findByRole("dialog", { name: "What's new in PageLamp" });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // Read at leisure: by now only "Got it" says the student is here.
    later(start, 11 * HOUR + 5 * MINUTE);
    await user.click(within(sheet).getByRole("button", { name: "Got it" }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
  });

  it("doesn't take What's new going away by itself for the student closing it", async () => {
    const start = startClock();
    const api = mockApi({ scenario: "upgrader" });
    const sync = vi.spyOn(api, "syncAll");
    later(start, 11 * HOUR);
    const { queryClient } = renderRoute("/courses", { api });
    await screen.findByRole("dialog", { name: "What's new in PageLamp" });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    // Acknowledged somewhere else, minutes later: the sheet goes without a click here.
    later(start, 11 * HOUR + 5 * MINUTE);
    await api.acknowledgeWhatsNew();
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
    expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("doesn't ask for a sync while an update is being installed, and asks again afterwards", async () => {
    useUpdateStore.setState({ install: { phase: "downloading", downloaded: 0, total: null } });
    const api = mockApi({ scenario: "auto-sync-due" });
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();

    for (const phase of ["installing", "restarting", "held"] as const) {
      act(() => useUpdateStore.setState({ install: { phase } }));
      await settle(20);
      expect(sync).not.toHaveBeenCalled();
    }

    // The install didn't happen after all.
    act(() => useUpdateStore.setState({ install: { phase: "idle" } }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
  });

  it("doesn't start under an open dialog; it starts when the dialog closes", async () => {
    const start = startClock();
    const api = mockApi();
    const sync = vi.spyOn(api, "syncAll");
    const { user, queryClient } = renderRoute("/sources", { api });
    await user.click(await screen.findByRole("button", { name: "Add source" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a source" });

    // The student is at the dialog (typing in it) when the hour's answer says a sync is due.
    later(start, 13 * HOUR);
    input(new KeyboardEvent("keydown", { key: "a" }));
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    await settle();
    expect(sync).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
  });

  describe("a dialog that was left open", () => {
    /** A dialog stays open in the window, as one does for days in the tray. */
    function leaveDialogOpen() {
      const dialog = document.createElement("div");
      dialog.setAttribute("role", "dialog");
      document.body.appendChild(dialog);
      return () => dialog.remove();
    }

    /** A quiet launch with nothing due, then the hour's answer saying a sync is. */
    async function quietLaunch(options: MockOptions = {}) {
      const start = startClock();
      const api = mockApi(options);
      let due = false;
      const tasks = api.startupTasks.bind(api);
      api.startupTasks = async () => {
        const answer = await tasks();
        return due ? { ...answer, sync_due: DUE } : answer;
      };
      const sync = vi.spyOn(api, "syncAll");
      const { queryClient } = renderRoute("/courses", { api });
      await screen.findByRole("heading", { level: 1 });
      await settle();
      expect(sync).not.toHaveBeenCalled();
      const becomeDue = () => {
        due = true;
      };
      const hourly = async () => {
        becomeDue();
        await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
        await settle();
      };
      return { start, api, sync, queryClient, becomeDue, hourly };
    }

    it("lets a light sync start at the first answer that finds it untouched for a quarter of an hour", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, DIALOG_LEFT_MS - 1000);
        await hourly();
        expect(sync).not.toHaveBeenCalled();

        later(start, DIALOG_LEFT_MS);
        await hourly();
        await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
        expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
      } finally {
        close();
      }
    });

    it("counts from the last thing pressed in the window", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, HOUR);
        input(new Event("pointerdown"));
        later(start, HOUR + DIALOG_LEFT_MS - 1000);
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
    });

    it("counts from the window coming to the front too: someone may be about to use it", async () => {
      const { start, sync, becomeDue } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, 13 * HOUR);
        becomeDue();
        // The focus asks by itself (the answer is old), and finds the sync due.
        focusWindow();
        await settle();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
    });

    it("never lets a full sync start under it, however long it was left", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, 13 * HOUR);
        // The student is known to be here by something that is no input in the window.
        act(() => useSyncStore.getState().noteStudentAction());
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "attended" });
    });

    it("doesn't start under it while an update is being installed", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, 13 * HOUR);
        act(() => useUpdateStore.setState({ install: { phase: "installing" } }));
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        act(() => useUpdateStore.setState({ install: { phase: "idle" } }));
        close();
      }
    });

    it("doesn't start under it while another process syncs, whatever the answer says", async () => {
      // The answer can have been worked out a moment before the other process took the lock.
      const start = startClock();
      const api = mockApi();
      const status = api.status.bind(api);
      api.status = async () => ({ ...(await status()), sync_in_progress: true });
      alwaysDue(api);
      const sync = vi.spyOn(api, "syncAll");
      const { queryClient } = renderRoute("/courses", { api });
      await seenSyncingElsewhere(queryClient);
      const close = leaveDialogOpen();
      try {
        later(start, 13 * HOUR);
        await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
        await settle();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
    });

    it("keeps the purge and Monday's note behind the sync it starts under it", async () => {
      const { start, api, sync, queryClient, hourly } = await quietLaunch();
      const held = holdSyncAll(api);
      const close = leaveDialogOpen();
      try {
        later(start, 13 * HOUR);
        const before = useSyncStore.getState().clearedAnswer;
        await hourly();
        await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
        // Like any start: the answer it started on isn't done with for the others.
        const answers = queryClient.getQueryState(queryKeys.startupTasks())?.dataUpdateCount;
        expect(useSyncStore.getState().clearedAnswer).toBe(before);
        expect(useSyncStore.getState().clearedAnswer).toBeLessThan(answers ?? 0);
        held.release();
        await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      } finally {
        close();
      }
    });

    it("keeps a run that goes wrong quiet under the capsule's own details, left open", async () => {
      // Those details are such a dialog. Open, they tell the store that the student looks on.
      const start = startClock();
      const api = mockApi();
      let due = false;
      const tasks = api.startupTasks.bind(api);
      api.startupTasks = async () => {
        const answer = await tasks();
        return due ? { ...answer, sync_due: DUE } : answer;
      };
      const { user, queryClient } = renderRoute("/courses", { api });
      await screen.findByRole("heading", { level: 1 });
      // The student syncs, opens the details of "Sync finished" and leaves them open.
      await user.click(await screen.findByRole("button", { name: "Sync now" }));
      await user.click(await screen.findByRole("button", { name: "Sync finished" }));
      await screen.findByRole("dialog", { name: "Sync details" });
      expect(useSyncStore.getState().watched).toBe(true);

      // Hours later the hour's light sync starts under them, and Canvas can't be reached.
      later(start, 13 * HOUR);
      const sync = failingWith(api, "network");
      due = true;
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
      await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      await settle();
      expectNoTrace();
      expect(announced()).not.toContain("problems");
      // The details went with the capsule; focus that was in them is on the page, not on
      // something that is no longer shown.
      expect(document.getElementById("main")).toHaveFocus();
    });

    /** A light run that is held until `fail()`, and then can't reach Canvas. */
    function heldThenFailing(api: PageLampApi) {
      let fail: () => void = () => {};
      const gate = new Promise<void>((resolve) => {
        fail = resolve;
      });
      const sync = vi
        .spyOn(api, "syncAll")
        .mockImplementation(async (_req, onEvent: (event: SyncEvent) => void) => {
          onEvent({ type: "source_started", source_id: CANVAS, label: "Demo Canvas" });
          await gate;
          onEvent({
            type: "source_finished",
            source_id: CANVAS,
            ok: false,
            error: "Synthetic failure",
            error_kind: "network",
          });
          const summary: SyncSummary = {
            started_at: "2026-10-05T13:00:00Z",
            finished_at: "2026-10-05T13:00:01Z",
            ok: false,
            results: [failedCanvas("network")],
          };
          return summary;
        });
      return { sync, fail: () => fail() };
    }

    /** The student syncs; afterwards a sync becomes due when the test says so. */
    async function afterTheStudentsSync() {
      const start = startClock();
      const api = mockApi();
      let due = false;
      const tasks = api.startupTasks.bind(api);
      api.startupTasks = async () => {
        const answer = await tasks();
        return due ? { ...answer, sync_due: DUE } : answer;
      };
      const { user, queryClient } = renderRoute("/courses", { api });
      await screen.findByRole("heading", { level: 1 });
      await user.click(await screen.findByRole("button", { name: "Sync now" }));
      const capsule = await screen.findByRole("button", { name: "Sync finished" });
      const hourly = async () => {
        due = true;
        await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      };
      return { start, api, user, capsule, hourly };
    }

    it("shows that run's problem after all when the student is back at the details before it ends", async () => {
      const { start, api, user, capsule, hourly } = await afterTheStudentsSync();
      await user.click(capsule);
      const details = await screen.findByRole("dialog", { name: "Sync details" });

      // Left open. Hours later the hour's light sync starts under them, watched by nobody.
      later(start, 13 * HOUR);
      const { sync, fail } = heldThenFailing(api);
      await hourly();
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      try {
        expect(useSyncStore.getState().unwatched).toBe(true);
        // The student comes back while it runs and presses in the details: they look on now.
        input(new Event("pointerdown"));
        expect(useSyncStore.getState().unwatched).toBe(false);
        expect(details).toBeInTheDocument();
      } finally {
        // It can't reach Canvas. Nothing vanishes in front of them: the capsule says so.
        fail();
      }
      await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      expect(
        await screen.findByRole("button", { name: "Sync finished with problems" }),
      ).toBeInTheDocument();
      expect(useSyncStore.getState().lastSummary).not.toBeNull();
    });

    it("changes nothing where no dialog is open: a capsule the student rests on still shows the problem", async () => {
      const { start, api, capsule, hourly } = await afterTheStudentsSync();
      // Focus rests on the capsule ("Sync finished" stays for that); its details are closed.
      act(() => capsule.focus());
      await waitFor(() => expect(useSyncStore.getState().watched).toBe(true));
      expect(screen.queryByRole("dialog")).toBeNull();

      // Hours later, with nothing in its way, the hour's light sync runs and can't reach Canvas.
      later(start, 13 * HOUR);
      const { sync, fail } = heldThenFailing(api);
      await hourly();
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      try {
        expect(useSyncStore.getState().unwatched).toBe(false);
      } finally {
        fail();
      }
      await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      expect(
        await screen.findByRole("button", { name: "Sync finished with problems" }),
      ).toBeInTheDocument();
    });

    it("doesn't start under it while the app is writing", async () => {
      // Clearing removed courses, removing or restoring one: they hold the lock a sync needs.
      const { start, sync, queryClient, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      let written: () => void = () => {};
      const write = queryClient.getMutationCache().build(queryClient, {
        mutationFn: () =>
          new Promise<void>((resolve) => {
            written = resolve;
          }),
      });
      try {
        later(start, 13 * HOUR);
        void write.execute(undefined);
        await waitFor(() => expect(queryClient.isMutating()).toBe(1));
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        written();
        close();
      }
    });

    it("lets the student finish what they left in the dialog while that run is going", async () => {
      // Adding a source needs nothing the run holds: nothing typed is lost, nothing is refused.
      const start = startClock();
      const api = mockApi();
      let due = false;
      const tasks = api.startupTasks.bind(api);
      api.startupTasks = async () => {
        const answer = await tasks();
        return due ? { ...answer, sync_due: DUE } : answer;
      };
      const held = holdSyncAll(api);
      const sync = vi.spyOn(api, "syncAll");
      const one = vi.spyOn(api, "syncSource");
      const { user, queryClient } = renderRoute("/sources", { api });
      await user.click(await screen.findByRole("button", { name: "Add source" }));
      const dialog = await screen.findByRole("dialog", { name: "Add a source" });
      await user.click(within(dialog).getByRole("checkbox", { name: "I understand" }));
      await user.click(within(dialog).getByRole("button", { name: "Choose folder…" }));
      await within(dialog).findByDisplayValue("/Users/demo/Documents/Courses");

      // They leave it at that. Hours later the hour's light sync starts under the dialog.
      later(start, 13 * HOUR);
      due = true;
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });

      // Back at the window while it runs: the form is as they left it, and "Add source" adds.
      expect(within(dialog).getByDisplayValue("/Users/demo/Documents/Courses")).toBeInTheDocument();
      await user.click(within(dialog).getByRole("button", { name: "Add source" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      expect(
        await screen.findByText("Source added. It will be included in the next sync."),
      ).toBeInTheDocument();
      expect(one).not.toHaveBeenCalled();

      // And the new source is synced as soon as that run has ended.
      held.release();
      await waitFor(() => expect(one).toHaveBeenCalledTimes(1));
    });

    it("doesn't take a dialog the student is typing in for left when the clock is set back", async () => {
      // On the wall clock every dialog would look left for as long as the clock is behind.
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, 2 * HOUR);
        input(new KeyboardEvent("keydown", { key: "a" }));
        // An hour back: on the wall clock their last key lies an hour ahead.
        clockSetTo(start, HOUR);
        pass(MINUTE);
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
    });

    it("counts the quarter of an hour on the steady clock, whatever the wall clock is set to", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        // Set forward by a day: no time has passed, and nothing was left.
        later(start, MINUTE);
        clockSetTo(start, 24 * HOUR);
        await hourly();
        expect(sync).not.toHaveBeenCalled();
        // Set back by a day, and a quarter of an hour passes: it was.
        clockSetTo(start, -24 * HOUR);
        pass(DIALOG_LEFT_MS);
        await hourly();
        await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
        expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
      } finally {
        close();
      }
    });

    it("waits when the computer slept: the steady clock may have stood still", async () => {
      const { start, sync, hourly } = await quietLaunch();
      const close = leaveDialogOpen();
      try {
        later(start, 5 * MINUTE);
        sleep(8 * HOUR);
        await hourly();
        expect(sync).not.toHaveBeenCalled();
      } finally {
        close();
      }
    });

    it("leaves What's new alone in a window that started hidden, for hours", async () => {
      // An upgrader's start at login: What's new is such a dialog, and must be read first.
      const start = startClock();
      const api = mockApi({ scenario: "upgrader", startedHidden: true });
      alwaysDue(api);
      const sync = vi.spyOn(api, "syncAll");
      const { queryClient } = renderRoute("/courses", { api });
      await screen.findByRole("dialog", { name: "What's new in PageLamp" });
      later(start, 13 * HOUR);
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
      await settle();
      expect(sync).not.toHaveBeenCalled();
      expect(screen.getByRole("dialog", { name: "What's new in PageLamp" })).toBeInTheDocument();
    });
  });

  describe("the shell's tick", () => {
    /** The shell's ticker fires, as it does every quarter of an hour. */
    function tick(times = 1) {
      act(() => {
        for (let i = 0; i < times; i++) window.dispatchEvent(new Event(MOCK_STARTUP_CHECK_EVENT));
      });
    }

    /** A launch with nothing due. Counts the questions; later answers are as the test says. */
    async function launched() {
      const start = startClock();
      const api = mockApi();
      let due: { unattended: boolean; attended: boolean } | null = null;
      let failing = false;
      const tasks = api.startupTasks.bind(api);
      const asked = vi.fn();
      api.startupTasks = async () => {
        asked();
        if (failing) throw new ApiError("internal", "Synthetic failure");
        const answer = await tasks();
        return due ? { ...answer, sync_due: due } : answer;
      };
      const sync = vi.spyOn(api, "syncAll");
      renderRoute("/courses", { api });
      await screen.findByRole("heading", { level: 1 });
      await settle();
      expect(asked).toHaveBeenCalledTimes(1);
      expect(sync).not.toHaveBeenCalled();
      return {
        start,
        sync,
        asked,
        becomeDue: (what = DUE) => {
          due = what;
        },
        fail: (on: boolean) => {
          failing = on;
        },
      };
    }

    it("asks nothing while the last question is younger than the page's own hour", async () => {
      const { start, sync, asked, becomeDue } = await launched();
      becomeDue();
      later(start, TICK_REREAD_MS - 1000);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(1);
      expect(sync).not.toHaveBeenCalled();
    });

    it("asks when the page's timer is late, and what is due then starts unattended", async () => {
      const { start, sync, asked, becomeDue } = await launched();
      becomeDue();
      later(start, TICK_REREAD_MS);
      tick();
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
      // The question it asked, and the one after the run.
      await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      await settle();
      expect(asked).toHaveBeenCalledTimes(3);
    });

    it("asks once when several ticks come at once", async () => {
      const { start, asked } = await launched();
      later(start, 9 * HOUR);
      tick(5);
      await settle();
      expect(asked).toHaveBeenCalledTimes(2);
    });

    it("counts from the last question, answered or not: no retry every quarter of an hour", async () => {
      const { start, asked, fail } = await launched();
      fail(true);
      later(start, 2 * HOUR);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(2);

      later(start, 2 * HOUR + 15 * MINUTE);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(2);

      later(start, 2 * HOUR + TICK_REREAD_MS);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(3);
    });

    it("asks when the clock went back", async () => {
      const { start, asked } = await launched();
      clockSetTo(start, -HOUR);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(2);
    });

    it("is nobody: a full sync that is due doesn't start, and no input is waited for", async () => {
      const { start, sync, asked, becomeDue } = await launched();
      becomeDue({ unattended: false, attended: true });
      const noted = useSyncStore.getState().attendedUntil;
      later(start, 9 * HOUR);
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(2);
      expect(sync).not.toHaveBeenCalled();
      expect(useSyncStore.getState().attendedUntil).toBe(noted);

      // A press with no focus before it stays what it was: nothing new about the student.
      input(new Event("pointerdown"));
      await afterInput();
      expect(useSyncStore.getState().attendedUntil).toBe(noted);
      expect(sync).not.toHaveBeenCalled();
    });

    it("leaves the question to what the student just did", async () => {
      const { start, asked } = await launched();
      later(start, 9 * HOUR);
      act(() => useSyncStore.getState().noteStudentAction());
      tick();
      await settle();
      expect(asked).toHaveBeenCalledTimes(1);
    });
  });

  it("reads a start at login from the window's own flag: only an unattended sync", async () => {
    // What window.rs writes for `--hidden`, read by the real client, not by the mock's option.
    window.__PAGELAMP_WINDOW__ = Object.freeze({ backdrop: "none", hidden: true });
    try {
      const api = mockApi({ scenario: "auto-sync-due" });
      api.startedHidden = createTauriApi().startedHidden;
      const sync = vi.spyOn(api, "syncAll");
      renderRoute("/courses", { api });
      await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
      expect(sync.mock.calls[0]?.[0]).toEqual({ automatic: "unattended" });
      await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
      await settle();
      // Nothing "attended" follows by itself.
      expect(sync.mock.calls.map((call) => call[0])).toEqual([{ automatic: "unattended" }]);
    } finally {
      Reflect.deleteProperty(window, "__PAGELAMP_WINDOW__");
    }
  });

  it("still asks again when a dialog was opened while it waited for a sync to end", async () => {
    const start = startClock();
    const api = mockApi();
    const held = holdSourceSync(api);
    const all = vi.spyOn(api, "syncAll");
    const { user, queryClient } = renderRoute("/sources", { api });
    await screen.findByRole("heading", { level: 2, name: "Course calendar" });

    later(start, 13 * HOUR);
    await user.click(screen.getByRole("button", { name: "Sync Course calendar" }));
    await waitFor(() => expect(useSyncStore.getState().running).toBe(true));
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
    // The student opens a dialog before that sync ends.
    await user.click(screen.getByRole("button", { name: "Add source" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a source" });
    held.release();
    await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
    await settle();
    expect(all).not.toHaveBeenCalled();

    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(all).toHaveBeenCalledTimes(1));
  });

  it("never runs during onboarding", async () => {
    const api = mockApi({ scenario: "empty" });
    alwaysDue(api);
    const tasks = vi.spyOn(api, "startupTasks");
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/welcome", { api });
    await screen.findByRole("heading", { level: 1, name: "Welcome to PageLamp" });
    await settle();
    expect(tasks).not.toHaveBeenCalled();
    expect(sync).not.toHaveBeenCalled();
  });

  it("does nothing without sources", async () => {
    const api = mockApi({ scenario: "empty" });
    alwaysDue(api);
    const sync = vi.spyOn(api, "syncAll");
    renderRoute("/courses", { api });
    await screen.findByRole("heading", { level: 1 });
    await settle();
    expect(sync).not.toHaveBeenCalled();
  });

  it("syncs a source added during an automatic run as soon as that run ends", async () => {
    const api = mockApi({ scenario: "auto-sync-due" });
    const real = api.syncAll;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    api.syncAll = async (req, onEvent: (event: SyncEvent) => void) => {
      onEvent({ type: "source_started", source_id: "folder:demo-courses", label: "Course folder" });
      await gate;
      return real(req, onEvent);
    };
    const one = vi.spyOn(api, "syncSource");
    const { user } = renderRoute("/sources", { api });
    await screen.findByRole("button", { name: "Syncing · Course folder" });

    await user.click(screen.getByRole("button", { name: "Add source" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a source" });
    await user.click(within(dialog).getByRole("checkbox", { name: "I understand" }));
    await user.click(within(dialog).getByRole("button", { name: "Choose folder…" }));
    await within(dialog).findByDisplayValue("/Users/demo/Documents/Courses");
    await user.click(within(dialog).getByRole("button", { name: "Add source" }));
    expect(
      await screen.findByText("Source added. It will be included in the next sync."),
    ).toBeInTheDocument();
    expect(one).not.toHaveBeenCalled();

    release();
    await waitFor(() => expect(one).toHaveBeenCalledTimes(1));
  });
});
