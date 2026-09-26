import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useToday } from "./useToday";

afterEach(() => vi.useRealTimers());

describe("useToday", () => {
  it("moves to the next date at local midnight", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 8, 25, 23, 59, 50));
    const { result } = renderHook(() => useToday());
    expect(result.current).toBe("2026-09-25");

    act(() => {
      vi.advanceTimersByTime(15_000);
    });
    expect(result.current).toBe("2026-09-26");
  });
});
