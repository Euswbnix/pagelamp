// The Tauri client's window-focus helper, against Tauri's mocked IPC and events.

import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { afterEach, expect, it, vi } from "vitest";
import { createTauriApi } from "./tauri";

afterEach(() => clearMocks());

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

it("calls back when the window gains focus, not when it loses it, until stopped", async () => {
  mockWindows("main");
  mockIPC(() => null, { shouldMockEvents: true });
  const onFocus = vi.fn();
  const stop = createTauriApi().onWindowFocus(onFocus);
  await flush();

  await emit("tauri://focus");
  await emit("tauri://blur");
  expect(onFocus).toHaveBeenCalledTimes(1);

  stop();
  await emit("tauri://focus");
  expect(onFocus).toHaveBeenCalledTimes(1);
});

it("stops cleanly when stopped before listening has started", async () => {
  mockWindows("main");
  mockIPC(() => null, { shouldMockEvents: true });
  const onFocus = vi.fn();
  createTauriApi().onWindowFocus(onFocus)();
  await flush();
  await emit("tauri://focus");
  expect(onFocus).not.toHaveBeenCalled();
});

it("asks once per page whether this load is the launch, and keeps the answer", async () => {
  mockWindows("main");
  const asked = vi.fn(() => true);
  mockIPC((cmd) => (cmd === "first_page_load" ? asked() : null));
  const api = createTauriApi();
  expect(await api.firstPageLoad()).toBe(true);
  // Rust says "first" only once per process: asking it again would turn the launch into a reload.
  expect(await api.firstPageLoad()).toBe(true);
  expect(asked).toHaveBeenCalledTimes(1);
});

it("says when the page loaded: one time, taken as the page starts and kept", () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  try {
    vi.setSystemTime(new Date(2026, 9, 5, 23, 0));
    mockWindows("main");
    mockIPC(() => null);
    const api = createTauriApi();
    expect(api.pageLoadedAt()).toBe(Date.now());
    // A night later (the computer slept) it still names the same moment.
    vi.setSystemTime(new Date(2026, 9, 6, 7, 0));
    expect(api.pageLoadedAt()).toBe(new Date(2026, 9, 5, 23, 0).getTime());
  } finally {
    vi.useRealTimers();
  }
});

it("takes the load for a reload when Rust says so, or when it can't be asked", async () => {
  mockWindows("main");
  mockIPC((cmd) => (cmd === "first_page_load" ? false : null));
  expect(await createTauriApi().firstPageLoad()).toBe(false);

  clearMocks();
  mockWindows("main");
  mockIPC(() => {
    throw new Error("Synthetic IPC failure");
  });
  // Unsure is never "the student just opened PageLamp".
  expect(await createTauriApi().firstPageLoad()).toBe(false);
});
