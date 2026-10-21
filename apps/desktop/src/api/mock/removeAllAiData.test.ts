import { describe, expect, it } from "vitest";
import { createMockApi } from ".";

const NOW = new Date(2026, 8, 29, 10, 0); // Tuesday 2026-09-29
const COURSE = "canvas:canvas.demo.test/course/205"; // "proposals": week 4 has one material

describe("mock “Remove all AI data”, as the facade's store does it", () => {
  it("drops explanations, notes and plan drafts, and the AI settings they use", async () => {
    const api = createMockApi({
      latencyMs: 0,
      syncStepMs: 0,
      now: () => NOW,
      scenario: "proposals",
    });
    await api.explainWeek(COURSE, 4, "ex-1", {}, () => {});
    await api.setAiOutputLanguage("course");
    await api.writeWeeklyNote("note-1", {}, () => {});
    await api.setPrepareWeeklyNoteOnMonday(true);
    await api.generateStudyPlan({}, "plan-1", () => {});
    await api.generateStudyPlan({}, "plan-2", () => {});
    const accepted = await api.acceptStudyPlan("plan-1");

    const report = await api.removeAllAiData();
    // The explanation, the note, both drafts and the scenario's one AI calendar proposal.
    expect(report.generations_removed).toBe(5);
    expect(await api.savedExplanations(COURSE, 4)).toEqual([]);
    expect(await api.aiOutputLanguage()).toBe("ui");
    expect(await api.weeklyNotes()).toEqual([]);
    expect((await api.weeklyNoteSettings()).prepare_on_monday).toBe(false);
    await expect(api.acceptStudyPlan("plan-2")).rejects.toMatchObject({ kind: "not_found" });
    // The plan in use stays, with its label, no longer linked to its draft.
    expect(await api.latestStudyPlan()).toEqual({ ...accepted, generation_id: null });
  });
});
