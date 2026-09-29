import { describe, expect, it } from "vitest";
import type { ApiError } from "../errors";
import { createMockApi } from ".";

const NOW = new Date(2026, 8, 28, 10, 0); // Monday 2026-09-28
// "proposals" routes AI to an API key, so a plan can be written.
const fast = { latencyMs: 0, syncStepMs: 0, now: () => NOW, scenario: "proposals" as const };
const events = () => {
  const list: string[] = [];
  return { list, onEvent: (e: { type: string }) => list.push(e.type) };
};

describe("mock study plans", () => {
  it("checks the request like the facade", async () => {
    const api = createMockApi(fast);
    const { onEvent } = events();
    for (const request of [
      { horizon_days: 0 },
      { horizon_days: 57 },
      { hours_per_week: 81 },
      {
        days_off: [
          "monday",
          "tuesday",
          "wednesday",
          "thursday",
          "friday",
          "saturday",
          "sunday",
        ] as const,
      },
    ]) {
      await expect(
        api.generateStudyPlan(
          { ...request, days_off: [...(request.days_off ?? [])] },
          "g",
          onEvent,
        ),
      ).rejects.toMatchObject({ kind: "invalid" } satisfies Partial<ApiError>);
    }
  });

  it("writes a draft on study days only, at most 4 hours a day, and saves it when accepted", async () => {
    const api = createMockApi(fast);
    const { list, onEvent } = events();
    const draft = await api.generateStudyPlan(
      { horizon_days: 14, hours_per_week: 40, days_off: ["saturday", "sunday"] },
      "plan-1",
      onEvent,
    );
    expect(list).toEqual(["started", "stage", "stage", "stage", "usage", "finished"]);
    expect(draft.plan.horizon_start).toBe("2026-09-28");
    expect(draft.plan.horizon_end).toBe("2026-10-11");
    const weekdays = draft.plan.items.map((i) => new Date(`${i.date}T12:00`).getDay());
    expect(weekdays.every((d) => d !== 0 && d !== 6)).toBe(true);
    const perDay = new Map<string, number>();
    for (const item of draft.plan.items) {
      perDay.set(item.date, (perDay.get(item.date) ?? 0) + (item.minutes ?? 0));
    }
    expect(Math.max(...perDay.values())).toBeLessThanOrEqual(240);
    expect(draft.warnings).toEqual([{ code: "graded_work_left_out", count: 1 }]);
    expect(await api.latestStudyPlan()).not.toMatchObject({ generation_id: "plan-1" });

    const stored = await api.acceptStudyPlan("plan-1");
    expect(stored).toMatchObject({
      origin: "pagelamp",
      generation_id: "plan-1",
      ai_label: { backend_label: draft.meta.backend_label, model: draft.meta.model },
    });
    expect(await api.latestStudyPlan()).toEqual(stored);
    await expect(api.acceptStudyPlan("plan-1")).rejects.toMatchObject({ kind: "invalid" });
    await expect(api.acceptStudyPlan("nope")).rejects.toMatchObject({ kind: "not_found" });

    const ticked = await api.setStudyPlanItemDone(stored.id, 0, true);
    expect(ticked.plan.items[0]?.done).toBe(true);
    await expect(api.setStudyPlanItemDone(stored.id, 999, true)).rejects.toMatchObject({
      kind: "not_found",
    });
  });

  it("stops at the next stage, and is listed as a generation while it runs", async () => {
    const api = createMockApi({ ...fast, syncStepMs: 20 });
    const running = api.generateStudyPlan({}, "plan-2", () => {});
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect((await api.activity()).items).toMatchObject([
      { kind: "generation", generation_id: "plan-2" },
    ]);
    await api.cancelGeneration("plan-2");
    await expect(running).rejects.toMatchObject({ kind: "cancelled" });
    expect((await api.activity()).items).toEqual([]);
  });

  it("is blocked without a model", async () => {
    const api = createMockApi({ ...fast, scenario: "demo" });
    await expect(api.generateStudyPlan({}, "g", () => {})).rejects.toMatchObject({
      kind: "blocked",
    });
  });
});
