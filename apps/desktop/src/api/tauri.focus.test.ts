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
