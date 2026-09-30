import { describe, expect, it } from "vitest";
import type { ApiError } from "../errors";
import { createMockApi } from ".";

const MONDAY = new Date(2026, 8, 28, 10, 0); // Monday 2026-09-28
const TUESDAY = new Date(2026, 8, 29, 10, 0);
const fast = { latencyMs: 0, syncStepMs: 0 };

describe("mock weekly note", () => {
  it("writes a note from structure with the run's events, keeps the last 5, deletes one", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-key", now: () => TUESDAY });
    const types: string[] = [];
    const note = await api.writeWeeklyNote("n-1", {}, (e) => types.push(e.type));
    expect(types).toEqual(["stage", "context", "started", "stage", "usage", "stage", "finished"]);
    expect(note.automatic).toBe(false);
    expect(note.week_of).toBe("2026-09-28");
    expect(note.focus.length).toBeLessThanOrEqual(3);
    expect(note.meta.feature).toBe("weekly_note");
    // Structure only: no material text is read.
    expect(note.meta.context?.materials_included).toBe(0);
    for (let i = 2; i <= 6; i++) await api.writeWeeklyNote(`n-${i}`, {}, () => {});
    const kept = (await api.weeklyNotes()).map((n) => n.meta.generation_id);
    expect(kept).toEqual(["n-6", "n-5", "n-4", "n-3", "n-2"]);
    await api.deleteWeeklyNote("n-4");
    expect((await api.weeklyNotes()).map((n) => n.meta.generation_id)).not.toContain("n-4");
    await expect(api.deleteWeeklyNote("n-4")).rejects.toMatchObject({ kind: "not_found" });
  });

  it("has nothing to write about without an active course, a deadline or a plan item", async () => {
    const api = createMockApi({ ...fast, scenario: "empty" });
    const error = (await api.writeWeeklyNote("n-1", {}, () => {}).catch((e) => e)) as ApiError;
    expect(error.kind).toBe("invalid");
    expect(error.message).toMatch(/nothing to write about/);
  });

  it("lets Monday's note be prepared only with an API key or a model on this computer", async () => {
    const key = createMockApi({ ...fast, scenario: "ai-key" });
    expect(await key.weeklyNoteSettings()).toEqual({
      prepare_on_monday: false,
      prepare_on_monday_allowed: true,
    });
    expect((await key.setPrepareWeeklyNoteOnMonday(true)).prepare_on_monday).toBe(true);
    const local = createMockApi({ ...fast, scenario: "ai-local" });
    expect((await local.weeklyNoteSettings()).prepare_on_monday_allowed).toBe(true);
    const plan = createMockApi({ ...fast, scenario: "codex-plus" });
    expect((await plan.weeklyNoteSettings()).prepare_on_monday_allowed).toBe(false);
    await expect(plan.setPrepareWeeklyNoteOnMonday(true)).rejects.toMatchObject({
      kind: "invalid",
    });
    // Turning it off always works.
    expect((await plan.setPrepareWeeklyNoteOnMonday(false)).prepare_on_monday).toBe(false);
  });

  it("asks for Monday's note once a Monday: the try counts as it starts", async () => {
    let clock = MONDAY;
    const api = createMockApi({ ...fast, scenario: "ai-key", now: () => clock });
    await api.setPrepareWeeklyNoteOnMonday(true);
    expect((await api.startupTasks()).prepare_weekly_note).toBe(true);
    const note = await api.writeWeeklyNote("auto-1", { automatic: true }, () => {});
    expect(note.automatic).toBe(true);
    expect((await api.startupTasks()).prepare_weekly_note).toBe(false);
    // A second automatic run isn't due any more.
    await expect(
      api.writeWeeklyNote("auto-2", { automatic: true }, () => {}),
    ).rejects.toMatchObject({ kind: "invalid" });
    clock = TUESDAY;
    expect((await api.startupTasks()).prepare_weekly_note).toBe(false);
    clock = new Date(2026, 9, 5, 9, 0); // the next Monday
    expect((await api.startupTasks()).prepare_weekly_note).toBe(true);
  });

  it("lists a note run in activity while it runs, so an install waits for it", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 20, scenario: "ai-key" });
    const run = api.writeWeeklyNote("n-1", {}, () => {});
    expect((await api.activity()).items).toMatchObject([
      { kind: "generation", generation_id: "n-1" },
    ]);
    await run;
    expect((await api.activity()).items).toEqual([]);
  });

  it("stops a run with Stop, and saves nothing", async () => {
    const api = createMockApi({ latencyMs: 0, syncStepMs: 20, scenario: "ai-key" });
    const run = api.writeWeeklyNote("n-1", {}, () => {}).catch((e) => e as ApiError);
    await api.cancelGeneration("n-1");
    expect(((await run) as ApiError).kind).toBe("cancelled");
    expect(await api.weeklyNotes()).toEqual([]);
  });
});
