// Mock scenarios for course weeks, phases and the lifecycle (M0.10, F1), picked with
// `?scenario=` like the others:
// - uoft-fall: the owner's case with synthetic data. Canvas reports a May → January "Fall" term
//   (an enrollment window), so it is not used to count weeks: one course gets its week from the
//   professor's "Week 1/2/3" posts, one has no week numbers ("Week unknown"), one keeps a v0.1
//   override ("Check this course's dates"), and older courses sit under Past.
// - phases: one course per phase and lifecycle state, a full-year course, a kept-current one.
// - all-past: only past courses.
// Every course, code and date is made up; dates are relative to `now`.

import type {
  Confidence,
  Course,
  CourseLifecycle,
  Deadline,
  MaterialView,
  RejectedDates,
} from "../types";
import {
  type CalendarFields,
  calendarFields,
  dayFrom,
  ev,
  lifecycle,
  resolution,
  teachingStart,
} from "./calendar";
import {
  buildMockDb,
  course,
  deadline,
  type MockCourse,
  type MockDb,
  type MockScenario,
  material,
  mockCourse,
  SOURCE_CANVAS,
  SOURCE_FOLDER,
  timeline,
} from "./fixtures";

export const CALENDAR_SCENARIOS = [
  "uoft-fall",
  "phases",
  "all-past",
  "removed",
  "proposals",
] as const;
type CalendarScenario = (typeof CALENDAR_SCENARIOS)[number];

function isCalendarScenario(scenario: MockScenario): scenario is CalendarScenario {
  return (CALENDAR_SCENARIOS as readonly string[]).includes(scenario);
}

interface ScenarioSpec {
  key: string;
  source?: string;
  code: string;
  name: string;
  hidden?: boolean;
  /** Course.term_* as the facade reports them (the student's override wins over the sync). */
  term?: { start: string | null; end: string | null; user?: boolean };
  week: number | null;
  confidence: Confidence;
  evidence?: string[];
  calendar: Partial<CalendarFields> & Pick<CalendarFields, "phase">;
  lifecycle: CourseLifecycle;
  keptCurrentUntil?: string;
  materials?: (c: Course) => MaterialView[];
  deadlines?: (c: Course) => Deadline[];
}

function build(now: Date, spec: ScenarioSpec): MockCourse {
  const source = spec.source ?? SOURCE_CANVAS;
  const id = `${source}/course/${spec.key}`;
  const courseSpec = {
    id,
    sourceId: source,
    code: spec.code,
    name: spec.name,
    policy: "unknown" as const,
    policyNote: null,
    hidden: spec.hidden ?? false,
    termStartDays: null,
    week: spec.week,
    confidence: spec.confidence,
    evidence: spec.evidence ?? [],
    url: source === SOURCE_CANVAS ? `https://canvas.demo.test/courses/${spec.key}` : null,
    calendar: calendarFields(spec.calendar),
  };
  const c: Course = {
    ...course(courseSpec, now),
    term_start: spec.term?.start ?? null,
    term_end: spec.term?.end ?? null,
    term_source: spec.term?.user ? "user" : spec.term?.start || spec.term?.end ? "synced" : "none",
  };
  return mockCourse({
    course: c,
    timeline: timeline(courseSpec, now, []),
    lifecycle: spec.lifecycle,
    keptCurrentUntil: spec.keptCurrentUntil ?? null,
    modules: [],
    materials: spec.materials?.(c) ?? [],
    announcements: [],
    deadlines: spec.deadlines?.(c) ?? [],
  });
}

/** The Fall session's year: this year from May, else last year. */
function fallYear(now: Date): number {
  return now.getMonth() >= 4 ? now.getFullYear() : now.getFullYear() - 1;
}

function scenarioCourses(now: Date, scenario: CalendarScenario): MockCourse[] {
  const fy = fallYear(now);
  // Days from today to a weekday of the week `w` weeks from this one (0 = this week's Monday).
  const dow = (now.getDay() + 6) % 7;
  const wk = (w: number, day = 0) => -dow + 7 * w + day;
  const on = (w: number, day = 0) => dayFrom(now, wk(w, day));

  // Canvas's "Fall" term: an enrollment window from May to the end of January (39 weeks).
  const wide = { start: dayFrom(now, -147), end: dayFrom(now, 125) };
  const wideNotUsed: RejectedDates = {
    source: "lms_term",
    start: wide.start,
    end: wide.end,
    reason: "longer_than_teaching_term",
    end_only: false,
  };
  const wideEvidence = ev("term_looks_like_enrollment_window", {
    term_name: `Fall ${fy}`,
    start: wide.start,
    end: wide.end,
    weeks: 39,
  });

  const fitted = build(now, {
    key: "332",
    code: `DEM332H5 F LEC0101 ${fy}9`,
    name: "Methods of Demonstration",
    term: wide,
    week: 4,
    confidence: "medium",
    evidence: [`Week-numbered materials put week 1 on the week of ${on(-3)} → week 4`],
    calendar: {
      phase: "teaching",
      default_week: 4,
      notes_week: 3,
      term: resolution({
        week_one_monday: on(-3),
        anchor: "published_week_labels",
        anchor_confidence: "medium",
        outer_frame: wide,
        not_used: [wideNotUsed],
      }),
      evidence_items: [
        wideEvidence,
        ev("week_labels_fit", { weeks: 3, monday: on(-3) }),
        ev("week_from_dates", { week: 4, monday: on(-3) }),
        ev("breaks_unknown"),
      ],
    },
    lifecycle: lifecycle({
      state: "current",
      last_activity: on(-1, 1),
      next_event: dayFrom(now, 5),
    }),
    materials: (c) => [
      material(c.id, "Week 1 slides", "file", 1, wk(-3, 1), now, { status: "not_downloaded" }),
      material(c.id, "Week 2 slides", "file", 2, wk(-2, 1), now, { status: "not_downloaded" }),
      material(c.id, "Week 3 slides", "file", 3, wk(-1, 1), now, { status: "not_downloaded" }),
    ],
    deadlines: (c) => [deadline(c, "Problem set 1", "assignment_due", 5, now)],
  });

  const unlabelled = build(now, {
    key: "240",
    code: `DEM240H5 F LEC0101 ${fy}9`,
    name: "Placeholder Structures",
    term: wide,
    week: null,
    confidence: "low",
    evidence: ["No week numbers found in this course's materials"],
    calendar: {
      phase: "unknown",
      phase_confidence: "low",
      term: resolution({ outer_frame: wide, not_used: [wideNotUsed] }),
      evidence_items: [wideEvidence, ev("no_week_signal")],
    },
    lifecycle: lifecycle({ state: "unknown", confidence: "low", last_activity: on(-1, 3) }),
    materials: (c) => [
      material(c.id, "Lecture slides — Introduction", "page", null, wk(-3, 2), now),
      material(c.id, "Lecture slides — Measuring placeholders", "page", null, wk(-1, 3), now),
    ],
  });

  // A v0.1 override that survived the v3 clean-up: the start the student typed stays, the
  // untouched prefilled end was cleared (so Course.term_end shows Canvas's again).
  const legacy = build(now, {
    key: "205",
    code: `DEM205H5 F LEC0101 ${fy}9`,
    name: "Sample Spaces and Other Rooms",
    term: { start: on(-3, 1), end: wide.end, user: true },
    week: 4,
    confidence: "high",
    evidence: [`Classes started ${on(-3, 1)} (set by you) → week 4`],
    calendar: {
      phase: "teaching",
      phase_confidence: "high",
      default_week: 4,
      notes_week: 4,
      term: resolution({
        week_one_monday: on(-3),
        teaching: [{ first_class: on(-3, 1), last_class: null, first_week_number: 1 }],
        anchor: "student_confirmed",
        anchor_confidence: "high",
        anchor_origin: "legacy",
        outer_frame: wide,
        not_used: [wideNotUsed],
        student_start: on(-3, 1),
      }),
      evidence_items: [
        ev("legacy_dates", { start: on(-3, 1) }),
        ev("dates_agree", { source: "published_week_labels", monday: on(-3) }),
        ev("week_from_dates", { week: 4, monday: on(-3) }),
      ],
    },
    lifecycle: lifecycle({ state: "current", confidence: "high", last_activity: on(0) }),
    materials: (c) => [material(c.id, "Week 4 notes", "page", 4, wk(0), now)],
  });

  const summer = build(now, {
    key: "101",
    code: `DEM101H5 F LEC0101 ${fy - 2}5`,
    name: "First Steps in Demonstration",
    term: { start: `${fy - 2}-05-01`, end: `${fy - 2}-08-31` },
    week: null,
    confidence: "low",
    calendar: {
      phase: "unknown",
      phase_confidence: "low",
      term: resolution({ not_used: [] }),
      evidence_items: [ev("no_week_signal")],
    },
    lifecycle: lifecycle({
      state: "ended",
      since: `${fy - 2}-09-21`,
      last_activity: `${fy - 2}-06-20`,
      evidence_items: [
        ev("session_ended", { session: `${fy - 2}5`, end: `${fy - 2}-06-30` }),
        ev("quiet_since", { date: `${fy - 2}-06-20`, days: 800 }),
      ],
    }),
  });

  const lastFall = build(now, {
    key: "236",
    code: `DEM236H5 F LEC0101 ${fy - 1}9`,
    name: "Intermediate Placeholders",
    term: { start: `${fy - 1}-05-04`, end: `${fy}-01-31` },
    week: null,
    confidence: "low",
    calendar: {
      phase: "ended",
      phase_confidence: "medium",
      term: resolution({
        outer_frame: { start: `${fy - 1}-05-04`, end: `${fy}-01-31` },
        not_used: [{ ...wideNotUsed, start: `${fy - 1}-05-04`, end: `${fy}-01-31` }],
      }),
      evidence_items: [ev("no_week_signal")],
    },
    lifecycle: lifecycle({
      state: "ended",
      confidence: "high",
      since: `${fy - 1}-12-31`,
      last_activity: `${fy - 1}-12-18`,
      evidence_items: [
        ev("lms_concluded"),
        ev("quiet_since", { date: `${fy - 1}-12-18`, days: 280 }),
      ],
    }),
  });

  const winterHidden = build(now, {
    key: "150",
    code: `DEM150H5 S LEC0101 ${fy}1`,
    name: "Winter Demonstrations",
    hidden: true,
    term: { start: `${fy}-01-05`, end: `${fy}-04-30` },
    week: null,
    confidence: "low",
    calendar: { phase: "ended", term: resolution({ anchor: "lms_term" }) },
    lifecycle: lifecycle({
      state: "ended",
      since: `${fy}-05-21`,
      last_activity: `${fy}-04-22`,
      evidence_items: [
        ev("term_end_passed", { term_name: `Winter ${fy}`, end: `${fy}-04-30` }),
        ev("quiet_since", { date: `${fy}-04-22`, days: 150 }),
      ],
    }),
  });

  // ---- phases ------------------------------------------------------------------------------
  const userTerm = (start: string, end: string | null) =>
    resolution({
      week_one_monday: start,
      teaching: [{ first_class: start, last_class: end, first_week_number: 1 }],
      anchor: "student_confirmed",
      anchor_confidence: "high",
      anchor_origin: "user",
      student_start: start,
      student_end: end,
    });

  const teaching = build(now, {
    key: "PHS110",
    source: SOURCE_FOLDER,
    code: "PHS110",
    name: "Teaching Week Studies",
    term: { start: on(-5), end: on(7, 4), user: true },
    week: 6,
    confidence: "high",
    calendar: {
      phase: "teaching",
      phase_confidence: "high",
      default_week: 6,
      notes_week: 7,
      term: userTerm(on(-5), on(7, 4)),
      evidence_items: [
        ev("student_dates", { start: on(-5), end: on(7, 4) }),
        ev("notes_ahead", { week: 7 }),
      ],
    },
    lifecycle: lifecycle({ state: "current", confidence: "high", next_event: dayFrom(now, 2) }),
    deadlines: (c) => [deadline(c, "Essay draft", "assignment_due", 2, now)],
  });

  const readingWeekNumbered = build(now, {
    key: "PHS120",
    source: SOURCE_FOLDER,
    code: "PHS120",
    name: "Numbered Breaks",
    week: 7,
    confidence: "high",
    calendar: {
      phase: "break",
      phase_confidence: "high",
      default_week: 7,
      current_break_kind: "reading_week",
      term: resolution({
        ...userTerm(on(-6), on(6, 4)),
        breaks: [
          {
            kind: "reading_week",
            span: { start: on(0), end: on(0, 4) },
            numbered: true,
            label: "Reading Week",
          },
        ],
      }),
      evidence_items: [ev("in_break", { kind: "reading_week", start: on(0), end: on(0, 4) })],
    },
    lifecycle: lifecycle({ state: "current", confidence: "high" }),
  });

  const readingWeek = build(now, {
    key: "PHS130",
    source: SOURCE_FOLDER,
    code: "PHS130",
    name: "Unnumbered Breaks",
    week: null,
    confidence: "high",
    calendar: {
      phase: "break",
      phase_confidence: "high",
      default_week: 6,
      break_after_week: 6,
      current_break_kind: "reading_week",
      term: resolution({
        ...userTerm(on(-6), on(7, 4)),
        breaks: [
          {
            kind: "reading_week",
            span: { start: on(0), end: on(0, 4) },
            numbered: false,
            label: "Fall Reading Week (no classes)",
          },
        ],
      }),
      evidence_items: [ev("in_break", { kind: "reading_week", start: on(0), end: on(0, 4) })],
    },
    lifecycle: lifecycle({ state: "current", confidence: "high" }),
  });

  const exams = build(now, {
    key: "PHS140",
    code: "PHS140",
    name: "Examination Season",
    term: { start: on(-13, 1), end: dayFrom(now, -5) },
    week: null,
    confidence: "low",
    calendar: {
      phase: "exam_period",
      phase_confidence: "low",
      last_teaching_week: 12,
      term: resolution({
        week_one_monday: on(-13),
        teaching: [{ first_class: on(-13, 1), last_class: dayFrom(now, -5), first_week_number: 1 }],
        anchor: "lms_course_dates",
        anchor_confidence: "medium",
      }),
      evidence_items: [ev("exam_period_estimated", { days: 21 })],
    },
    lifecycle: lifecycle({
      state: "finishing",
      next_event: dayFrom(now, 6),
      evidence_items: [
        ev("exam_period_estimated", { days: 21 }),
        ev("next_event", { date: dayFrom(now, 6), title: "Final exam" }),
      ],
    }),
    deadlines: (c) => [deadline(c, "Final exam", "exam", 6, now, 9, 0)],
  });

  const ended = build(now, {
    key: "PHS150",
    code: "PHS150",
    name: "Concluded Matters",
    term: { start: dayFrom(now, -140), end: dayFrom(now, -45) },
    week: null,
    confidence: "medium",
    calendar: {
      phase: "ended",
      phase_confidence: "high",
      term: resolution({
        teaching: [
          { first_class: dayFrom(now, -140), last_class: dayFrom(now, -45), first_week_number: 1 },
        ],
        exams_end: dayFrom(now, -30),
        anchor: "lms_course_dates",
        anchor_confidence: "medium",
      }),
      evidence_items: [ev("ended_on", { date: dayFrom(now, -30) })],
    },
    lifecycle: lifecycle({
      state: "ended",
      confidence: "high",
      since: dayFrom(now, -23),
      last_activity: dayFrom(now, -33),
      evidence_items: [
        ev("dates_ended", { date: dayFrom(now, -30) }),
        ev("quiet_since", { date: dayFrom(now, -33), days: 33 }),
      ],
    }),
  });

  const upcoming = build(now, {
    key: "PHS160",
    code: "PHS160",
    name: "Coming Attractions",
    term: { start: dayFrom(now, 40), end: dayFrom(now, 40 + 12 * 7) },
    week: null,
    confidence: "medium",
    calendar: {
      phase: "not_started",
      starts_on: teachingStart(dayFrom(now, 40)),
      term: resolution({
        week_one_monday: on(Math.ceil((40 + dow) / 7)),
        teaching: [{ first_class: dayFrom(now, 40), last_class: null, first_week_number: 1 }],
        anchor: "lms_course_dates",
        anchor_confidence: "medium",
      }),
      evidence_items: [ev("starts_on", { date: dayFrom(now, 40) })],
    },
    lifecycle: lifecycle({ state: "upcoming", starts_on: dayFrom(now, 40) }),
  });

  const unknownWithSignal = build(now, {
    key: "PHS170",
    source: SOURCE_FOLDER,
    code: "PHS170",
    name: "Uncertain Timelines",
    week: 3,
    confidence: "low",
    calendar: {
      phase: "unknown",
      phase_confidence: "low",
      default_week: 3,
      evidence_items: [
        ev("week_from_latest_material", {
          week: 3,
          title: "Week 3 reading",
          date: dayFrom(now, -12),
          days: 10,
        }),
      ],
    },
    lifecycle: lifecycle({ state: "unknown", confidence: "low" }),
  });

  const fullYear = build(now, {
    key: "PHS180",
    code: "PHS180Y",
    name: "The Long Course (full year)",
    week: 5,
    confidence: "high",
    calendar: {
      phase: "teaching",
      phase_confidence: "high",
      default_week: 5,
      calendar: "accepted",
      term: resolution({
        week_one_monday: on(-4),
        teaching: [
          { first_class: on(-4, 1), last_class: on(9, 3), first_week_number: 1 },
          { first_class: on(13, 1), last_class: on(25, 3), first_week_number: 13 },
        ],
        breaks: [
          {
            kind: "winter_break",
            span: { start: on(10), end: on(12, 6) },
            numbered: false,
            label: "Winter break",
          },
        ],
        exams_end: on(27, 4),
        anchor: "student_confirmed",
        anchor_confidence: "high",
        anchor_origin: "ai",
        ai_label: {
          backend_label: "Demo AI (API key)",
          model: "demo-model-1",
          created_at: `${dayFrom(now, -10)}T15:04:00Z`,
        },
      }),
      evidence_items: [
        ev("student_dates", { start: on(-4, 1), end: on(25, 3) }),
        ev("week_from_dates", { week: 5, monday: on(-4) }),
      ],
    },
    lifecycle: lifecycle({ state: "current", confidence: "high" }),
  });

  const inactive = build(now, {
    key: "PHS190",
    code: "PHS190",
    name: "Orientation Site",
    // An old week-numbered material still names a week; an inactive course shows none.
    week: 12,
    confidence: "low",
    calendar: { phase: "unknown", phase_confidence: "low", default_week: 12 },
    lifecycle: lifecycle({
      state: "inactive",
      last_activity: dayFrom(now, -150),
      evidence_items: [ev("no_dates_inactive", { days: 150 })],
    }),
  });

  const keptBase = lifecycle({
    state: "ended",
    since: dayFrom(now, -8),
    evidence_items: [ev("course_end_passed", { source: "lms_term", end: dayFrom(now, -29) })],
  });
  const kept = build(now, {
    key: "PHS200",
    code: "PHS200",
    name: "Still Taking This",
    term: { start: dayFrom(now, -120), end: dayFrom(now, -29) },
    week: null,
    confidence: "medium",
    calendar: {
      phase: "ended",
      term: resolution({
        teaching: [
          { first_class: dayFrom(now, -120), last_class: dayFrom(now, -29), first_week_number: 1 },
        ],
        anchor: "lms_term",
        anchor_confidence: "medium",
      }),
    },
    lifecycle: keptBase,
    keptCurrentUntil: dayFrom(now, 60),
  });

  const conflicting = build(now, {
    key: "PHS210",
    code: "PHS210",
    name: "Disagreeing Sources",
    term: { start: on(-2, 1), end: on(10, 4) },
    week: 3,
    confidence: "low",
    calendar: {
      phase: "teaching",
      phase_confidence: "low",
      default_week: 3,
      notes_week: 5,
      term: resolution({
        week_one_monday: on(-2),
        teaching: [{ first_class: on(-2, 1), last_class: on(10, 4), first_week_number: 1 }],
        anchor: "lms_course_dates",
        anchor_confidence: "low",
      }),
      evidence_items: [
        ev("lms_course_dates", { start: on(-2, 1), end: on(10, 4) }),
        ev("week_from_dates", { week: 3, monday: on(-2) }),
        ev("dates_may_be_wrong", { source: "published_week_labels", monday: on(-4), days: 14 }),
      ],
    },
    lifecycle: lifecycle({ state: "current", confidence: "low" }),
  });

  switch (scenario) {
    case "uoft-fall":
      return [fitted, unlabelled, legacy, summer, lastFall, winterHidden];
    case "phases":
      return [
        teaching,
        readingWeekNumbered,
        readingWeek,
        exams,
        ended,
        upcoming,
        unknownWithSignal,
        fullYear,
        inactive,
        kept,
        conflicting,
      ];
    case "all-past":
      return [summer, lastFall, ended, inactive];
    case "proposals":
      // proposals.ts adds the outlines, schedules and proposals.
      return [fitted, unlabelled, legacy, summer, lastFall];
    case "removed":
      // removal.ts removes the first three past courses; two stay suggested.
      return [fitted, unlabelled, legacy, summer, lastFall, winterHidden, ended, inactive];
  }
}

/** The mock database for a calendar scenario, or null for any other scenario. */
export function buildCalendarScenarioDb(now: Date, scenario: MockScenario): MockDb | null {
  if (!isCalendarScenario(scenario)) return null;
  return { ...buildMockDb(now, "demo"), courses: scenarioCourses(now, scenario), studyPlan: null };
}
