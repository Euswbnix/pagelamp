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
    expect(listed.campus).toBe(5);
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
      campus: null,
      courses: [],
    });
  });

  it("copies a course's own dates to the courses chosen, and skips what it can't", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED, SUMMER, "no-such-course"]);
    expect(report.applied).toEqual([UNLABELLED]);
    expect(report.replaced).toEqual([
      { course_id: UNLABELLED, start: null, end: null, legacy: false, copied_from: null },
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

  it("never gives a course its own dates, and gives a course asked for twice its dates once", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    const report = await api.applyCourseDatesTo(FITTED, [FITTED, UNLABELLED, UNLABELLED]);
    expect(report.skipped).toEqual([{ course_id: FITTED, reason: "from_course" }]);
    expect(report.applied).toEqual([UNLABELLED]);
    expect(report.replaced).toHaveLength(1);
  });

  it("gives on no dates from version 0.1 the student hasn't confirmed", async () => {
    const api = mock();
    expect((await api.applyCourseDatesTo(LEGACY, [UNLABELLED])).skipped).toEqual([
      { course_id: UNLABELLED, reason: "dates_not_confirmed" },
    ]);
  });

  it("knows unconfirmed dates by the confirmed list, a last day alone too, like the facade", async () => {
    const api = mock();
    // A 0.1 course with only a last day of classes: no anchor of its own says "legacy".
    await api.restoreCourseDates([
      { course_id: FITTED, start: null, end: "2026-12-04", legacy: true, copied_from: null },
    ]);
    const term = (await api.courseOverview(FITTED)).timeline.term;
    expect(term).toMatchObject({ student_end: "2026-12-04", student_dates_confirmed: false });
    expect(term.anchor_origin).not.toBe("legacy");
    expect((await api.applyCourseDatesTo(FITTED, [UNLABELLED])).skipped).toEqual([
      { course_id: UNLABELLED, reason: "dates_not_confirmed" },
    ]);
    // Confirmed ("These dates are right"), they go on.
    await api.confirmCourseDates(FITTED);
    expect((await api.courseOverview(FITTED)).timeline.term.student_dates_confirmed).toBe(true);
    expect((await api.applyCourseDatesTo(FITTED, [UNLABELLED])).applied).toEqual([UNLABELLED]);
    // Saved, a course's dates are confirmed, and a course with none has none to confirm.
    await api.setCourseTerm(LEGACY, null, "2026-12-04");
    expect((await api.courseOverview(LEGACY)).timeline.term.student_dates_confirmed).toBe(true);
    await api.setCourseTerm(LEGACY, null, null);
    expect((await api.courseOverview(LEGACY)).timeline.term.student_dates_confirmed).toBe(true);
  });

  it("puts back where a course's dates had been copied from before", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    await api.applyCourseDatesTo(FITTED, [UNLABELLED]);
    // Then the dates of the course they went to go on to a third one... and come back.
    await api.setCourseTerm(LEGACY, "2026-09-15", null);
    const report = await api.applyCourseDatesTo(LEGACY, [UNLABELLED]);
    expect(report.replaced[0]).toMatchObject({ course_id: UNLABELLED, copied_from: FITTED });
    await api.restoreCourseDates(report.replaced);
    const items = (await api.courseOverview(UNLABELLED)).timeline.evidence_items;
    expect(items.find((i) => i.code === "dates_copied_from")?.params).toEqual([
      expect.objectContaining({ key: "course", value: expect.stringMatching(/^DEM332H5/) }),
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
      student_dates_confirmed: false,
    });
  });

  it("tells Undo of a last day kept alone from version 0.1, and puts it back unconfirmed", async () => {
    const api = mock();
    await api.restoreCourseDates([
      { course_id: UNLABELLED, start: null, end: "2026-12-11", legacy: true, copied_from: null },
    ]);
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED]);
    expect(report.replaced).toEqual([
      { course_id: UNLABELLED, start: null, end: "2026-12-11", legacy: true, copied_from: null },
    ]);
    await api.restoreCourseDates(report.replaced);
    expect((await api.courseOverview(UNLABELLED)).timeline.term).toMatchObject({
      student_start: null,
      student_end: "2026-12-11",
      student_dates_confirmed: false,
    });
  });

  /** A calendar of its own for `courseId`, saved with the dates form v2 (exams after classes). */
  const ownCalendar = (api: ReturnType<typeof mock>, courseId: string) =>
    api.setCourseDates(courseId, {
      first_class: "2026-09-08",
      last_class: "2026-12-04",
      exams_end: "2026-12-22",
      breaks: [],
    });

  it("lists a course with a calendar of its own as such, and leaves it alone, like the facade", async () => {
    const api = mock();
    await ownCalendar(api, UNLABELLED);
    const listed = (await api.sessionCourses(FITTED)).courses;
    // Its term dates are only the calendar's span as mirrored there: they end with the exams.
    expect(listed.find((c) => c.course_id === UNLABELLED)).toMatchObject({
      own_calendar: true,
      own_dates: true,
      student_start: "2026-09-08",
      student_end: "2026-12-22",
    });
    expect(listed.find((c) => c.course_id === LEGACY)?.own_calendar).toBe(false);
    await api.setCourseTerm(FITTED, "2026-09-15", null);
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED, LEGACY]);
    expect(report.applied).toEqual([LEGACY]);
    expect(report.skipped).toEqual([{ course_id: UNLABELLED, reason: "own_calendar" }]);
    expect(report.replaced.map((r) => r.course_id)).toEqual([LEGACY]);
    expect((await api.courseOverview(UNLABELLED)).timeline.term.student_start).toBe("2026-09-08");
    // Cleared with the dates form, it has no calendar of its own any more.
    await api.setCourseDates(UNLABELLED, null);
    const again = (await api.sessionCourses(FITTED)).courses;
    expect(again.find((c) => c.course_id === UNLABELLED)?.own_calendar).toBe(false);
  });

  it("says in the timeline whether a course has a calendar of its own, with no session too", async () => {
    const api = mock();
    expect((await api.courseOverview(FITTED)).timeline.term.own_calendar).toBe(false);
    await ownCalendar(api, FITTED);
    expect((await api.courseOverview(FITTED)).timeline.term.own_calendar).toBe(true);
    const plain = createMockApi({ latencyMs: 0 });
    const folder = "folder:demo-courses/course/DEMO101";
    await plain.setCourseDates(folder, {
      first_class: "2026-09-08",
      last_class: "2026-12-04",
      breaks: [],
    });
    expect(await plain.sessionCourses(folder)).toMatchObject({ session: null, courses: [] });
    expect((await plain.courseOverview(folder)).timeline.term.own_calendar).toBe(true);
  });

  it("copies a calendar's own days of classes, not the end of its exams", async () => {
    const api = mock();
    await ownCalendar(api, FITTED);
    expect((await api.applyCourseDatesTo(FITTED, [UNLABELLED])).applied).toEqual([UNLABELLED]);
    expect((await api.courseOverview(UNLABELLED)).timeline.term).toMatchObject({
      student_start: "2026-09-08",
      student_end: "2026-12-04",
    });
    // A full-year calendar: its second part's last day of classes.
    await api.setCourseDates(FITTED, {
      first_class: "2026-09-08",
      last_class: "2026-12-04",
      second_segment: {
        first_class: "2027-01-06",
        last_class: "2027-04-02",
        restart_numbering: false,
      },
      exams_end: "2027-04-30",
      breaks: [],
    });
    await api.applyCourseDatesTo(FITTED, [LEGACY]);
    expect((await api.courseOverview(LEGACY)).timeline.term).toMatchObject({
      student_start: "2026-09-08",
      student_end: "2027-04-02",
    });
  });

  it("ends a course's copied mark when its dates are cleared with the dates form", async () => {
    const api = mock();
    await api.setCourseTerm(FITTED, "2026-09-08", null);
    await api.applyCourseDatesTo(FITTED, [UNLABELLED]);
    await api.setCourseDates(UNLABELLED, null);
    // Copied again: nothing to say it had come from FITTED before.
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED]);
    expect(report.applied).toEqual([UNLABELLED]);
    expect(report.replaced[0]?.copied_from ?? null).toBeNull();
  });

  it("counts an accepted calendar as the course's own, and copies from its days of classes", async () => {
    const api = createMockApi({ latencyMs: 0, scenario: "proposals" });
    // DEM205 starts with an accepted calendar (gone stale): its own, like the facade says.
    expect((await api.courseCalendar(LEGACY)).status).toBe("accepted_stale");
    expect((await api.courseOverview(LEGACY)).timeline.term.own_calendar).toBe(true);
    const pending = (await api.courseCalendar(FITTED)).proposals[0];
    if (!pending) throw new Error("the scenario has a proposal for DEM332");
    await api.acceptCalendarProposal(pending.id, null);
    const listed = (await api.sessionCourses(UNLABELLED)).courses;
    expect(listed.find((c) => c.course_id === FITTED)?.own_calendar).toBe(true);
    expect(listed.find((c) => c.course_id === LEGACY)?.own_calendar).toBe(true);
    await api.setCourseTerm(UNLABELLED, "2026-09-15", null);
    expect((await api.applyCourseDatesTo(UNLABELLED, [FITTED, LEGACY])).skipped).toEqual([
      { course_id: FITTED, reason: "own_calendar" },
      { course_id: LEGACY, reason: "own_calendar" },
    ]);
    // From it: the accepted calendar's first and last day of classes.
    const segments = pending.calendar.segments;
    expect(segments[0]?.first_class).not.toBe("2026-09-15");
    expect((await api.applyCourseDatesTo(FITTED, [UNLABELLED])).applied).toEqual([UNLABELLED]);
    expect((await api.courseOverview(UNLABELLED)).timeline.term).toMatchObject({
      student_start: segments[0]?.first_class,
      student_end: segments.at(-1)?.last_class ?? null,
    });
  });

  it("keeps a course's own calendar when its term dates are saved, like the facade", async () => {
    const api = mock();
    await ownCalendar(api, FITTED);
    await api.setCourseTerm(FITTED, "2026-09-15", null);
    const term = (await api.courseOverview(FITTED)).timeline.term;
    expect(term).toMatchObject({ student_start: "2026-09-15", student_end: null });
    expect(term.teaching[0]).toMatchObject({ first_class: "2026-09-08", last_class: "2026-12-04" });
    expect((await api.courseOverview(FITTED)).timeline.term.own_calendar).toBe(true);
    // A course with a calendar and no term dates of its own: own_calendar, not own_dates.
    await api.setCourseTerm(FITTED, null, null);
    const listed = (await api.sessionCourses(UNLABELLED)).courses.find(
      (c) => c.course_id === FITTED,
    );
    expect(listed).toMatchObject({ own_calendar: true, own_dates: false });
  });

  it("counts a calendar the scenario starts with as the course's own", async () => {
    const api = createMockApi({ latencyMs: 0, scenario: "phases" });
    const course = (await api.listCourses()).find((c) => c.course.code === "PHS180Y");
    if (!course) throw new Error("the phases scenario has PHS180Y");
    expect(course.timeline.calendar).toBe("accepted");
    expect((await api.courseOverview(course.course.id)).timeline.term.own_calendar).toBe(true);
  });

  it("puts every course back exactly, unconfirmed dates from version 0.1 included", async () => {
    const api = mock();
    const before = (await api.courseOverview(LEGACY)).timeline.term;
    await api.setCourseTerm(FITTED, "2026-09-08", "2026-12-04");
    const report = await api.applyCourseDatesTo(FITTED, [UNLABELLED, LEGACY]);
    expect(report.replaced).toEqual([
      { course_id: UNLABELLED, start: null, end: null, legacy: false, copied_from: null },
      {
        course_id: LEGACY,
        start: before.student_start,
        end: null,
        legacy: true,
        copied_from: null,
      },
    ]);
    await api.restoreCourseDates(report.replaced);
    const legacy = (await api.courseOverview(LEGACY)).timeline.term;
    expect(legacy).toMatchObject({
      anchor_origin: "legacy",
      student_start: before.student_start,
      student_dates_confirmed: false,
    });
    const unlabelled = (await api.courseOverview(UNLABELLED)).timeline.term;
    expect(unlabelled.student_start ?? null).toBeNull();
    expect(unlabelled.anchor).not.toBe("student_confirmed");
  });
});
