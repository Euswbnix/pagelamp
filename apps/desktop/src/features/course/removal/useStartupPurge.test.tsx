import { act, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import { createMockApi } from "@/api/mock";
import { queryKeys } from "@/api/queries";
import type { PurgeReport } from "@/api/types";
import { useSyncStore } from "@/stores/sync";
import { renderRoute } from "@/test/render";

/** What a purge with nothing to clear answers. */
const NOTHING_PURGED: PurgeReport = {
  purged: [],
  files_pending: [],
  backup_deleted: false,
  backup_failed: false,
};

afterEach(async () => {
  // A sync still in flight would end inside the next test, in the store they share.
  await waitFor(() => expect(useSyncStore.getState().running).toBe(false), { timeout: 5000 });
});

it("runs the app-start purge once when the facade says it's due", async () => {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({ ...(await tasks()), purge_due: true });
  const purge = vi.spyOn(api, "purgeRemovedCourses");
  renderRoute("/courses", { api });
  await screen.findByRole("heading", { level: 1 });
  await waitFor(() => expect(purge).toHaveBeenCalledWith(null, false));
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(purge).toHaveBeenCalledTimes(1);
});

it("doesn't purge when nothing is due", async () => {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  const purge = vi.spyOn(api, "purgeRemovedCourses");
  renderRoute("/courses", { api });
  await screen.findByRole("heading", { level: 1 });
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(purge).not.toHaveBeenCalled();
});

it("leaves the purge to the automatic sync when one is due at the same answer", async () => {
  // Both would take the sync lock at once; the sync does the purge itself as it starts.
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  let synced = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: !synced,
    sync_due: { unattended: !synced, attended: !synced },
  });
  const sync = api.syncAll.bind(api);
  const order: string[] = [];
  api.syncAll = async (...args) => {
    order.push("sync");
    const summary = await sync(...args);
    synced = true;
    return summary;
  };
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockImplementation(async () => {
    order.push("purge");
    return NOTHING_PURGED;
  });
  renderRoute("/courses", { api });
  await waitFor(() => expect(order).toContain("sync"));
  await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(order).toEqual(["sync"]);
  expect(purge).not.toHaveBeenCalled();
});

it("runs after a sync that didn't do it, on the answer that follows", async () => {
  // The automatic sync was refused (another app held the lock): the purge is still due.
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  let tried = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: true,
    sync_due: { unattended: !tried, attended: !tried },
  });
  const order: string[] = [];
  api.syncAll = async () => {
    order.push("sync");
    tried = true;
    throw new ApiError("busy", "Another sync is running.");
  };
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockImplementation(async () => {
    order.push("purge");
    return NOTHING_PURGED;
  });
  renderRoute("/courses", { api });
  await waitFor(() => expect(purge).toHaveBeenCalledWith(null, false));
  expect(order).toEqual(["sync", "purge"]);
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(purge).toHaveBeenCalledTimes(1);
});

it("asks again after the purge: while it held the lock, a sync that was due couldn't show", async () => {
  // As the facade answers: while anything holds the sync lock, nothing is due.
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  let due = false;
  let purging = false;
  let purged = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: !purged,
    sync_due: purging ? { unattended: false, attended: false } : { unattended: due, attended: due },
  });
  let done: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    done = resolve;
  });
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockImplementation(async () => {
    purging = true;
    await held;
    purging = false;
    purged = true;
    return NOTHING_PURGED;
  });
  const sync = vi.spyOn(api, "syncAll");
  const { queryClient } = renderRoute("/courses", { api });
  // Nothing is due at launch, so the purge starts; it holds the lock for a while.
  await waitFor(() => expect(purge).toHaveBeenCalledTimes(1));

  // A sync becomes due meanwhile. The answer asked now says "not due", because of the lock.
  due = true;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(sync).not.toHaveBeenCalled();

  // The purge is done: asked again, the sync shows and starts.
  done();
  await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
  expect(sync.mock.calls[0]?.[0]).toMatchObject({ automatic: expect.any(String) });
  expect(purge).toHaveBeenCalledTimes(1);
});

it("doesn't start a sync into a write that is going, though the answer says one is due", async () => {
  // The answer can be worked out a moment before the write takes the lock.
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  let due = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: true,
    sync_due: { unattended: due, attended: due },
  });
  let done: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    done = resolve;
  });
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockImplementation(async () => {
    await held;
    return NOTHING_PURGED;
  });
  const sync = vi.spyOn(api, "syncAll");
  const { queryClient } = renderRoute("/courses", { api });
  await waitFor(() => expect(purge).toHaveBeenCalledTimes(1));

  due = true;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(sync).not.toHaveBeenCalled();

  done();
  await waitFor(() => expect(sync).toHaveBeenCalledTimes(1));
});

it("waits for the student's own sync, which does the purge as it starts", async () => {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  // The answer keeps saying "due" until the sync has done it, also while the sync runs.
  let synced = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({ ...(await tasks()), purge_due: !synced });
  let done: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    done = resolve;
  });
  const sync = api.syncAll.bind(api);
  api.syncAll = async (...args) => {
    const summary = await sync(...args);
    await held;
    synced = true;
    return summary;
  };
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockResolvedValue(NOTHING_PURGED);
  // Before the first answer is there, the student starts a sync.
  const start = api.startupTasks;
  let first: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    first = resolve;
  });
  api.startupTasks = async () => {
    await gate;
    return start();
  };
  const { user } = renderRoute("/courses", { api });
  await user.click(await screen.findByRole("button", { name: "Sync now" }));
  await waitFor(() => expect(useSyncStore.getState().running).toBe(true));
  try {
    first();
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(purge).not.toHaveBeenCalled();
  } finally {
    done();
  }
  await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
  await new Promise((resolve) => setTimeout(resolve, 100));
  // The sync did it: nothing is left for the hook.
  expect(purge).not.toHaveBeenCalled();
});

it("goes ahead while What's new is up", async () => {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "upgrader" });
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({ ...(await tasks()), purge_due: true });
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockResolvedValue(NOTHING_PURGED);
  renderRoute("/courses", { api });
  await screen.findByRole("dialog", { name: "What's new in PageLamp" });
  await waitFor(() => expect(purge).toHaveBeenCalledTimes(1));
});

it("goes ahead when a sync is due but held for half an hour", async () => {
  // The student stopped a sync a minute ago: nothing automatic starts for half an hour.
  const until = Date.now() + 29 * 60_000;
  useSyncStore.setState({ noAutomaticBefore: { unattended: until, attended: until } });
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: true,
    sync_due: { unattended: true, attended: true },
  });
  const sync = vi.spyOn(api, "syncAll");
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockResolvedValue(NOTHING_PURGED);
  renderRoute("/courses", { api });
  await waitFor(() => expect(purge).toHaveBeenCalledTimes(1));
  expect(sync).not.toHaveBeenCalled();
});

it("waits under What's new too while the student's own sync runs", async () => {
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "upgrader" });
  let synced = false;
  const tasks = api.startupTasks.bind(api);
  // The student starts a sync before the first answer is there.
  let answer: () => void = () => {};
  const asked = new Promise<void>((resolve) => {
    answer = resolve;
  });
  api.startupTasks = async () => {
    await asked;
    return { ...(await tasks()), purge_due: !synced };
  };
  let done: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    done = resolve;
  });
  const sync = api.syncAll.bind(api);
  api.syncAll = async (...args) => {
    const summary = await sync(...args);
    await held;
    synced = true;
    return summary;
  };
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockResolvedValue(NOTHING_PURGED);
  const { user } = renderRoute("/courses", { api });
  await user.click(await screen.findByRole("button", { name: "Sync now" }));
  await waitFor(() => expect(useSyncStore.getState().running).toBe(true));
  try {
    answer();
    await screen.findByRole("dialog", { name: "What's new in PageLamp" });
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(purge).not.toHaveBeenCalled();
  } finally {
    done();
  }
});

it("leaves the purge to the sync when both become due at a later answer", async () => {
  // The tray keeps the app open: a removed course's time runs out in the hour a sync is due.
  const api = createMockApi({ latencyMs: 0, syncStepMs: 0, scenario: "removed" });
  let due = false;
  let synced = false;
  const tasks = api.startupTasks.bind(api);
  api.startupTasks = async () => ({
    ...(await tasks()),
    purge_due: due && !synced,
    sync_due: { unattended: due && !synced, attended: due && !synced },
  });
  const order: string[] = [];
  const sync = api.syncAll.bind(api);
  api.syncAll = async (...args) => {
    order.push("sync");
    const summary = await sync(...args);
    synced = true;
    return summary;
  };
  const purge = vi.spyOn(api, "purgeRemovedCourses").mockImplementation(async () => {
    order.push("purge");
    return NOTHING_PURGED;
  });
  const { queryClient } = renderRoute("/courses", { api });
  await screen.findByRole("heading", { level: 1 });
  // The first answer has nothing for either, and is done with.
  await waitFor(() => expect(useSyncStore.getState().clearedAnswer).toBe(1));

  due = true;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.startupTasks() }));
  await waitFor(() => expect(order).toContain("sync"));
  await waitFor(() => expect(useSyncStore.getState().running).toBe(false));
  await new Promise((resolve) => setTimeout(resolve, 100));
  expect(order).toEqual(["sync"]);
  expect(purge).not.toHaveBeenCalled();
});
