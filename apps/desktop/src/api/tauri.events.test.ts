import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const desktopDir = process.cwd();

it("listens for the ticker's events by the names the shell emits them under", () => {
  const shell = readFileSync(join(desktopDir, "src-tauri/src/reminders.rs"), "utf8");
  const page = readFileSync(join(desktopDir, "src/api/tauri.ts"), "utf8");
  for (const constant of ["CHECK_EVENT", "STARTUP_CHECK_EVENT"]) {
    const name = new RegExp(`pub const ${constant}: &str = "([^"]+)";`).exec(shell)?.[1];
    expect(name, constant).toBeTruthy();
    expect(page).toContain(`listen("${name}"`);
  }
});
