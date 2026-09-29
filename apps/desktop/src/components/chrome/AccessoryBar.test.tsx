import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/api/errors";
import type { SyncSummary } from "@/api/types";
import { paths } from "@/lib/routes";
import { useSyncStore } from "@/stores/sync";
import { renderRoute } from "@/test/render";
import { FINISHED_MS } from "./AccessoryBar";

const summary = (ok: boolean): SyncSummary => ({
  started_at: "2026-09-28T12:00:00Z",
  finished_at: "2026-09-28T12:01:00Z",
  ok,
  results: [],
});

function startRun() {
  act(() => {
    const store = useSyncStore.getState();
    store.begin(2);
    store.apply({ type: "source_started", source_id: "canvas", label: "Demo Canvas" });
  });
}

function finishCanvas(ok: boolean) {
  act(() =>
    useSyncStore.getState().apply({
      type: "source_finished",
      source_id: "canvas",
      ok,
      error: ok ? null : "Synthetic failure",
      error_kind: ok ? null : "network",
    }),
  );
}

async function renderIdle() {
  const result = renderRoute("/settings");
  await screen.findByRole("heading", { level: 1, name: "Settings" });
  return result;
}

const announced = () =>
  screen
    .getAllByRole("status")
    .map((s) => s.textContent)
    .join(" ");

afterEach(() => {
  vi.useRealTimers();
});

describe("AccessoryBar", () => {
  it("stays out of sight (and out of reach) when nothing is syncing", async () => {
    await renderIdle();
    expect(screen.queryByRole("button", { name: /sync/i })).toBeNull();
  });

  it("counts sources while this window syncs, and announces only the start", async () => {
    await renderIdle();
    startRun();
    expect(screen.getByRole("button", { name: "Syncing 0 of 2 · Demo Canvas" })).toBeVisible();
    expect(announced()).toContain("Syncing…");

    finishCanvas(true);
    act(() =>
      useSyncStore
        .getState()
        .apply({ type: "source_started", source_id: "folder", label: "Course folder" }),
    );
    expect(screen.getByRole("button", { name: "Syncing 1 of 2 · Course folder" })).toBeVisible();
    expect(announced()).not.toContain("Course folder");

    // Every source done, the run not over yet.
    act(() =>
      useSyncStore
        .getState()
        .apply({ type: "source_finished", source_id: "folder", ok: true, error: null }),
    );
    expect(screen.getByRole("button", { name: "Syncing 2/2…" })).toBeVisible();
  });

  it("says 'Sync finished' for 4 seconds, then leaves", async () => {
    await renderIdle();
    startRun();
    vi.useFakeTimers();
    finishCanvas(true);
    act(() => useSyncStore.getState().finish(summary(true), null));
    expect(screen.getByRole("button", { name: "Sync finished" })).toBeInTheDocument();
    expect(announced()).toContain("Sync finished");

    act(() => vi.advanceTimersByTime(FINISHED_MS - 1));
    expect(screen.getByRole("button", { name: "Sync finished" })).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByRole("button", { name: "Sync finished" })).toBeNull();
  });

  it("keeps a problem until it is dismissed with × or Esc", async () => {
    await renderIdle();
    startRun();
    vi.useFakeTimers();
    finishCanvas(false);
    act(() => useSyncStore.getState().finish(summary(false), null));
    expect(announced()).toContain("Sync finished with problems");
    act(() => vi.advanceTimersByTime(FINISHED_MS * 2));
    expect(screen.getByRole("button", { name: "Sync finished with problems" })).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("button", { name: "Sync finished with problems" })).toBeNull();

    vi.useRealTimers();
    startRun();
    act(() => useSyncStore.getState().finish(null, new ApiError("network", "Synthetic failure")));
    expect(screen.getByRole("button", { name: "Sync failed" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("button", { name: "Sync failed" })).toBeNull();
  });

  it("leaves quietly when the student stopped the sync, but says so to screen readers", async () => {
    await renderIdle();
    startRun();
    act(() => useSyncStore.getState().finish(null, new ApiError("cancelled", "Cancelled")));
    expect(screen.queryByRole("button", { name: /sync/i })).toBeNull();
    expect(announced()).toContain("Sync stopped");
  });

  it("waits while focused, and hands focus to the page when it leaves", async () => {
    await renderIdle();
    startRun();
    vi.useFakeTimers();
    finishCanvas(true);
    act(() => useSyncStore.getState().finish(summary(true), null));
    const capsule = screen.getByRole("button", { name: "Sync finished" });
    act(() => capsule.focus());
    act(() => vi.advanceTimersByTime(FINISHED_MS * 2));
    expect(capsule).toHaveFocus();

    // A problem dismissed with ×: focus goes to the page, not to <body>.
    vi.useRealTimers();
    startRun();
    act(() => useSyncStore.getState().finish(null, new ApiError("network", "Synthetic failure")));
    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    act(() => dismiss.focus());
    fireEvent.click(dismiss);
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("opens per-source progress with Stop and a way to Sources & sync", async () => {
    const { user } = await renderIdle();
    startRun();
    await user.click(screen.getByRole("button", { name: "Syncing 0 of 2 · Demo Canvas" }));

    const details = await screen.findByRole("dialog", { name: "Sync details" });
    await waitFor(() => expect(details).toHaveFocus());
    expect(within(details).getByText("Demo Canvas")).toBeInTheDocument();
    expect(within(details).getByRole("button", { name: "Stop" })).toBeInTheDocument();
    const link = within(details).getByRole("link", { name: "Open Sources & sync" });
    expect(link).toHaveAttribute("href", paths.sources);

    await user.click(link);
    expect(await screen.findByRole("heading", { level: 1, name: "Sources & sync" })).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Sync details" })).toBeNull();
  });
});
