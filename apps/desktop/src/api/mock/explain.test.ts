import { describe, expect, it } from "vitest";
import type { GenEvent } from "../ai";
import type { MaterialView } from "../types";
import { createMockApi } from ".";
import { selectMaterials } from "./explain";

const fast = { latencyMs: 0, syncStepMs: 0, scenario: "proposals" as const };
const COURSE = "canvas:canvas.demo.test/course/205"; // week 4 has a readable material

describe("mock weekly explanations", () => {
  it("explains a week with every paragraph cited, and keeps it", async () => {
    const api = createMockApi(fast);
    const types: string[] = [];
    const explanation = await api.explainWeek(COURSE, null, "ex-1", {}, (e) => types.push(e.type));
    expect(types).toEqual([
      "stage",
      "context",
      "started",
      "stage",
      "usage",
      "stage",
      "notice",
      "finished",
    ]);
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

describe("mock weekly explanations: the question (b) reminder, as a reading's", () => {
  /** Each event's type, a notice by its code. */
  const kinds = (events: GenEvent[]) => events.map((e) => (e.type === "notice" ? e.code : e.type));
  /** Resolves once the run's events include `type`. */
  function once(events: GenEvent[], type: GenEvent["type"]) {
    return new Promise<void>((resolve) => {
      const check = () => (events.some((e) => e.type === type) ? resolve() : setTimeout(check, 1));
      check();
    });
  }

  it("says it before a run stopped after it started ends; then claimed", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const events: GenEvent[] = [];
    const run = api.explainWeek(COURSE, 4, "ex-1", {}, (e) => events.push(e));
    await once(events, "started");
    await api.cancelGeneration("ex-1");
    await expect(run).rejects.toMatchObject({ kind: "cancelled" });
    expect(kinds(events).slice(-2)).toEqual(["material_sharing_reminder", "finished"]);
    expect(events.at(-1)).toEqual({ type: "finished", ok: false });
    const again: GenEvent[] = [];
    const next = await api.explainWeek(COURSE, 4, "ex-2", {}, (e) => again.push(e));
    expect(kinds(again)).not.toContain("material_sharing_reminder");
    expect(next.sharing_reminder).toBe(false);
  });

  it("says nothing of a run stopped before it started, and claims nothing", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const events: GenEvent[] = [];
    const run = api.explainWeek(COURSE, 4, "ex-1", {}, (e) => events.push(e));
    await once(events, "stage");
    await api.cancelGeneration("ex-1");
    await expect(run).rejects.toMatchObject({ kind: "cancelled" });
    expect(kinds(events)).not.toContain("started");
    expect(kinds(events)).not.toContain("material_sharing_reminder");
    expect((await api.explainWeek(COURSE, 4, "ex-2", {}, () => {})).sharing_reminder).toBe(true);
  });

  it("says it, notice first, on the success that claims it", async () => {
    const api = createMockApi(fast);
    const events: GenEvent[] = [];
    await api.explainWeek(COURSE, 4, "ex-1", {}, (e) => events.push(e));
    expect(kinds(events).slice(-2)).toEqual(["material_sharing_reminder", "finished"]);
  });

  it("follows the answer stored when the run ends", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    // Answered "allowed" while it runs: nothing to remind of, and nothing claimed.
    const events: GenEvent[] = [];
    const run = api.explainWeek(COURSE, 4, "ex-1", {}, (e) => events.push(e));
    await once(events, "started");
    await api.setCourseMaterialSharing(COURSE, "allowed");
    expect((await run).sharing_reminder).toBe(false);
    expect(kinds(events)).not.toContain("material_sharing_reminder");
    // Back to "not sure" while another runs: reminded then.
    const second = api.explainWeek(COURSE, 4, "ex-2", {}, () => {});
    await api.setCourseMaterialSharing(COURSE, "not_sure");
    expect((await second).sharing_reminder).toBe(true);
  });

  it("is one claim a course, whichever run sent the text, and all AI data forgets it", async () => {
    const api = createMockApi(fast);
    expect((await api.explainWeek(COURSE, 4, "ex-1", {}, () => {})).sharing_reminder).toBe(true);
    // The course's syllabus reading after it: already said.
    const reading: GenEvent[] = [];
    const proposal = await api.readCourseCalendar(
      COURSE,
      "gen-1",
      { override_budget: false },
      (e) => reading.push(e),
    );
    expect(proposal.sharing_reminder).toBe(false);
    expect(kinds(reading)).not.toContain("material_sharing_reminder");
    // Forgotten with all AI data: the next cloud run asks again.
    await api.removeAllAiData();
    await api.addModelProvider("openai", null, "sk-demo-000000000000007Qx2");
    const [facts] = await api.modelDisclosure({ kind: "provider", provider_id: "openai" }, false);
    await api.acknowledgeModelDisclosure(
      { kind: "provider", provider_id: "openai" },
      false,
      facts.version,
    );
    await api.setFeatureModel("weekly_explanation", {
      backend: { kind: "provider", provider_id: "openai" },
      model: "gpt-6-luna",
      effort: "lowest",
    });
    expect((await api.explainWeek(COURSE, 4, "ex-2", {}, () => {})).sharing_reminder).toBe(true);
  });
});

describe("mock weekly explanations: priced as the facade bills them", () => {
  it("is free here, has no price in the cloud through a server here, is priced with a key", async () => {
    const api = createMockApi(fast);
    const cost = async (id: string) =>
      (await api.explainWeek(COURSE, 4, id, {}, () => {})).meta.est_cost_micro_usd;
    expect(await cost("key")).toBe(3_100);
    const added = await api.addModelProvider("ollama", null, null);
    const ollama = { kind: "provider", provider_id: added.provider_id } as const;
    for (const here of [true, false]) {
      const [facts] = await api.modelDisclosure(ollama, here);
      await api.acknowledgeModelDisclosure(ollama, here, facts.version);
    }
    const use = (model: string) =>
      api.setFeatureModel("weekly_explanation", { backend: ollama, model, effort: "lowest" });
    await use("qwen3.5:9b");
    expect(await cost("here")).toBe(0);
    await use("gpt-oss:120b-cloud");
    await api.acknowledgeUnpricedModel(ollama, "gpt-oss:120b-cloud");
    expect(await cost("cloud")).toBeNull();
  });
});

describe("the mock's left-out list, in the facade's order", () => {
  const material = (id: string, title: string, text: "ok" | "not_downloaded") =>
    ({ id, title, text_status: text }) as MaterialView;

  it("says no text for a graded-looking material without text, include or not", () => {
    const week = [
      material("quiz", "Quiz 3", "not_downloaded"),
      material("ps", "Problem set 4", "ok"),
      material("slides", "Week 4 slides", "ok"),
    ];
    for (const include of [[], ["quiz"]]) {
      const { read, leftOut } = selectMaterials(week, include);
      expect(leftOut.find((m) => m.material_id === "quiz")?.reason).toBe("no_text");
      expect(read.map((m) => m.id)).not.toContain("quiz");
    }
    // The readable graded-looking one comes back with include.
    expect(selectMaterials(week, []).leftOut.find((m) => m.material_id === "ps")?.reason).toBe(
      "looks_like_assessment",
    );
    expect(selectMaterials(week, ["ps"]).read.map((m) => m.id)).toEqual(["ps", "slides"]);
  });
});
