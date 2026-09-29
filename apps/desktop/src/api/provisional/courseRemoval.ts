// Provisional facade types for course removal and the course dates form v2 (calendar design
// §4, §7.10, §8.3–§8.7; work item F2), as proposed to backend-2 on 2026-09-28. Written by hand
// so the UI and the mock can be built before B5/B6 land.
//
// Delete this file when `pnpm gen:types` brings the real types into generated.ts: the
// augmentation below then only repeats what generated.ts has, and the named types are
// re-exported from there instead (src/api/types.ts).

import type { BreakKind, CourseLifecycle, SourceKind } from "../generated";

/** Calendar date, "YYYY-MM-DD". */
type IsoDate = string;
/** RFC 3339 instant. */
type Timestamp = string;

export type RemovalReason = "ended" | "inactive" | "not_mine" | "other";

/** What a later sync can't bring back (design §8.3). */
export type LostAfterPurge =
  | "old_announcements"
  | "locked_files"
  | "whole_course"
  | "redownload_counts_as_viewing";

export interface BackupInfo {
  age_days: number;
  /** True when the backup is at least 14 days old (the facade decides). */
  delete_by_default: boolean;
  /** backup_old | backup_recent: why the box is ticked or not. */
  reason_code: string;
}

export interface RemovalPreviewItem {
  course_id: string;
  code?: string | null;
  name: string;
  source_kind: SourceKind;
  lifecycle: CourseLifecycle;
  materials: number;
  downloaded_files: number;
  downloaded_bytes: number;
  deadlines: number;
  generated_items: number;
  custom_settings: boolean;
  /** A folder course: the student's own files are never touched. */
  own_folder_untouched: boolean;
  /** Canvas restricts access to it, so a sync can't bring it back. */
  cannot_sync_again: boolean;
  lost_after_purge: LostAfterPurge[];
}

export interface RemovalPreview {
  items: RemovalPreviewItem[];
  /** The pre-update backup, when there is one (it still holds the courses' text). */
  backup?: BackupInfo | null;
}

export interface RemoveOptions {
  /** null = derived from the lifecycle (ended, inactive, else other). */
  reason?: RemovalReason | null;
  keep_downloaded_files: boolean;
  purge_now: boolean;
  delete_pre_update_backup: boolean;
}

export type RemovedState = "pending" | "purged" | "restoring";

export interface RemovedCourse {
  removed_id: string;
  source_id: string;
  source_kind: SourceKind;
  external_id: string;
  course_id: string;
  code?: string | null;
  name: string;
  reason: RemovalReason;
  state: RemovedState;
  removed_at: Timestamp;
  purge_after?: IsoDate | null;
  purged_at?: Timestamp | null;
  /** Whole days until the local data is deleted (pending only). */
  purge_in_days?: number | null;
  keep_files: boolean;
  /** Moving the downloaded files to the Trash failed; retried later. */
  files_pending: boolean;
}

export interface RemovalReport {
  removed: RemovedCourse[];
  /** "Delete now": the local data is already gone (no undo). */
  purged_now: boolean;
  backup_deleted: boolean;
}

export type RestoreFailure = "not_listed" | "access_restricted" | "offline" | "other";

export interface RestoreOutcome {
  restored: boolean;
  course_id?: string | null;
  failure?: RestoreFailure | null;
}

export interface PurgeReport {
  purged: string[];
  files_pending: string[];
}

export interface BreakInput {
  kind: BreakKind;
  start: IsoDate;
  end: IsoDate;
  numbered: boolean;
  label?: string | null;
}

export interface SegmentInput {
  first_class: IsoDate;
  last_class?: IsoDate | null;
  /** Start again at week 1 (else the numbering continues after the first part). */
  restart_numbering: boolean;
}

export interface CourseDatesInput {
  first_class?: IsoDate | null;
  last_class?: IsoDate | null;
  exams_end?: IsoDate | null;
  breaks: BreakInput[];
  second_segment?: SegmentInput | null;
}

declare module "../generated" {
  interface StoreCounts {
    /** Removed courses (pending and purged) listed under "Removed courses". */
    removed_courses: number;
  }
}
