import { describe, expect, it } from "vitest";
import { materialSharing } from "../ai";
import { createMockApi } from ".";

const fast = { latencyMs: 0, syncStepMs: 0 };
const openai = { kind: "provider", provider_id: "openai" } as const;

async function courseId(api: ReturnType<typeof createMockApi>, code: string) {
  const courses = await api.listCourses();
  const found = courses.find((c) => c.course.code === code);
  if (!found) throw new Error(`no ${code}`);
  return found.course;
}

describe("mock AI setup", () => {
  it("starts with nothing set up, the default budget, and no model to estimate with", async () => {
    const api = createMockApi(fast);
    const status = await api.aiStatus();
    expect(status.backends).toEqual([]);
    expect(status.features.map((f) => f.choice)).toEqual([null, null, null, null]);
    expect(status.budget).toEqual({
      monthly_micro_usd: 5_000_000,
      spent_micro_usd: 0,
      warn_at_percent: 80,
    });
    const estimate = await api.estimateGeneration({ feature: "weekly_note" });
    expect(estimate.would_block).toBe("no_model_chosen");
  });

  it("keeps only the key's last 4 characters and asks for the disclosure first", async () => {
    const api = createMockApi(fast);
    const key = "sk-demo-secret-value-9f3A";
    const record = await api.addModelProvider("openai", null, key);
    expect(record).toMatchObject({ provider_id: "openai", key_last4: "9f3A", on_device: false });
    const status = await api.aiStatus();
    expect(JSON.stringify(status)).not.toContain("secret-value");
    const [backend] = status.backends;
    expect(backend).toMatchObject({ state: "needs_disclosure", disclosure_acknowledged: null });
    await expect(
      api.acknowledgeAiDisclosure(openai, (backend?.disclosure.version ?? 0) + 1),
    ).rejects.toMatchObject({ kind: "invalid" });
    await api.acknowledgeAiDisclosure(openai, backend?.disclosure.version ?? 0);
    expect((await api.aiStatus()).backends[0]?.state).toBe("ready");
  });

  it("refuses coding-plan keys, rejected keys and plain http to another computer", async () => {
    const api = createMockApi(fast);
    await expect(
      api.addModelProvider("custom", "https://api.z.ai/api/coding/paas/v4", "demo-key"),
    ).rejects.toMatchObject({ kind: "blocked", blocked: "coding_plan_key" });
    await expect(api.addModelProvider("openai", null, "sk-sp-demo")).rejects.toMatchObject({
      blocked: "coding_plan_key",
    });
    await expect(api.addModelProvider("openai", null, "sk-bad-key")).rejects.toMatchObject({
      kind: "model",
      model_error: "auth_rejected",
    });
    await expect(
      api.addModelProvider("custom", "http://llm.example.edu/v1", "demo-key"),
    ).rejects.toMatchObject({ kind: "invalid" });
    await expect(api.addModelProvider("openai", null, "  ")).rejects.toMatchObject({
      kind: "invalid",
    });
    await expect(api.addModelProvider("no-such-preset", null, "k")).rejects.toMatchObject({
      kind: "not_found",
    });
    // A local server needs no key, and http is fine on this computer.
    const local = await api.addModelProvider("ollama", "http://127.0.0.1:11434", null);
    expect(local).toMatchObject({ on_device: true, key_last4: null });
    // One provider per preset.
    await expect(
      api.addModelProvider("ollama", "http://127.0.0.1:11434", null),
    ).rejects.toMatchObject({ kind: "invalid" });
  });

  it("estimates an upper bound and applies the course gates, question (b) included", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-key" });
    const demo101 = await courseId(api, "DEMO101");
    const ok = await api.estimateGeneration({ feature: "weekly_explanation", course: demo101.id });
    expect(ok.would_block).toBeNull();
    expect(ok.price_known).toBe(true);
    expect(ok.micro_usd_upper).toBeGreaterThan(0);
    expect(ok.reasoning_allowance).toBe(2_000); // effort "low"

    const demo205 = await courseId(api, "DEMO205");
    expect(materialSharing(demo205)).toBe("not_allowed");
    const shared = await api.estimateGeneration({
      feature: "weekly_explanation",
      course: demo205.id,
    });
    // A blocked estimate carries no amount and no tokens.
    expect(shared).toMatchObject({
      would_block: "material_sharing_not_allowed",
      micro_usd_upper: null,
      input_tokens: 0,
    });

    const demo310 = await courseId(api, "DEMO310");
    const prohibited = await api.estimateGeneration({
      feature: "weekly_explanation",
      course: demo310.id,
    });
    expect(prohibited.would_block).toBe("course_policy_prohibited");

    // Answering "allowed" lifts the question-(b) block.
    await api.setCourseMaterialSharing(demo205.id, "allowed");
    expect(materialSharing(await courseId(api, "DEMO205"))).toBe("allowed");
    const after = await api.estimateGeneration({
      feature: "weekly_explanation",
      course: demo205.id,
    });
    expect(after.would_block).toBeNull();
  });

  it("prices what Include sends, so a budget can stop only the run that includes", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-key" });
    const demo101 = await courseId(api, "DEMO101");
    const { materials } = await api.weekMaterials(demo101.id);
    const assignment = materials.find((m) => m.title.startsWith("Assignment 4"));
    if (!assignment) throw new Error("week 4 has Assignment 4");
    const plain = { feature: "weekly_explanation", course: demo101.id } as const;
    const withAssignment = { ...plain, include: [assignment.id] };
    const base = await api.estimateGeneration(plain);
    // An id the week doesn't have is sent nowhere, so it costs nothing (like the facade).
    const unknown = await api.estimateGeneration({ ...plain, include: ["no-such-material"] });
    expect(unknown.input_tokens).toBe(base.input_tokens);
    const included = await api.estimateGeneration(withAssignment);
    expect(included.input_tokens).toBeGreaterThan(base.input_tokens);
    const [low, high] = [base.micro_usd_upper ?? 0, included.micro_usd_upper ?? 0];
    expect(high).toBeGreaterThan(low + 1);
    const { spent_micro_usd } = (await api.aiStatus()).budget;
    await api.setMonthlyBudget(spent_micro_usd + Math.floor((low + high) / 2));
    expect((await api.estimateGeneration(plain)).would_block).toBeNull();
    expect((await api.estimateGeneration(withAssignment)).would_block).toBe("budget_reached");
    // The run with the same include stops the same way, and the plain one doesn't.
    const options = { include: [assignment.id], override_budget: false, ui_language: "en" };
    await expect(
      api.explainWeek(demo101.id, null, "explain-include", options, () => {}),
    ).rejects.toMatchObject({ kind: "blocked", blocked: "budget_reached" });
  });

  it("prices only what Include lifts: a material read anyway or not graded-looking adds nothing", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-key" });
    const demo101 = await courseId(api, "DEMO101");
    const { materials } = await api.weekMaterials(demo101.id);
    const slides = materials.find((m) => m.title.startsWith("Week 4 slides"));
    const practice = materials.find((m) => m.title.startsWith("Week 4 practice"));
    if (!slides || !practice) throw new Error("week 4 has its slides and practice questions");
    const plain = { feature: "weekly_explanation", course: demo101.id } as const;
    const base = await api.estimateGeneration(plain);
    for (const id of [slides.id, practice.id]) {
      const priced = await api.estimateGeneration({ ...plain, include: [id] });
      expect(priced.input_tokens).toBe(base.input_tokens);
      expect(priced.micro_usd_upper).toBe(base.micro_usd_upper);
    }
  });

  it("blocks a run over the budget until the cap is raised or removed", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-budget" });
    expect((await api.aiStatus()).budget.spent_micro_usd).toBe(4_960_000);
    const demo101 = await courseId(api, "DEMO101");
    const req = { feature: "weekly_explanation", course: demo101.id } as const;
    // Over budget keeps its amount: the student decides on the per-run override with it.
    const over = await api.estimateGeneration(req);
    expect(over.would_block).toBe("budget_reached");
    expect(over.micro_usd_upper).toBeGreaterThan(0);
    await api.setMonthlyBudget(null);
    expect((await api.estimateGeneration(req)).would_block).toBeNull();
    await expect(api.setMonthlyBudget(-1)).rejects.toMatchObject({ kind: "invalid" });
  });

  it("asks once before using a model with no price", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-unpriced" });
    const demo101 = await courseId(api, "DEMO101");
    const req = { feature: "weekly_explanation", course: demo101.id } as const;
    const first = await api.estimateGeneration(req);
    expect(first).toMatchObject({
      price_known: false,
      micro_usd_upper: null,
      would_block: "price_unknown_not_acknowledged",
    });
    expect(first.input_tokens).toBeGreaterThan(0);
    await api.acknowledgeUnpricedModel(openai, "gpt-6-preview-0929");
    expect((await api.estimateGeneration(req)).would_block).toBeNull();
  });

  it("costs nothing on this computer, where question (b) doesn't apply", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-local" });
    const [backend] = (await api.aiStatus()).backends;
    expect(backend).toMatchObject({ kind: "local", state: "ready" });
    expect(backend?.disclosure).toMatchObject({ on_device: true, cost: "free_on_device" });
    const demo205 = await courseId(api, "DEMO205");
    const estimate = await api.estimateGeneration({
      feature: "weekly_explanation",
      course: demo205.id,
    });
    expect(estimate).toMatchObject({ micro_usd_upper: 0, would_block: null });
    const models = await api.listModels({ kind: "provider", provider_id: "ollama" });
    expect(models.filter((m) => !m.on_device).map((m) => m.id)).toEqual(["gpt-oss:120b-cloud"]);
  });

  it("asks again when the disclosure facts changed", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-disclosure-changed" });
    const [backend] = (await api.aiStatus()).backends;
    expect(backend).toMatchObject({ state: "needs_disclosure", problems: ["disclosure_changed"] });
    const estimate = await api.estimateGeneration({ feature: "weekly_note" });
    // Fully estimated: only the acknowledgement stops the run.
    expect(estimate).toMatchObject({
      would_block: "disclosure_not_acknowledged",
      price_known: true,
    });
    expect(estimate.input_tokens).toBeGreaterThan(0);
    expect(estimate.micro_usd_upper).toBeGreaterThan(0);
  });

  it("reports model errors with their codes", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-errors" });
    const [backend] = (await api.aiStatus()).backends;
    if (!backend) throw new Error("no backend");
    await expect(api.listModels(backend.backend)).rejects.toMatchObject({
      kind: "model",
      model_error: "network",
    });
    // Like the facade: a model error in "Test" is a failed probe, not a thrown error.
    expect(await api.testModel(backend.backend, "demo-model-large")).toMatchObject({
      ok: false,
      error: "rate_limited",
    });
    await expect(
      api.testModel({ kind: "provider", provider_id: "nope" }, "m"),
    ).rejects.toMatchObject({ kind: "not_found" });
  });

  it("sums usage per month and forgets everything on remove all", async () => {
    const now = () => new Date("2026-10-14T12:00:00Z");
    const api = createMockApi({ ...fast, now, scenario: "ai-key" });
    const month = await api.usageSummary(null);
    expect(month).toMatchObject({ month: "2026-10-01", total_micro_usd: 2_000_000 });
    expect((await api.usageSummary("2026-09-01")).rows).toHaveLength(2);
    expect((await api.usageSummary("2026-08-01")).rows).toEqual([]);
    await api.removeAllAiData();
    expect((await api.aiStatus()).backends).toEqual([]);
    expect((await api.usageSummary(null)).rows).toEqual([]);
  });

  it("keeps a provider whose key the keychain won't delete, like the facade", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-keychain-locked" });
    const before = await api.aiStatus();
    // One removal: nothing changes.
    await expect(api.removeModelProvider("openai")).rejects.toMatchObject({
      kind: "internal",
      message: expect.stringContaining("under `llm:openai`. Nothing was removed"),
    });
    expect(await api.aiStatus()).toEqual(before);
    // Remove all: the rest goes, that provider and its key stay, and it says so.
    await expect(api.removeAllAiData()).rejects.toMatchObject({
      kind: "internal",
      message:
        "Removed 1 provider(s), 0 usage record(s) and 0 generated text(s), but the keychain didn't let PageLamp delete the API key of openai: it is still there (`llm:openai`), and so is that provider. Run this again when the keychain can be reached.",
    });
    const after = await api.aiStatus();
    expect(after.providers.map((p) => p.provider_id)).toEqual(["openai"]);
    expect(after.features.map((f) => f.choice)).toEqual([null, null, null, null]);
    // Its settings went too: its note is to be read again.
    expect(after.backends[0]).toMatchObject({
      state: "needs_disclosure",
      disclosure_acknowledged: null,
    });
  });

  it("removing a provider clears the features that used it", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-key" });
    await api.removeModelProvider("openai");
    const status = await api.aiStatus();
    expect(status.backends).toEqual([]);
    expect(status.features.every((f) => f.choice === null)).toBe(true);
  });
});

describe("mock: a server on this computer that may run a model in the cloud", () => {
  const OLLAMA = { kind: "provider", provider_id: "ollama" } as const;

  it("adds a custom server here without a key, and asks one of a custom server elsewhere", async () => {
    const api = createMockApi(fast);
    await expect(
      api.addModelProvider("custom", "https://llm.example.edu/v1", null),
    ).rejects.toMatchObject({
      kind: "invalid",
    });
    const here = await api.addModelProvider("custom", "http://localhost:8080/v1", null);
    const status = (await api.aiStatus()).backends.find((b) => b.label === here.label);
    expect(status?.problems).not.toContain("key_missing");
    // Its own note keeps what it's sent here; its models may run anywhere.
    expect(status?.disclosure).toMatchObject({ on_device: true, cost: "free_on_device" });
    expect(status?.cloud_disclosure).toMatchObject({ on_device: false, cost: "cloud_via_local" });
    const backend = { kind: "provider", provider_id: here.provider_id } as const;
    for (const m of await api.listModels(backend)) {
      expect(m).toMatchObject({ on_device: false, runs_in_cloud: true, price_known: false });
    }
    // A new key is a key, and one may be given later.
    await expect(api.updateModelProviderKey(here.provider_id, " ")).rejects.toMatchObject({
      kind: "invalid",
    });
    expect(await api.updateModelProviderKey(here.provider_id, "demo-key-0000Ab12")).toMatchObject({
      key_last4: "Ab12",
    });
  });

  it("gives no key to a server that takes none, here or elsewhere", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-local" });
    await expect(api.updateModelProviderKey("ollama", "demo-key-0000Ab12")).rejects.toMatchObject({
      kind: "invalid",
      message: "Ollama doesn't use a key.",
    });
  });

  it("lists the models of a local app on another computer as running there, priced", async () => {
    const api = createMockApi(fast);
    const remote = await api.addModelProvider("ollama", "https://gpu.example.edu:11434", null);
    expect(remote.on_device).toBe(false);
    const models = await api.listModels({ kind: "provider", provider_id: remote.provider_id });
    expect(models.length).toBeGreaterThan(0);
    for (const m of models) expect(m).toMatchObject({ on_device: false, runs_in_cloud: false });
  });

  it("makes a provider off this computer ready once its note is read, also when routed", async () => {
    const api = createMockApi(fast);
    await api.addModelProvider("openai", null, "sk-demo-secret-value-9f3A");
    await api.setFeatureModel("weekly_explanation", {
      backend: openai,
      model: "gpt-6-luna",
      effort: "lowest",
    });
    const [facts] = await api.modelDisclosure(openai, false);
    await api.acknowledgeModelDisclosure(openai, false, facts.version);
    const status = (await api.aiStatus()).backends.find((b) => b.label === "OpenAI");
    expect(status).toMatchObject({ state: "ready", cloud_disclosure: null });
  });

  it("answers where a model runs as the facade does, asked or not", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-local" });
    const local = { backend: OLLAMA, model: "qwen3.5:9b", effort: "lowest" } as const;
    const cloud = { ...local, model: "gpt-oss:120b-cloud" } as const;
    expect(await api.modelRunsHere(local, true)).toBe(true);
    expect(await api.modelRunsHere(cloud, true)).toBe(false);
    // Not asked, an Ollama model is unknown.
    expect(await api.modelRunsHere(local, false)).toBe(false);
  });

  it("keeps the server's note and the cloud note apart, each under its own key", async () => {
    const api = createMockApi({ ...fast, scenario: "ai-local" });
    await api.setFeatureModel("weekly_explanation", {
      backend: OLLAMA,
      model: "gpt-oss:120b-cloud",
      effort: "lowest",
    });
    const ollama = () => api.aiStatus().then((s) => s.backends.find((b) => b.label === "Ollama"));
    // Its own note was read; a routed cloud model's note wasn't.
    expect(await ollama()).toMatchObject({ state: "needs_disclosure" });
    const [facts, read] = await api.modelDisclosure(OLLAMA, false);
    expect(read).toBe(false);
    expect((await ollama())?.disclosure.cost).toBe("free_on_device");
    await api.acknowledgeModelDisclosure(OLLAMA, false, facts.version);
    expect(await ollama()).toMatchObject({
      state: "ready",
      cloud_disclosure_acknowledged: facts.version,
    });
    expect(
      (
        await api.estimateGeneration({
          feature: "weekly_explanation",
          course: "folder:demo-courses/course/DEMO101",
        })
      ).would_block,
    ).toBe("price_unknown_not_acknowledged");
  });

  it("clears a note's block by its version alone, like the facade (the ChatGPT plan too)", async () => {
    const api = createMockApi({ ...fast, scenario: "codex-free" });
    expect((await api.estimateGeneration({ feature: "weekly_note" })).would_block).toBe(
      "disclosure_not_acknowledged",
    );
    const [facts] = await api.modelDisclosure({ kind: "codex" }, false);
    await api.acknowledgeModelDisclosure({ kind: "codex" }, false, facts.version);
    expect((await api.estimateGeneration({ feature: "weekly_note" })).would_block).not.toBe(
      "disclosure_not_acknowledged",
    );
  });
});
