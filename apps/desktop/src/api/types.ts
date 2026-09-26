// The backend ⇄ frontend contract, as TypeScript.
//
// All shapes come from `generated.ts`, which `pnpm gen:types` produces from the Rust facade's
// JSON Schema (crates/studentos-app). Never hand-write a contract type here — change the Rust
// type and regenerate. This file only re-exports them and adds a few UI conveniences.
//
// Conventions (serde on the Rust side):
// - Option<T> fields are `field?: T | null`; serde always sends null, but handle both (`??`).
// - Instants are RFC 3339 strings; calendar dates are "YYYY-MM-DD" strings.

import type * as G from "./generated";

export type {
  AiPolicy,
  AppError,
  AppErrorKind,
  AppStatus,
  Confidence,
  CourseCounts,
  CourseTimeline,
  Deadline,
  EventKind,
  InstallKind,
  MaterialKind,
  MaterialView,
  McpClient,
  McpClientConfig,
  McpLaunch,
  McpNoteCode,
  Module,
  SearchHit,
  SourceErrorKind,
  SourceKind,
  SourceRecord,
  SourceSyncResult,
  StoreCounts,
  StoredStudyPlan,
  StudyPlan,
  StudyPlanItem,
  SyncEvent,
  SyncRequest,
  SyncSummary,
  TextStatus,
  WeekNoteKind,
} from "./generated";

// ─── PENDING SCHEMA FIELDS ─────────────────────────────────────────────────────────────────
// Agreed in docs/ARCHITECTURE.md §3 rule 8 and the §5 additions, but not in `studentos schema`
// yet. The mock already implements them. Once `pnpm gen:types` brings them in, delete this
// block and re-export Course/CourseSummary/CourseOverview/WeekMaterials (and the two enums)
// from "./generated" above — the intersections below then become no-ops.

/** Whether the student's AI app may read a course's material text (computed by the backend). */
export type AiMaterialsState = "readable" | "turned_off" | "withheld_by_policy";

/** Where a course's effective term dates come from. */
export type TermSource = "user" | "synced" | "none";

export type Course = G.Course & {
  /** The per-course switch "Let my AI app read this course's materials" (default true). */
  ai_access: boolean;
  term_source: TermSource;
};

export type CourseSummary = Omit<G.CourseSummary, "course"> & {
  course: Course;
  ai_materials: AiMaterialsState;
};

export type CourseOverview = Omit<G.CourseOverview, "course"> & {
  course: Course;
  ai_materials: AiMaterialsState;
};

export type WeekMaterials = Omit<G.WeekMaterials, "course"> & { course: Course };

// ────────────────────────────────────────────────────────────────────────────────────────────

import type { AiPolicy } from "./generated";

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
