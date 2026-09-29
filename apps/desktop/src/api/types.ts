// The backend ⇄ frontend contract, as TypeScript.
//
// All shapes come from `generated.ts`, which `pnpm gen:types` produces from the Rust facade's
// JSON Schema (crates/pagelamp-app). Never hand-write a contract type here — change the Rust
// type and regenerate. This file only re-exports them and adds a few UI conveniences.
//
// Conventions (serde on the Rust side):
// - Option<T> fields are `field?: T | null`; serde always sends null, but handle both (`??`).
// - Instants are RFC 3339 strings; calendar dates are "YYYY-MM-DD" strings.

export type {
  Activity,
  ActivityItem,
  ActivityKind,
  AiLabel,
  AiMaterialsState,
  AiPolicy,
  AppError,
  AppErrorKind,
  AppStatus,
  BreakKind,
  CalendarBreak,
  CalendarOrigin,
  CalendarStatus,
  Confidence,
  Course,
  CourseCounts,
  CourseGroup,
  CourseLifecycle,
  CourseOverview,
  CoursePhase,
  CourseSummary,
  CourseTimeline,
  CrashReport,
  DateSpan,
  Deadline,
  DownloadBlock,
  EventKind,
  EvidenceCode,
  EvidenceItem,
  EvidenceParam,
  EvidenceSignal,
  InstallKind,
  LifecycleState,
  MaterialKind,
  MaterialView,
  McpClient,
  McpClientConfig,
  McpLaunch,
  McpNoteCode,
  Module,
  RejectedDates,
  RejectReason,
  SearchHit,
  SourceErrorKind,
  SourceKind,
  SourceRecord,
  SourceSyncResult,
  StartupTasks,
  StoreCounts,
  StoredStudyPlan,
  StudyPlan,
  StudyPlanItem,
  SyncEvent,
  SyncRequest,
  SyncSummary,
  TeachingSegment,
  TemporaryLocation,
  TermAnchorSource,
  TermResolution,
  TermSource,
  TextStatus,
  UpdateChannel,
  UpdateCheckOutcome,
  UpdateCheckRecord,
  UpdatePrefs,
  WeekMaterials,
  WeekNoteKind,
  WhatsNew,
  WhatsNewTopic,
} from "./generated";

import type { AiMaterialsState, AiPolicy, Course } from "./generated";

/** RFC 3339 instant, e.g. "2026-09-25T14:03:00Z". */
export type Timestamp = string;
/** Calendar date, "YYYY-MM-DD". */
export type IsoDate = string;

/** All AI policies in the order the policy editor lists them. */
export const AI_POLICIES: readonly AiPolicy[] = [
  "unknown",
  "prohibited",
  "learning_aid",
  "allowed_with_citation",
  "unrestricted",
];

/**
 * Effective AI access to a course's material text (§3 rule 8): "No AI" wins over the switch,
 * then the switch, else readable ("Not set" counts as readable). The backend computes this;
 * the mock uses this function so both agree.
 */
export function aiMaterialsState(
  course: Pick<Course, "ai_policy" | "ai_access">,
): AiMaterialsState {
  if (course.ai_policy === "prohibited") return "withheld_by_policy";
  if (!course.ai_access) return "turned_off";
  return "readable";
}
