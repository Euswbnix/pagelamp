// Provisional facade types for course weeks, phases and the lifecycle (calendar design §3.3 and
// §4; plan M0.10, work item F1), written by hand so the UI and the mock can be built before the
// backend lands them. Agreed with backend-2 and the leader on 2026-09-28.
//
// Delete this file when `pnpm gen:types` brings the real types into generated.ts: the
// augmentation below then only repeats fields generated.ts already has, and the named types are
// re-exported from there instead (src/api/types.ts).

import type { Confidence } from "../generated";

/** Calendar date, "YYYY-MM-DD" (same as IsoDate in ../types). */
type IsoDate = string;
/** RFC 3339 instant. */
type Timestamp = string;

export type CoursePhase =
  | "not_started"
  | "teaching"
  | "break"
  | "exam_period"
  | "ended"
  | "unknown";

/** Where week 1 came from, strongest first (design §6.4). */
export type TermAnchorSource =
  | "student_confirmed"
  | "lms_course_dates"
  | "lms_term"
  | "folder_config"
  | "institution_calendar"
  | "published_week_labels"
  | "none";

/** Why a pair of dates was not used to count weeks (design §6.3). */
export type RejectReason =
  | "longer_than_teaching_term"
  | "shorter_than_teaching_term"
  | "starts_long_before_activity"
  | "starts_before_session_window"
  | "end_outside_session_window"
  | "starts_after_end"
  | "conflicts_with_stronger_source";

export interface RejectedDates {
  source: TermAnchorSource;
  start?: IsoDate | null;
  end?: IsoDate | null;
  reason: RejectReason;
  /** True: only the end was not used (the start still counts). */
  end_only: boolean;
}

export type BreakKind = "reading_week" | "holiday" | "winter_break" | "other";

export type CalendarOrigin = "user" | "legacy" | "scan" | "ai" | "ai_app" | "restored";

export type CalendarStatus = "none" | "proposed" | "accepted" | "accepted_stale";

export interface AiLabel {
  backend_label: string;
  model: string;
  created_at: Timestamp;
}

export interface DateSpan {
  start: IsoDate;
  end: IsoDate;
}

export interface TeachingSegment {
  first_class: IsoDate;
  last_class?: IsoDate | null;
  /** 0 is allowed ("Week 0"). */
  first_week_number: number;
}

export interface CalendarBreak {
  kind: BreakKind;
  span: DateSpan;
  /** True when the break has a week number of its own. */
  numbered: boolean;
  /** Material text (≤ 80 chars), e.g. "Reading Week"; shown as plain text. */
  label: string;
}

/** The resolver's result: which dates count and which were set aside. */
export interface TermResolution {
  week_one_monday?: IsoDate | null;
  teaching: TeachingSegment[];
  breaks: CalendarBreak[];
  exams_end?: IsoDate | null;
  anchor: TermAnchorSource;
  anchor_confidence: Confidence;
  /** For `student_confirmed`: where the confirmed dates came from. */
  anchor_origin?: CalendarOrigin | null;
  /** Set when the confirmed dates were read by AI. */
  ai_label?: AiLabel | null;
  outer_frame?: DateSpan | null;
  not_used: RejectedDates[];
  /** The student's own saved first day of classes (raw override), if any. */
  student_start?: IsoDate | null;
  /** The student's own saved last day of classes (raw override), if any. */
  student_end?: IsoDate | null;
}

export interface EvidenceParam {
  key: string;
  value: string;
}

/**
 * One reason, as a code plus parameters, so the UI can translate it. Param conventions: keys
 * `date`, `start`, `end`, `since`, `until`, `monday` or ending in `_on` hold ISO dates; `week`,
 * `weeks`, `days`, `offset` integers; `source` a TermAnchorSource; `kind` a BreakKind; `reason`
 * a RejectReason; anything else is plain text shown as is.
 */
export interface EvidenceItem {
  code: string;
  params: EvidenceParam[];
}

export type LifecycleState =
  | "upcoming"
  | "current"
  | "finishing"
  | "ended"
  | "inactive"
  | "unknown";

/** Ended and Inactive → past; Finishing and Unknown → current. */
export type CourseGroup = "current" | "upcoming" | "past";

export interface CourseLifecycle {
  state: LifecycleState;
  group: CourseGroup;
  confidence: Confidence;
  since?: IsoDate | null;
  /** First day of classes, for "Starts Jan 11" (Upcoming courses). */
  starts_on?: IsoDate | null;
  last_activity?: IsoDate | null;
  next_event?: IsoDate | null;
  evidence_items: EvidenceItem[];
  suggest_removal: boolean;
  /** "I'm still taking this": the course counts as current until this date. */
  kept_current_until?: IsoDate | null;
}

declare module "../generated" {
  interface CourseTimeline {
    phase: CoursePhase;
    phase_confidence: Confidence;
    /** The week features and the week view use by default. */
    default_week?: number | null;
    /** Set during a break that isn't numbered: the last teaching week before it. */
    break_after_week?: number | null;
    /** Set during the exam period: the last teaching week. */
    last_teaching_week?: number | null;
    /** Set during a break. */
    current_break_kind?: BreakKind | null;
    /** How far the professor's (non-bulk) materials have got. */
    notes_week?: number | null;
    term: TermResolution;
    calendar: CalendarStatus;
    evidence_items: EvidenceItem[];
  }

  interface CourseSummary {
    lifecycle: CourseLifecycle;
  }

  interface CourseOverview {
    lifecycle: CourseLifecycle;
  }
}
