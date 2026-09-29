import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { CALENDAR_UI, calendarUiEnabled } from "./availability";

// Vitest runs from apps/desktop (vite.config.ts), where the .env files live.
const desktopDir = process.cwd();

describe("the F3 switch", () => {
  it("is off in a production build and on in mock mode", () => {
    expect(calendarUiEnabled({ MODE: "production", PROD: true })).toBe(false);
    expect(calendarUiEnabled({ MODE: "development", VITE_API: "tauri" })).toBe(false);
    expect(calendarUiEnabled({ MODE: "mock", VITE_API: "mock" })).toBe(true);
    expect(calendarUiEnabled({ MODE: "production", VITE_PAGELAMP_CALENDAR_UI: "1" })).toBe(true);
    expect(CALENDAR_UI).toBe(true);
  });

  it("isn't turned on by any env file a real build reads", () => {
    const envFiles = readdirSync(desktopDir).filter((f) => f.startsWith(".env"));
    expect(envFiles).toContain(".env.mock");
    for (const file of envFiles.filter((f) => f !== ".env.mock")) {
      const text = readFileSync(join(desktopDir, file), "utf8");
      expect({ file, on: /^\s*VITE_PAGELAMP_CALENDAR_UI\s*=\s*1/m.test(text) }).toEqual({
        file,
        on: false,
      });
    }
  });
});
