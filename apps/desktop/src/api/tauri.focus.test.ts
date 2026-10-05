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

it("listens for the student's input in the window, and takes only the browser's own events", () => {
  mockWindows("main");
  mockIPC(() => null);
  const add = vi.spyOn(window, "addEventListener");
  const remove = vi.spyOn(window, "removeEventListener");
  try {
    const onInput = vi.fn();
    const stop = createTauriApi().onStudentInput(onInput);
    const heard = (type: string) => add.mock.calls.filter(([name]) => name === type);
    for (const type of ["pointerdown", "click", "keydown", "wheel"]) {
      expect(heard(type), type).toHaveLength(1);
      expect(heard(type)[0]?.[2], type).toEqual({ capture: true, passive: true });
    }
    expect(heard("pointermove")).toHaveLength(0);
    expect(heard("mousemove")).toHaveLength(0);

    // Nothing a script dispatches is the student (and a test can dispatch nothing else)...
    window.dispatchEvent(new Event("pointerdown"));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    window.dispatchEvent(new WheelEvent("wheel"));
    expect(onInput).not.toHaveBeenCalled();
    // ...so the listener is handed what the browser would give it.
    const listener = heard("pointerdown")[0]?.[1] as (event: unknown) => void;
    listener({ type: "pointerdown", isTrusted: true });
    expect(onInput).toHaveBeenCalledTimes(1);
    listener({ type: "keydown", isTrusted: true, key: "a", repeat: true });
    listener({ type: "keydown", isTrusted: false, key: "a" });
    expect(onInput).toHaveBeenCalledTimes(1);

    stop();
    for (const type of ["pointerdown", "click", "keydown", "wheel"]) {
      expect(remove, type).toHaveBeenCalledWith(type, listener, { capture: true, passive: true });
    }
  } finally {
    add.mockRestore();
    remove.mockRestore();
  }
});

it("says the window started hidden only when the Rust side said so", () => {
  const api = createTauriApi();
  try {
    // No window facts at all (a browser tab): not hidden.
    Reflect.deleteProperty(window, "__PAGELAMP_WINDOW__");
    expect(api.startedHidden()).toBe(false);
    window.__PAGELAMP_WINDOW__ = Object.freeze({ backdrop: "none", hidden: false });
    expect(api.startedHidden()).toBe(false);
    // What window.rs writes for a start at login (`--hidden`).
    window.__PAGELAMP_WINDOW__ = Object.freeze({ backdrop: "none", hidden: true });
    expect(api.startedHidden()).toBe(true);
  } finally {
    Reflect.deleteProperty(window, "__PAGELAMP_WINDOW__");
  }
});
