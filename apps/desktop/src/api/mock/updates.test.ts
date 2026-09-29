import { describe, expect, it } from "vitest";
import type { UpdateEvent } from "../client";
import { createMockApi } from ".";
import { MOCK_APP_VERSION, MOCK_UPDATE_VERSION } from "./fixtures";

const fast = { latencyMs: 0, syncStepMs: 0 };

describe("mock updates", () => {
  it("uses beta by default for a pre-release build, and remembers a chosen channel", async () => {
    const api = createMockApi(fast);
    expect(await api.updatePrefs()).toEqual({
      auto_check: true,
      channel: null,
      effective_channel: "beta",
    });
    await api.setUpdatePrefs({ auto_check: false, channel: "stable" });
    expect(await api.updatePrefs()).toMatchObject({
      auto_check: false,
      effective_channel: "stable",
    });
  });

  it("shows upgraders 'What's new' first and only then makes a check due", async () => {
    const api = createMockApi({ ...fast, scenario: "upgrader" });
    const first = await api.startupTasks();
    expect(first.whats_new?.topics).toEqual(["update_check", "course_weeks"]);
    expect(first.update_check_due).toBe(false);
    await api.acknowledgeWhatsNew();
    const after = await api.startupTasks();
    expect(after.whats_new).toBeNull();
    expect(after.update_check_due).toBe(true);
  });

  it("makes no check due on a fresh install until onboarding disclosed it", async () => {
    const api = createMockApi({ ...fast, scenario: "empty" });
    expect((await api.startupTasks()).update_check_due).toBe(false);
    await api.acknowledgeUpdateDisclosure();
    // Disclosed now, but the mock's last check was 2 h ago: the next one is due after a day.
    expect((await api.startupTasks()).update_check_due).toBe(false);
    expect(await api.lastUpdateCheck()).toMatchObject({ outcome: "up_to_date" });
  });

  it("offers an update, records the check, and installs it with progress", async () => {
    const api = createMockApi({ ...fast, scenario: "update-available" });
    expect(await api.lastUpdateCheck()).toBeNull();
    expect((await api.startupTasks()).update_check_due).toBe(true);
    const update = await api.checkForUpdate();
    expect(update?.version).toBe(MOCK_UPDATE_VERSION);
    expect(await api.lastUpdateCheck()).toMatchObject({
      outcome: "available",
      version: MOCK_UPDATE_VERSION,
      channel: "beta",
    });
    const events: UpdateEvent[] = [];
    await api.installUpdate((event) => events.push(event));
    expect(events.map((e) => e.type)).toEqual([
      "download_started",
      "progress",
      "progress",
      "progress",
      "progress",
      "installing",
      "restarting",
    ]);
  });

  it("reports up to date otherwise", async () => {
    const api = createMockApi(fast);
    expect(await api.checkForUpdate()).toBeNull();
    expect(await api.lastUpdateCheck()).toMatchObject({ outcome: "up_to_date" });
    expect((await api.updaterStatus()).current_version).toBe(MOCK_APP_VERSION);
  });

  it("only links to the download on deb/rpm installs", async () => {
    const api = createMockApi({ ...fast, scenario: "deb" });
    expect(await api.updaterStatus()).toMatchObject({
      install: "download_only",
      platform: "linux",
    });
    expect((await api.checkForUpdate())?.download_url).toContain(`v${MOCK_UPDATE_VERSION}`);
    await expect(api.installUpdate(() => {})).rejects.toMatchObject({ kind: "invalid" });
  });
});
