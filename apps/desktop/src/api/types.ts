// The backend ⇄ frontend contract, as TypeScript.
//
// All shapes come from `generated.ts`, which `pnpm gen:types` produces from the Rust facade's
// JSON Schema (crates/studentos-app). Never hand-write a contract type here — change the Rust
// type and regenerate. This file only re-exports them and adds a few UI conveniences.
//
// Conventions (serde on the Rust side):
// - Option<T> fields are `field?: T | null`; serde always sends null, but handle both (`??`).
// - Instants are RFC 3339 strings; calendar dates are "YYYY-MM-DD" strings.

export type {
  AiPolicy,
  AppError,
  AppErrorKind,
  AppStatus,
  Confidence,
  Course,
  CourseCounts,
  CourseOverview,
  CourseSummary,
  CourseTimeline,
  Deadline,
  EventKind,
  InstallKind,
  MaterialKind,
  MaterialView,
  McpClient,
  McpClientConfig,
  McpLaunch,
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
  WeekMaterials,
} from "./generated";

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
