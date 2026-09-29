import { describe, expect, it } from "vitest";
import { createMockApi } from ".";
import { addDays, isoOf, mondayOf, teachingStart } from "./calendar";
import { CALENDAR_SCENARIOS } from "./courseScenarios";

// A fixed Monday, so week arithmetic in the fixtures is easy to check.
const NOW = new Date(2026, 8, 28, 10, 0); // 2026-09-28
const fast = { latencyMs: 0, syncStepMs: 0, now: () => NOW };
const TODAY = isoOf(NOW);

describe("mock calendar helpers", () => {
  it("finds the Monday of a week", () => {
    expect(mondayOf("2026-09-28")).toBe("2026-09-28");
    expect(mondayOf("2026-10-04")).toBe("2026-09-28"); // Sunday
    expect(mondayOf("2026-11-03")).toBe("2026-11-02"); // across the DST change
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    // Like the facade: a weekend first class starts teaching the next Monday.
    expect(teachingStart("2026-10-03")).toBe("2026-10-05"); // Saturday
    expect(teachingStart("2026-10-04")).toBe("2026-10-05"); // Sunday
    expect(teachingStart("2026-10-06")).toBe("2026-10-06");
  });
});

describe("mock calendar scenarios", () => {
  it.each(CALENDAR_SCENARIOS)(
    "%s: every course has a phase and a lifecycle group",
    async (scenario) => {
      const api = createMockApi({ ...fast, scenario });
      const courses = await api.listCourses();
      expect(courses.length).toBeGreaterThan(0);
      for (const c of courses) {
        expect(c.timeline.phase).toBeTruthy();
        expect(["current", "upcoming", "past"]).toContain(c.lifecycle.group);
        // outside_term is derived from the phase (design §3.3).
        expect(c.timeline.outside_term).toBe(
          c.timeline.phase === "not_started" || c.timeline.phase === "ended",
        );
      }
    },
  );

  it("uoft-fall: the wide Canvas term is never used to count weeks", async () => {
    const api = createMockApi({ ...fast, scenario: "uoft-fall" });
    const courses = await api.listCourses();
    const fitted = courses.find((c) => c.course.code?.startsWith("DEM332"));
    expect(fitted?.timeline.current_week).toBe(4);
    expect(fitted?.timeline.term.anchor).toBe("published_week_labels");
    expect(fitted?.timeline.term.not_used[0]?.reason).toBe("longer_than_teaching_term");
    const unlabelled = courses.find((c) => c.course.code?.startsWith("DEM240"));
    expect(unlabelled?.timeline.current_week ?? null).toBeNull();
    expect(unlabelled?.timeline.phase).toBe("unknown");
    expect(courses.filter((c) => c.lifecycle.group === "past")).toHaveLength(3);
  });

  it("all-past: nothing is current", async () => {
    const api = createMockApi({ ...fast, scenario: "all-past" });
    const courses = await api.listCourses();
    expect(courses.every((c) => c.lifecycle.group === "past")).toBe(true);
  });
});

describe("mock course dates and lifecycle", () => {
  it("keeps a past course current until the default date, and undoes it", async () => {
    const api = createMockApi({ ...fast, scenario: "phases" });
    const id = "canvas:canvas.demo.test/course/PHS150";
    await api.keepCourseCurrent(id, null);
    let overview = await api.courseOverview(id);
    expect(overview.lifecycle.group).toBe("current");
    expect(overview.lifecycle.kept_current_until).toBe(addDays(TODAY, 120));

    await api.clearKeepCourseCurrent(id);
    overview = await api.courseOverview(id);
    expect(overview.lifecycle.group).toBe("past");
    expect(overview.lifecycle.kept_current_until ?? null).toBeNull();
  });

  it("refuses a keep-current date in the past", async () => {
    const api = createMockApi({ ...fast, scenario: "phases" });
    const error = await api
      .keepCourseCurrent("canvas:canvas.demo.test/course/PHS150", "2026-09-01")
      .catch((e) => e);
    expect(error.kind).toBe("invalid");
  });

  it("confirms dates kept from version 0.1", async () => {
    const api = createMockApi({ ...fast, scenario: "uoft-fall" });
    const id = "canvas:canvas.demo.test/course/205";
    expect((await api.courseOverview(id)).timeline.term.anchor_origin).toBe("legacy");
    await api.confirmCourseDates(id);
    expect((await api.courseOverview(id)).timeline.term.anchor_origin).toBe("user");
  });

  it("counts weeks from the student's first day of classes", async () => {
    const api = createMockApi({ ...fast, scenario: "uoft-fall" });
    const id = "canvas:canvas.demo.test/course/240";
    await api.setCourseTerm(id, "2026-09-08", null);
    const { timeline, lifecycle } = await api.courseOverview(id);
    expect(timeline.current_week).toBe(4);
    expect(timeline.phase).toBe("teaching");
    expect(timeline.term.student_start).toBe("2026-09-08");
    expect(timeline.term.student_end ?? null).toBeNull();
    expect(lifecycle.group).toBe("current");
  });

  it("treats the last day of classes as an end-only anchor", async () => {
    const api = createMockApi({ ...fast, scenario: "uoft-fall" });
    const id = "canvas:canvas.demo.test/course/240";
    await api.setCourseTerm(id, "2026-06-01", "2026-09-20");
    expect((await api.courseOverview(id)).timeline.phase).toBe("exam_period");
    await api.setCourseTerm(id, "2026-05-04", "2026-08-01");
    const ended = await api.courseOverview(id);
    expect(ended.timeline.phase).toBe("ended");
    expect(ended.lifecycle.group).toBe("past");

    // Clearing goes back to what the source reported.
    await api.setCourseTerm(id, null, null);
    const cleared = await api.courseOverview(id);
    expect(cleared.timeline.phase).toBe("unknown");
    expect(cleared.course.term_source).toBe("synced");
  });
});
