// Provisional facade types for course calendar proposals, candidates and syllabus reading
// (calendar design §4, §7; work item F3), as proposed to backend-2 on 2026-09-28 for B6–B9.
// Written by hand so the UI and the mock can be built first.
//
// Delete this file when `pnpm gen:types` brings the real types into generated.ts, and
// re-export them from there instead (src/api/types.ts).

import type {
  AiLabel,
  BreakKind,
  CalendarOrigin,
  CalendarStatus,
  CoursePhase,
  DateSpan,
  EvidenceParam,
  MaterialKind,
  TeachingSegment,
} from "../generated";

/** Calendar date, "YYYY-MM-DD". */
type IsoDate = string;
/** RFC 3339 instant. */
type Timestamp = string;

/** A calendar the student confirmed, or one proposed to them (built by pagelamp-core). */
export interface CourseCalendar {
  segments: TeachingSegment[];
  breaks: CalendarBreakInCalendar[];
  exam_period?: DateSpan | null;
  final_exam_on?: IsoDate | null;
  weeks: CalendarWeek[];
}

export interface CalendarBreakInCalendar {
  kind: BreakKind;
  span: DateSpan;
  numbered: boolean;
  /** Material text (≤ 80 chars), plain text only. */
  label: string;
}

export interface CalendarWeek {
  number: number;
  starts_on: IsoDate;
  /** Material text (≤ 120 chars), plain text only. */
  topic?: string | null;
}

export type DateKind =
  | "first_class"
  | "last_class"
  | "break"
  | "exam_period"
  | "final_exam"
  | "week_start";

/** Where a date came from: a quote in a course material (or derived from a validated table). */
export interface DateEvidence {
  material_id: string;
  title: string;
  /** e.g. "p. 2". */
  locator?: string | null;
  /** The material's own words (material text), plain text only. */
  quote?: string | null;
  url?: string | null;
  /** Worked out from a validated schedule table rather than quoted. */
  derived: boolean;
}

export interface AlternativeDate {
  date: IsoDate;
  end?: IsoDate | null;
  label: string;
  evidence: DateEvidence[];
}

export interface ProposedDate {
  kind: DateKind;
  /** 0 = first part, 1 = second part of a full-year course. */
  segment: number;
  week?: number | null;
  break_kind?: BreakKind | null;
  numbered?: boolean | null;
  date: IsoDate;
  end?: IsoDate | null;
  label: string;
  evidence: DateEvidence[];
  /** Different materials gave different dates; the later one is the default (V9). */
  alternatives: AlternativeDate[];
}

export type ConflictCode =
  | "syllabus_from_another_year"
  | "inconsistent"
  | "disagrees_with_notes"
  | "disagrees_with_lms_dates"
  | "disagrees_with_class_event"
  | "differs_from_institution_calendar";

/** A choice between two for the student (V5, V7, V8). */
export interface CalendarConflict {
  code: ConflictCode;
  kind: DateKind;
  segment: number;
  options: AlternativeDate[];
}

export type DropReason =
  | "unknown_source"
  | "unsupported_quote"
  | "date_not_in_quote"
  | "ambiguous_year"
  | "outside_frame"
  | "inconsistent"
  | "table_dropped";

export interface DropCount {
  reason: DropReason;
  count: number;
}

export type ChangeCode =
  | "first_class_moved"
  | "last_class_moved"
  | "break_added"
  | "break_removed"
  | "exams_end_set"
  | "week_today_changes"
  | "phase_changes";

/** What accepting would change, as a code and params (like EvidenceItem). */
export interface CalendarChange {
  code: ChangeCode;
  params: EvidenceParam[];
}

export interface CalendarProposal {
  id: number;
  course_id: string;
  origin: CalendarOrigin;
  calendar: CourseCalendar;
  dates: ProposedDate[];
  conflicts: CalendarConflict[];
  dropped: DropCount[];
  low_quality: boolean;
  /** No conflicts and not low quality: can be accepted in a batch. */
  passing: boolean;
  ai_label?: AiLabel | null;
  /** Show the one-time question (b) reminder with this proposal (D37 option 2). */
  sharing_reminder: boolean;
  resulting_week_today?: number | null;
  resulting_phase: CoursePhase;
  changes: CalendarChange[];
  created_at: Timestamp;
}

export interface AcceptedCalendar {
  id: number;
  origin: CalendarOrigin;
  calendar: CourseCalendar;
  dates: ProposedDate[];
  ai_label?: AiLabel | null;
  accepted_at: Timestamp;
  /** A quoted material changed and a quote is no longer found (still in force). */
  stale: boolean;
  stale_since?: IsoDate | null;
  changed_materials: string[];
}

export type CandidateReason =
  | "syllabus"
  | "linked_from_syllabus"
  | "title_outline"
  | "title_schedule"
  | "title_info"
  | "front_page"
  | "start_module"
  | "announcement"
  | "named_in_course_toml"
  | "student_added";

export type CandidateLeftOut = "no_text" | "scanned" | "over_budget" | "excluded_by_student";

export interface CalendarCandidate {
  material_id: string;
  title: string;
  kind: MaterialKind;
  reason: CandidateReason;
  /** Read by the scan and the AI (after the student's own choice, if any). */
  included: boolean;
  /** The student's add (true) or remove (false), if they chose. */
  student_choice?: boolean | null;
  has_text: boolean;
  /** A Canvas file not downloaded yet ("Download (counts as viewing in Canvas)"). */
  downloadable: boolean;
  left_out?: CandidateLeftOut | null;
  url?: string | null;
}

/** Why the AI can't read this course's syllabus now (facade BlockReason, design §3.4). */
export type CalendarBlockReason =
  | "course_policy_prohibited"
  | "course_ai_turned_off"
  | "course_hidden"
  | "no_readable_materials"
  | "material_sharing_not_allowed"
  | "disclosure_not_acknowledged"
  | "no_model_chosen";

export interface CourseCalendarView {
  course_id: string;
  accepted?: AcceptedCalendar | null;
  proposals: CalendarProposal[];
  status: CalendarStatus;
  candidates: CalendarCandidate[];
  blocked?: CalendarBlockReason | null;
}

/** A course the "Read syllabi for N courses" batch would read (the facade decides). */
export interface SyllabusOffer {
  course_id: string;
  reason_code: string;
  candidates: number;
  has_text: boolean;
}
