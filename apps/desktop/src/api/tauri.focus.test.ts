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

it("calls back when the window loses focus, not when it gains it, until stopped", async () => {
  mockWindows("main");
  mockIPC(() => null, { shouldMockEvents: true });
  const onBlur = vi.fn();
  const stop = createTauriApi().onWindowBlur(onBlur);
  await flush();

  await emit("tauri://focus");
  expect(onBlur).not.toHaveBeenCalled();
  await emit("tauri://blur");
  expect(onBlur).toHaveBeenCalledTimes(1);

  stop();
  await emit("tauri://blur");
  expect(onBlur).toHaveBeenCalledTimes(1);
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

it("listens for the student's input in the window, and takes only the browser's own events", async () => {
  mockWindows("main");
  // (It also listens for the window losing focus: an event of Tauri's.)
  mockIPC(() => null, { shouldMockEvents: true });
  const add = vi.spyOn(window, "addEventListener");
  const remove = vi.spyOn(window, "removeEventListener");
  try {
    const onInput = vi.fn();
    const onRelease = vi.fn();
    const stop = createTauriApi().onStudentInput(onInput, onRelease);
    const heard = (type: string) => add.mock.calls.filter(([name]) => name === type);
    const types = [
      "pointerdown",
      "click",
      "keydown",
      "pointerup",
      "pointercancel",
      "contextmenu",
      "dragend",
      "keyup",
    ];
    for (const type of types) {
      expect(heard(type), type).toHaveLength(1);
      expect(heard(type)[0]?.[2], type).toEqual({ capture: true, passive: true });
    }
    // Scrolling and a pointer that moves are never heard at all.
    for (const type of ["wheel", "scroll", "pointermove", "mousemove"]) {
      expect(heard(type), type).toHaveLength(0);
    }

    // Nothing a script dispatches is the student (and a test can dispatch nothing else)...
    window.dispatchEvent(new Event("pointerdown"));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    window.dispatchEvent(new Event("pointerup"));
    expect(onInput).not.toHaveBeenCalled();
    expect(onRelease).not.toHaveBeenCalled();
    // ...so the listener is handed what the browser would give it.
    const listener = heard("pointerdown")[0]?.[1] as (event: unknown) => void;
    listener({ type: "pointerdown", isTrusted: true });
    expect(onInput.mock.calls).toEqual([["press"]]);
    listener({ type: "keydown", isTrusted: true, key: "a", repeat: true });
    listener({ type: "keydown", isTrusted: false, key: "a" });
    listener({ type: "keydown", isTrusted: true, key: "Tab", metaKey: true });
    listener({ type: "wheel", isTrusted: true });
    expect(onInput.mock.calls).toEqual([["press"]]);
    listener({ type: "pointerup", isTrusted: true });
    listener({ type: "keyup", isTrusted: true, key: "a" });
    expect(onRelease.mock.calls).toEqual([["pointer"], ["key"]]);

    stop();
    for (const type of types) {
      expect(remove, type).toHaveBeenCalledWith(type, listener, { capture: true, passive: true });
    }
    // (Listening for the loss of focus starts a moment later, and is stopped then.)
    await flush();
  } finally {
    add.mockRestore();
    remove.mockRestore();
  }
});

it("writes an automatic start to the log, and never rejects", async () => {
  mockWindows("main");
  const calls: Array<[string, unknown]> = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return null;
  });
  await createTauriApi().logAutoSyncStart({
    trigger: "attended",
    noted_by: "press",
    noted_ms_ago: 412,
  });
  expect(calls).toEqual([
    ["log_auto_sync_start", { trigger: "attended", notedBy: "press", notedMsAgo: 412 }],
  ]);

  clearMocks();
  mockWindows("main");
  mockIPC(() => {
    throw new Error("Synthetic IPC failure");
  });
  await expect(
    createTauriApi().logAutoSyncStart({
      trigger: "unattended",
      noted_by: null,
      noted_ms_ago: null,
    }),
  ).resolves.toBeUndefined();
});

it("lets go of a repeating key when the window loses focus", async () => {
  mockWindows("main");
  mockIPC(() => null, { shouldMockEvents: true });
  const add = vi.spyOn(window, "addEventListener");
  try {
    const onInput = vi.fn();
    const stop = createTauriApi().onStudentInput(onInput);
    await flush();
    const listener = add.mock.calls.find(([name]) => name === "click")?.[1] as (
      event: unknown,
    ) => void;
    // While a key repeats, a click with no pointer is the key's own.
    listener({ type: "keydown", isTrusted: true, key: "Enter", repeat: true });
    listener({ type: "click", isTrusted: true, detail: 0 });
    expect(onInput).not.toHaveBeenCalled();
    // The window loses focus, and the key's release is never heard: back in the window, a
    // click with no press before it counts again.
    await emit("tauri://blur");
    listener({ type: "click", isTrusted: true, detail: 0 });
    expect(onInput.mock.calls).toEqual([["click"]]);
    stop();
  } finally {
    add.mockRestore();
  }
});
