import { describe, expect, it } from "vitest";
import type { SyncEvent } from "../types";
import { createMockApi } from ".";

const fast = { latencyMs: 0, syncStepMs: 0 };

describe("mock API", () => {
  it("serves the synthetic demo courses, hidden ones included", async () => {
    const api = createMockApi(fast);
    const courses = await api.listCourses();
    expect(courses.map((c) => c.course.code)).toEqual(["DEMO101", "DEMO205", "DEMO310", "DEMO099"]);
    expect(courses.find((c) => c.course.code === "DEMO099")?.course.hidden).toBe(true);
  });

  it("rejects expired Canvas tokens with kind auth and never echoes the token", async () => {
    const api = createMockApi(fast);
    const token = "expired-token-1234567890";
    const error = await api.addCanvasSource("https://canvas.example.edu", token).catch((e) => e);
    expect(error.kind).toBe("auth");
    expect(error.message).not.toContain(token);
  });

  it("streams sync events and reports failed sources without throwing", async () => {
    const api = createMockApi({ ...fast, scenario: "expired" });
    const events: SyncEvent[] = [];
    const summary = await api.syncAll({}, (e) => events.push(e));
    expect(summary.ok).toBe(false);
    expect(events.at(0)?.type).toBe("source_started");
    const canvas = summary.results.find((r) => r.kind === "canvas");
    expect(canvas?.error_kind).toBe("auth_expired_or_revoked");
  });

  it("refuses to sync while another process holds the lock", async () => {
    const api = createMockApi({ ...fast, scenario: "busy" });
    await expect(api.syncAll({}, () => {})).rejects.toMatchObject({ kind: "busy" });
  });

  it("replacing an expired token clears the source error", async () => {
    const api = createMockApi({ ...fast, scenario: "expired" });
    const source = await api.updateSourceSecret("canvas:canvas.demo.test", "a-fresh-valid-token");
    expect(source.last_error_kind).toBeNull();
  });

  it("returns week materials with available weeks for the switcher", async () => {
    const api = createMockApi(fast);
    const week = await api.weekMaterials("folder:demo-courses/course/DEMO101");
    expect(week.week).toBe(4);
    expect(week.available_weeks).toEqual([1, 2, 3, 4]);
  });

  it("computes AI access to materials like the facade and keeps the switch under 'No AI'", async () => {
    const api = createMockApi(fast);
    const id = "folder:demo-courses/course/DEMO101";
    const find = async () => (await api.listCourses()).find((c) => c.course.id === id);

    expect((await find())?.ai_materials).toBe("readable");

    await api.setCourseAiAccess(id, false);
    let row = await find();
    expect(row?.ai_materials).toBe("turned_off");
    expect(row?.counts.indexed_materials).toBe(0);

    await api.setCoursePolicy(id, "prohibited", null);
    expect((await find())?.ai_materials).toBe("withheld_by_policy");

    // Switch turned back on while "No AI": still withheld, and the stored value is kept.
    await api.setCourseAiAccess(id, true);
    row = await find();
    expect(row?.ai_materials).toBe("withheld_by_policy");
    expect(row?.course.ai_access).toBe(true);

    await api.setCoursePolicy(id, "learning_aid", null);
    expect((await find())?.ai_materials).toBe("readable");
  });

  it("falls back to the synced term when the student clears their override", async () => {
    const api = createMockApi(fast);
    const id = "folder:demo-courses/course/DEMO101";
    const before = (await api.courseOverview(id)).course;
    expect(before.term_source).toBe("synced");

    await api.setCourseTerm(id, "2026-09-01", null);
    expect((await api.courseOverview(id)).course.term_source).toBe("user");

    await api.setCourseTerm(id, null, null);
    const after = (await api.courseOverview(id)).course;
    expect(after.term_source).toBe("synced");
    expect(after.term_start).toBe(before.term_start);
  });
});
