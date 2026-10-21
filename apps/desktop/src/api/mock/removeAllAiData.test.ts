import { describe, expect, it } from "vitest";
import type { GenEvent } from "../ai";
import { createMockApi } from ".";

const NOW = new Date(2026, 8, 29, 10, 0); // Tuesday 2026-09-29
const COURSE = "canvas:canvas.demo.test/course/205"; // "proposals": week 4 has one material

const options = { latencyMs: 0, syncStepMs: 0, now: () => NOW, scenario: "proposals" as const };

describe("mock “Remove all AI data”, as the facade's store does it", () => {
  it("drops explanations, notes and plan drafts, and the AI settings they use", async () => {
    const baseline = (await createMockApi(options).removeAllAiData()).generations_removed;
    const api = createMockApi(options);
    await api.explainWeek(COURSE, 4, "ex-1", {}, () => {});
    await api.setAiOutputLanguage("course");
    await api.writeWeeklyNote("note-1", {}, () => {});
    await api.setPrepareWeeklyNoteOnMonday(true);
    await api.generateStudyPlan({}, "plan-1", () => {});
    await api.generateStudyPlan({}, "plan-2", () => {});
    const accepted = await api.acceptStudyPlan("plan-1");

    const report = await api.removeAllAiData();
    // The explanation, the note and both drafts, besides what the scenario's readings left.
    expect(report.generations_removed).toBe(baseline + 4);
    expect(await api.savedExplanations(COURSE, 4)).toEqual([]);
    expect(await api.aiOutputLanguage()).toBe("ui");
    expect(await api.weeklyNotes()).toEqual([]);
    expect((await api.weeklyNoteSettings()).prepare_on_monday).toBe(false);
    await expect(api.acceptStudyPlan("plan-2")).rejects.toMatchObject({ kind: "not_found" });
    // The plan in use stays, with its label, no longer linked to its draft.
    expect(await api.latestStudyPlan()).toEqual({ ...accepted, generation_id: null });
  });

  it("stops the runs going on, which then keep nothing", async () => {
    const api = createMockApi({ ...options, syncStepMs: 20 });
    const started = new Set<string>();
    const note = (id: string) => (e: GenEvent) => {
      if (e.type === "started") started.add(id);
    };
    const explaining = api.explainWeek(COURSE, 4, "ex-1", {}, note("ex-1"));
    const writing = api.writeWeeklyNote("note-1", {}, note("note-1"));
    const planning = api.generateStudyPlan({}, "plan-1", note("plan-1"));
    // Once all three are past the gate (their model asked).
    await new Promise<void>((resolve) => {
      const check = () => (started.size === 3 ? resolve() : setTimeout(check, 1));
      check();
    });
    await api.removeAllAiData();
    for (const run of [explaining, writing, planning]) {
      await expect(run).rejects.toMatchObject({ kind: "cancelled" });
    }
    expect(await api.savedExplanations(COURSE, 4)).toEqual([]);
    expect(await api.weeklyNotes()).toEqual([]);
    await expect(api.acceptStudyPlan("plan-1")).rejects.toMatchObject({ kind: "not_found" });
  });
});
