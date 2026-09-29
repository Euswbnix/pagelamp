import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUiStore } from "@/stores/ui";

const setTheme = vi.fn(async (_theme: "light" | "dark" | null) => {});
vi.mock("@tauri-apps/api/app", () => ({
  setTheme: (theme: "light" | "dark" | null) => setTheme(theme),
}));
vi.mock("@/api", async (original) => ({ ...(await original<object>()), API_MODE: "tauri" }));

const { useAppearance } = await import("./useAppearance");
const { recordStartupScheme } = await import("@/lib/appearance");

function systemIsDark(dark: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: dark && query.includes("dark"),
    media: query,
    addEventListener: () => {},
    removeEventListener: () => {},
  })) as unknown as typeof window.matchMedia;
  recordStartupScheme();
}

beforeEach(() => {
  setTheme.mockClear();
});

afterEach(() => {
  delete document.documentElement.dataset.platform;
});

describe("useAppearance: the native theme", () => {
  it('leaves the window alone for "system" until the student picks light or dark', () => {
    systemIsDark(true);
    useUiStore.setState({ theme: "system" });
    renderHook(() => useAppearance());
    expect(setTheme).not.toHaveBeenCalled();

    act(() => useUiStore.setState({ theme: "light" }));
    expect(setTheme).toHaveBeenLastCalledWith("light");
  });

  it("on Linux, goes back to the system scheme seen at startup, never null", () => {
    document.documentElement.dataset.platform = "linux";
    systemIsDark(true);
    useUiStore.setState({ theme: "system" });
    renderHook(() => useAppearance());
    act(() => useUiStore.setState({ theme: "light" }));
    act(() => useUiStore.setState({ theme: "system" }));
    expect(setTheme).toHaveBeenLastCalledWith("dark");
  });

  it("on Windows and macOS, goes back to following the system (null)", () => {
    document.documentElement.dataset.platform = "windows";
    systemIsDark(false);
    useUiStore.setState({ theme: "dark" });
    renderHook(() => useAppearance());
    expect(setTheme).toHaveBeenLastCalledWith("dark");
    act(() => useUiStore.setState({ theme: "system" }));
    expect(setTheme).toHaveBeenLastCalledWith(null);
  });
});
