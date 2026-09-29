import { describe, expect, it } from "vitest";
import { AI_SETUP_ENABLED, mockScreensEnabled } from "./features";

describe("mock-only screens", () => {
  it("show in the mock, not in a real build, and not with ?shipped", () => {
    expect(mockScreensEnabled({ VITE_API: "mock" }, "")).toBe(true);
    expect(mockScreensEnabled({ VITE_API: "tauri" }, "")).toBe(false);
    expect(mockScreensEnabled({ MODE: "production", PROD: true }, "?scenario=demo")).toBe(false);
    expect(mockScreensEnabled({ VITE_API: "mock" }, "?shipped")).toBe(false);
    expect(mockScreensEnabled({ VITE_API: "mock", VITE_PAGELAMP_SHIPPED: "1" }, "")).toBe(false);
    // Tests run against the mock without ?shipped, so the screens are tested.
    expect(AI_SETUP_ENABLED).toBe(true);
  });
});
