import { describe, expect, it } from "vitest";
import { showsMacQuarantineHint } from "./platform";

const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
const WINDOWS = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

describe("showsMacQuarantineHint", () => {
  it("only shows in production macOS builds of the desktop app", () => {
    expect(showsMacQuarantineHint({ apiMode: "tauri", production: true, userAgent: MAC })).toBe(
      true,
    );
    expect(showsMacQuarantineHint({ apiMode: "tauri", production: false, userAgent: MAC })).toBe(
      false,
    );
    expect(showsMacQuarantineHint({ apiMode: "mock", production: true, userAgent: MAC })).toBe(
      false,
    );
    expect(showsMacQuarantineHint({ apiMode: "tauri", production: true, userAgent: WINDOWS })).toBe(
      false,
    );
  });
});
