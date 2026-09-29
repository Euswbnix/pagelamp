// Mock course calendar proposals (calendar design §7; F3): the calendar view with candidates,
// proposals from the scan, AI and the student's AI app, accepting (with edits), dismissing, the
// candidate picker and the syllabus-reading offers. A simplified stand-in for pagelamp-app's
// calendar module with its own state beside the mock database.

import { calendarToInput } from "@/lib/calendarInput";
import type { PageLampApi } from "../client";
import { ApiError } from "../errors";
import type {
  AcceptedCalendar,
  AiLabel,
  CalendarBlockReason,
  CalendarCandidate,
  CalendarOrigin,
  CalendarProposal,
  CalendarStatus,
  CourseCalendar,
  CourseCalendarView,
  CourseDatesInput,
  MaterialView,
  ProposedDate,
} from "../types";
import { aiMaterialsState } from "../types";
import { addDays, dayFrom, isoOf, mondayFrom } from "./calendar";
import { type MockCourse, type MockDb, type MockScenario, material } from "./fixtures";

type ProposalsApi = Pick<
  PageLampApi,
  | "courseCalendar"
  | "setCalendarSources"
  | "scanCourseCalendar"
  | "acceptCalendarProposal"
  | "acceptPassingProposals"
  | "dismissCalendarProposal"
  | "syllabusReadingOffers"
>;

interface CourseState {
  accepted: AcceptedCalendar | null;
  proposals: CalendarProposal[];
  /** The student's add (true) or remove (false) per material. */
  choices: Map<string, boolean>;
}

const CANDIDATE_TITLE: [RegExp, CalendarCandidate["reason"]][] = [
  [/syllabus|outline|教学大纲/i, "title_outline"],
  [/schedule|calendar|important dates|课程安排|日程/i, "title_schedule"],
  [/course information|info sheet/i, "title_info"],
];

export function createProposalsMock(deps: {
  db: MockDb;
  scenario: MockScenario;
  now: () => Date;
  respond: <T>(value: T | (() => T), extraLatency?: number) => Promise<T>;
  findCourse: (courseId: string) => MockCourse;
  /** Put a calendar in force on the mock course (timeline, lifecycle, term dates). */
  applyCalendar: (
    c: MockCourse,
    input: CourseDatesInput,
    origin: CalendarOrigin,
    aiLabel: AiLabel | null,
  ) => void;
}) {
  const { db, now, respond, findCourse } = deps;
  const states = new Map<string, CourseState>();
  let nextId = 1;

  function stateOf(courseId: string): CourseState {
    let state = states.get(courseId);
    if (!state) {
      state = { accepted: null, proposals: [], choices: new Map() };
      states.set(courseId, state);
    }
    return state;
  }

  function reasonFor(m: MaterialView): CalendarCandidate["reason"] | null {
    if (m.kind === "syllabus") return "syllabus";
    if (m.kind === "external_link" || m.kind === "announcement") return null;
    return CANDIDATE_TITLE.find(([re]) => re.test(m.title))?.[1] ?? null;
  }

  function candidatesOf(c: MockCourse): CalendarCandidate[] {
    const state = stateOf(c.course.id);
    const list: CalendarCandidate[] = [];
    for (const m of c.materials) {
      const choice = state.choices.get(m.id);
      const reason = choice === true && !reasonFor(m) ? "student_added" : reasonFor(m);
      if (!reason) continue;
      const hasText = m.text_status === "ok" && m.chunk_count > 0;
      const scanned = m.text_status === "ok" && m.chunk_count === 0;
      const downloadable =
        m.kind === "file" && m.text_status === "not_downloaded" && !m.download_blocked;
      const excluded = choice === false;
      list.push({
        material_id: m.id,
        title: m.title,
        kind: m.kind,
        reason,
        included: hasText && !excluded,
        student_choice: choice ?? null,
        has_text: hasText,
        downloadable,
        left_out: excluded
          ? "excluded_by_student"
          : scanned
            ? "scanned"
            : hasText
              ? null
              : "no_text",
        url: m.url ?? null,
      });
    }
    return list;
  }

  function blockedOf(c: MockCourse, candidates: CalendarCandidate[]): CalendarBlockReason | null {
    if (c.course.hidden) return "course_hidden";
    const materials = aiMaterialsState(c.course);
    if (materials === "withheld_by_policy") return "course_policy_prohibited";
    if (materials === "turned_off") return "course_ai_turned_off";
    if (!candidates.some((x) => x.included)) return "no_readable_materials";
    return null;
  }

  function statusOf(state: CourseState): CalendarStatus {
    if (state.accepted) return state.accepted.stale ? "accepted_stale" : "accepted";
    return state.proposals.length > 0 ? "proposed" : "none";
  }

  /** Keep CourseTimeline.calendar in step with this module's state. */
  function sync(c: MockCourse) {
    c.timeline = { ...c.timeline, calendar: statusOf(stateOf(c.course.id)) };
  }

  function view(c: MockCourse): CourseCalendarView {
    const state = stateOf(c.course.id);
    const candidates = candidatesOf(c);
    return {
      course_id: c.course.id,
      accepted: state.accepted,
      proposals: state.proposals,
      status: statusOf(state),
      candidates,
      blocked: blockedOf(c, candidates),
    };
  }

  function findProposal(id: number): {
    c: MockCourse;
    state: CourseState;
    proposal: CalendarProposal;
  } {
    for (const c of db.courses) {
      const state = stateOf(c.course.id);
      const proposal = state.proposals.find((p) => p.id === id);
      if (proposal) return { c, state, proposal };
    }
    throw new ApiError("not_found", `No calendar proposal ${id}`);
  }

  function accept(id: number, edits: CourseDatesInput | null): CourseCalendarView {
    const { c, state, proposal } = findProposal(id);
    deps.applyCalendar(
      c,
      edits ?? calendarToInput(proposal.calendar),
      proposal.origin,
      proposal.ai_label ?? null,
    );
    state.accepted = {
      id: proposal.id,
      origin: proposal.origin,
      calendar: proposal.calendar,
      dates: proposal.dates,
      ai_label: proposal.ai_label ?? null,
      accepted_at: now().toISOString(),
      stale: false,
      stale_since: null,
      changed_materials: [],
    };
    state.proposals = state.proposals.filter((p) => p.id !== id);
    sync(c);
    return view(c);
  }

  // ---- scenario "proposals" -------------------------------------------------------------------
  const today = isoOf(now());
  const on = (weeks: number, days = 0) => addDays(mondayFrom(now(), weeks), days);
  const quote = (
    materialId: string,
    title: string,
    text: string,
    locator: string | null = null,
  ) => ({
    material_id: materialId,
    title,
    locator,
    quote: text,
    url: null,
    derived: false,
  });

  /** A calendar starting the Tuesday of week 1 = 3 weeks ago: today is week 4. */
  function fallCalendar(): CourseCalendar {
    return {
      segments: [{ first_class: on(-3, 1), last_class: on(10, 1), first_week_number: 1 }],
      breaks: [
        {
          kind: "reading_week",
          span: { start: on(3), end: on(3, 4) },
          numbered: false,
          label: "Reading Week",
        },
      ],
      exam_period: { start: on(10, 3), end: on(12, 1) },
      final_exam_on: null,
      weeks: [],
    };
  }

  function proposal(
    c: MockCourse,
    origin: CalendarOrigin,
    outline: MaterialView,
    extra: Partial<CalendarProposal> = {},
  ): CalendarProposal {
    const calendar = fallCalendar();
    const title = outline.title;
    const dates: ProposedDate[] = [
      {
        kind: "first_class",
        segment: 0,
        date: calendar.segments[0]?.first_class ?? today,
        label: "First class",
        evidence: [quote(outline.id, title, `Classes begin Tuesday, ${fmt(on(-3, 1))}.`, "p. 1")],
        alternatives: [],
      },
      {
        kind: "break",
        segment: 0,
        break_kind: "reading_week",
        numbered: false,
        date: on(3),
        end: on(3, 4),
        label: "Reading Week",
        evidence: [
          quote(
            outline.id,
            title,
            `Reading Week (no classes): ${fmt(on(3))} to ${fmt(on(3, 4))}`,
            "p. 2",
          ),
        ],
        alternatives: [],
      },
      {
        kind: "last_class",
        segment: 0,
        date: on(10, 1),
        label: "Last class",
        evidence: [quote(outline.id, title, `Last lecture: ${fmt(on(10, 1))}`, "p. 2")],
        alternatives: [],
      },
      {
        kind: "exam_period",
        segment: 0,
        date: on(10, 3),
        end: on(12, 1),
        label: "Final exam period",
        evidence: [
          quote(
            outline.id,
            title,
            `Final exam period: ${fmt(on(10, 3))} – ${fmt(on(12, 1))}`,
            "p. 3",
          ),
        ],
        alternatives: [],
      },
    ];
    return {
      id: nextId++,
      course_id: c.course.id,
      origin,
      calendar,
      dates,
      conflicts: [],
      dropped: [],
      low_quality: false,
      passing: true,
      ai_label: null,
      sharing_reminder: false,
      resulting_week_today: 4,
      resulting_phase: "teaching",
      changes: [],
      created_at: new Date(now().getTime() - 3_600_000).toISOString(),
      ...extra,
    };
  }

  if (deps.scenario === "proposals") {
    const byCode = (prefix: string) => db.courses.find((c) => c.course.code?.startsWith(prefix));
    const fitted = byCode("DEM332");
    const unlabelled = byCode("DEM240");
    const legacy = byCode("DEM205");
    const aiLabel: AiLabel = {
      backend_label: "Demo AI (API key)",
      model: "demo-model-1",
      created_at: new Date(now().getTime() - 3_600_000).toISOString(),
    };

    if (fitted) {
      const outline = addMaterial(fitted, "Course outline", "syllabus", 8);
      const schedule = addMaterial(fitted, "Lecture schedule", "page", 5);
      const base = proposal(fitted, "ai", outline, { ai_label: aiLabel, sharing_reminder: true });
      // The outline and the schedule page disagree about the first class (V9), and the exam
      // period starts before the last class in one of them (V7): the student chooses.
      const first = base.dates[0];
      if (first) {
        first.alternatives = [
          {
            date: on(-3, 3),
            label: "First lecture",
            evidence: [
              quote(schedule.id, schedule.title, `Week 1 – Thu ${fmt(on(-3, 3))}: Introduction`),
            ],
          },
        ];
      }
      base.conflicts = [
        {
          code: "inconsistent",
          kind: "exam_period",
          segment: 0,
          options: [
            {
              date: on(10, 3),
              end: on(12, 1),
              label: "Final exam period",
              evidence: [
                quote(
                  outline.id,
                  outline.title,
                  `Final exam period: ${fmt(on(10, 3))} – ${fmt(on(12, 1))}`,
                  "p. 3",
                ),
              ],
            },
            {
              date: on(9, 4),
              end: on(11, 4),
              label: "Exams",
              evidence: [
                quote(schedule.id, schedule.title, `Exams: ${fmt(on(9, 4))} – ${fmt(on(11, 4))}`),
              ],
            },
          ],
        },
      ];
      base.passing = false;
      base.dropped = [
        { reason: "unsupported_quote", count: 2 },
        { reason: "date_not_in_quote", count: 1 },
      ];
      base.changes = [
        {
          code: "break_added",
          params: [
            { key: "kind", value: "reading_week" },
            { key: "start", value: on(3) },
            { key: "end", value: on(3, 4) },
          ],
        },
        { code: "exams_end_set", params: [{ key: "date", value: on(12, 1) }] },
      ];
      stateOf(fitted.course.id).proposals.push(base);
      sync(fitted);
    }

    if (unlabelled) {
      addMaterial(unlabelled, "Syllabus.pdf", "file", 0, "not_downloaded");
      const info = addMaterial(unlabelled, "Course information", "page", 3);
      const scan = proposal(unlabelled, "scan", info, {
        resulting_week_today: 4,
        changes: [
          {
            code: "week_today_changes",
            params: [
              { key: "from", value: "unknown" },
              { key: "to", value: "4" },
            ],
          },
          {
            code: "first_class_moved",
            params: [
              { key: "from", value: "" },
              { key: "to", value: on(-3, 1) },
            ],
          },
        ],
      });
      stateOf(unlabelled.course.id).proposals.push(scan);
      sync(unlabelled);
    }

    if (legacy) {
      const outline = addMaterial(legacy, "Course outline (updated)", "syllabus", 6);
      const old = proposal(legacy, "ai", outline, { ai_label: aiLabel });
      stateOf(legacy.course.id).accepted = {
        id: old.id,
        origin: "ai",
        calendar: old.calendar,
        dates: old.dates,
        ai_label: aiLabel,
        accepted_at: new Date(now().getTime() - 9 * 86_400_000).toISOString(),
        stale: true,
        stale_since: dayFrom(now(), -2),
        changed_materials: [outline.title],
      };
      sync(legacy);
    }
  }

  function addMaterial(
    c: MockCourse,
    title: string,
    kind: MaterialView["kind"],
    chunks: number,
    status: MaterialView["text_status"] = "ok",
  ): MaterialView {
    const m = material(c.course.id, title, kind, null, -20, now(), { chunks, status });
    c.materials.push(m);
    return m;
  }

  const api: ProposalsApi = {
    courseCalendar: (courseId) => respond(() => view(findCourse(courseId))),

    setCalendarSources: (courseId, include, exclude) =>
      respond(() => {
        const c = findCourse(courseId);
        const state = stateOf(courseId);
        for (const id of include) state.choices.set(id, true);
        for (const id of exclude) state.choices.set(id, false);
        return candidatesOf(c);
      }),

    scanCourseCalendar: (courseId) =>
      respond(() => {
        const c = findCourse(courseId);
        const state = stateOf(courseId);
        const source = candidatesOf(c).find((x) => x.included);
        if (!source || state.proposals.some((p) => p.origin === "scan")) return null;
        const m = c.materials.find((x) => x.id === source.material_id);
        if (!m) return null;
        const scan = proposal(c, "scan", m);
        state.proposals.push(scan);
        sync(c);
        return scan;
      }, 200),

    acceptCalendarProposal: (proposalId, edits) => respond(() => accept(proposalId, edits)),

    acceptPassingProposals: (proposalIds) =>
      respond(() =>
        proposalIds.map((id) => {
          const { proposal } = findProposal(id);
          if (!proposal.passing) {
            throw new ApiError("invalid", "This proposal has conflicts to choose between first.");
          }
          return accept(id, null);
        }),
      ),

    dismissCalendarProposal: (proposalId) =>
      respond(() => {
        const { c, state } = findProposal(proposalId);
        state.proposals = state.proposals.filter((p) => p.id !== proposalId);
        sync(c);
      }),

    syllabusReadingOffers: () =>
      respond(() =>
        db.courses
          .filter((c) => {
            const candidates = candidatesOf(c);
            return (
              c.lifecycle.group !== "past" &&
              !stateOf(c.course.id).accepted &&
              blockedOf(c, candidates) === null
            );
          })
          .map((c) => {
            const candidates = candidatesOf(c);
            return {
              course_id: c.course.id,
              reason_code: "no_calendar",
              candidates: candidates.length,
              has_text: candidates.some((x) => x.has_text),
            };
          }),
      ),
  };

  return { api };
}

/** "September 8" in English, as a syllabus would write it (quotes are the material's words). */
function fmt(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return new Intl.DateTimeFormat("en", { month: "long", day: "numeric" }).format(
    new Date(y, m - 1, d),
  );
}
