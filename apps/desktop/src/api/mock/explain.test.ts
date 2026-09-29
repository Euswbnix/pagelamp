import { describe, expect, it } from "vitest";
import { createMockApi } from ".";

const fast = { latencyMs: 0, syncStepMs: 0, scenario: "proposals" as const };
const COURSE = "canvas:canvas.demo.test/course/205"; // week 4 has a readable material

describe("mock weekly explanations", () => {
  it("explains a week with every paragraph cited, and keeps it", async () => {
    const api = createMockApi(fast);
    const types: string[] = [];
    const explanation = await api.explainWeek(COURSE, null, "ex-1", {}, (e) => types.push(e.type));
    expect(types).toEqual(["stage", "context", "started", "stage", "usage", "stage", "finished"]);
    expect(explanation.week).toBe(4);
    for (const section of explanation.sections) {
      for (const paragraph of section.paragraphs)
        expect(paragraph.citations.length).toBeGreaterThan(0);
    }
    expect(explanation.check_questions.length).toBeGreaterThan(0);
    expect(explanation.sharing_reminder).toBe(true);
    expect((await api.savedExplanations(COURSE, 4)).map((e) => e.meta.generation_id)).toEqual([
      "ex-1",
    ]);
    const again = await api.explainWeek(COURSE, 4, "ex-2", {}, () => {});
    expect(again.sharing_reminder).toBe(false);
    expect((await api.savedExplanations(COURSE, null)).map((e) => e.meta.generation_id)).toEqual([
      "ex-2",
      "ex-1",
    ]);
  });

  it("is blocked with the course's reason, and stops when asked", async () => {
    const demo = createMockApi({ ...fast, scenario: "demo" });
    await expect(
      demo.explainWeek("folder:demo-courses/course/DEMO310", null, "g", {}, () => {}),
    ).rejects.toMatchObject({ kind: "blocked", blocked: "course_policy_prohibited" });

    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const running = api.explainWeek(COURSE, null, "ex-3", {}, () => {});
    await new Promise((resolve) => setTimeout(resolve, 5));
    await api.cancelGeneration("ex-3");
    await expect(running).rejects.toMatchObject({ kind: "cancelled" });
  });

  it("keeps the output language", async () => {
    const api = createMockApi(fast);
    expect(await api.aiOutputLanguage()).toBe("ui");
    await api.setAiOutputLanguage("course");
    expect(await api.aiOutputLanguage()).toBe("course");
  });
});
