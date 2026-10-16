import { describe, expect, it } from "vitest";
import { createMockApi } from "./index";

const FITTED = "canvas:canvas.demo.test/course/332";
const UNLABELLED = "canvas:canvas.demo.test/course/240";
const LEGACY = "canvas:canvas.demo.test/course/205";
const SUMMER = "canvas:canvas.demo.test/course/101";

const mock = () => createMockApi({ latencyMs: 0, scenario: "uoft-fall" });

describe("mock: the other courses of a session", () => {
  it("lists the courses of the same session, with their own dates, not the course itself", async () => {
    const api = mock();
    const listed = await api.sessionCourses(FITTED);
    expect(listed.session).toMatch(/^\d{4}9$/);
    expect(listed.courses.map((c) => c.course_id).sort()).toEqual([LEGACY, UNLABELLED].sort());
    const legacy = listed.courses.find((c) => c.course_id === LEGACY);
    expect(legacy).toMatchObject({ own_dates: true, hidden: false });
    expect(legacy?.student_start).toBeTruthy();
    expect(listed.courses.find((c) => c.course_id === UNLABELLED)).toMatchObject({
      own_dates: false,
      student_start: null,
      student_end: null,
    });
  });

  it("lists nothing for a course whose code names no session", async () => {
    const api = createMockApi({ latencyMs: 0 });
    expect(await api.sessionCourses("folder:demo-courses/course/DEMO101")).toEqual({
      session: null,
      courses: [],
    });
  });

  it("copies a course's own dates to the courses chosen, and skips what it can't", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED, SUMMER, "no-such-course"]);
    expect(report.applied).toEqual([UNLABELLED]);
    expect(report.replaced).toEqual([
      { course_id: UNLABELLED, start: null, end: null, legacy: false },
    ]);
    expect(report.skipped).toEqual([
      { course_id: SUMMER, reason: "not_same_session" },
      { course_id: "no-such-course", reason: "unknown_course" },
    ]);
    const there = (await api.courseOverview(UNLABELLED)).timeline.term;
    expect(there).toMatchObject({
      anchor: "student_confirmed",
      student_start: "2026-09-08",
      student_end: "2026-12-04",
    });
  });

  it("copies nothing from a course with no dates of the student's", async () => {
    const api = mock();
    const report = await api.applyCourseDatesTo(UNLABELLED, [FITTED]);
    expect(report).toEqual({
      applied: [],
      skipped: [{ course_id: FITTED, reason: "no_dates" }],
      replaced: [],
    });
  });

  it("never gives a course its own dates", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    expect((await api.applyCourseDatesTo(FITTED, [FITTED])).skipped).toEqual([
      { course_id: FITTED, reason: "not_same_session" },
    ]);
  });

  it("restores from what it is given, like the facade", async () => {
    const api = mock();
    await api.restoreCourseDates([
      { course_id: UNLABELLED, start: "2026-09-01", end: null, legacy: true },
      { course_id: "removed-since", start: null, end: null, legacy: false },
    ]);
    expect((await api.courseOverview(UNLABELLED)).timeline.term).toMatchObject({
      student_start: "2026-09-01",
      anchor_origin: "legacy",
    });
  });

  it("puts every course back exactly, unconfirmed dates from version 0.1 included", async () => {
    const api = mock();
    const before = (await api.courseOverview(LEGACY)).timeline.term;
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED, LEGACY]);
    expect(report.replaced).toEqual([
      { course_id: UNLABELLED, start: null, end: null, legacy: false },
      { course_id: LEGACY, start: before.student_start, end: null, legacy: true },
    ]);
    await api.restoreCourseDates(report.replaced);
    const legacy = (await api.courseOverview(LEGACY)).timeline.term;
    expect(legacy).toMatchObject({ anchor_origin: "legacy", student_start: before.student_start });
    const unlabelled = (await api.courseOverview(UNLABELLED)).timeline.term;
    expect(unlabelled.student_start ?? null).toBeNull();
    expect(unlabelled.anchor).not.toBe("student_confirmed");
  });
});
