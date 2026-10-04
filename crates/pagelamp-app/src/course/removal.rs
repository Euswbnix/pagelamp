//! Removing finished courses in two stages (docs/design/v0.3-course-calendar.md §8.3–§8.7).
//!
//! The types come first (the alpha.2 contract agreed with the desktop); the facade methods
//! (`removal_preview`, `remove_courses`, `removed_courses`, `restore_course`,
//! `purge_removed_courses`, `forget_removed_course`) arrive with the schema-v4 tombstones.

use pagelamp_core::model::{CourseLifecycle, SourceKind, Timestamp};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why a course was removed (display only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemovalReason {
    Ended,
    Inactive,
    NotMine,
    Other,
}

/// What a later sync can't bring back once a course is purged (§8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LostAfterPurge {
    /// Announcements older than the sync window (365 days).
    OldAnnouncements,
    /// Files the course may lock after it ends.
    LockedFiles,
    /// The LMS restricts the course by date: it can't be synced again at all.
    WholeCourse,
    /// Downloading the files again counts as viewing them in the LMS.
    RedownloadCountsAsViewing,
}

/// The pre-update backup (`pagelamp.db.v<N>.bak`) still holds the course's text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BackupInfo {
    pub age_days: u32,
    /// "Also delete the pre-update backup" is ticked by default when it is 14+ days old.
    pub delete_by_default: bool,
    /// `backup_old` (safe to delete) or `backup_recent` (the way back from a bad update).
    pub reason_code: String,
}

/// One course in the removal dialog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemovalPreviewItem {
    pub course_id: String,
    pub code: Option<String>,
    pub name: String,
    pub source_kind: SourceKind,
    pub lifecycle: CourseLifecycle,
    pub materials: u32,
    pub downloaded_files: u32,
    pub downloaded_bytes: u64,
    pub deadlines: u32,
    pub generated_items: u32,
    /// The student changed the course's settings (AI policy, access, dates…).
    pub custom_settings: bool,
    /// A folder course: its files are never touched.
    pub own_folder_untouched: bool,
    /// The LMS restricts the course by date, so it can't be synced again.
    pub cannot_sync_again: bool,
    pub lost_after_purge: Vec<LostAfterPurge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemovalPreview {
    pub items: Vec<RemovalPreviewItem>,
    pub backup: Option<BackupInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemoveOptions {
    /// None: from each course's lifecycle (Ended → ended, Inactive → inactive, else other).
    pub reason: Option<RemovalReason>,
    pub keep_downloaded_files: bool,
    /// Delete at once instead of in 7 days (no undo).
    pub purge_now: bool,
    pub delete_pre_update_backup: bool,
}

/// Where a removed course is in the two stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TombstoneState {
    /// Removed; local data kept until `purge_after` (undo possible).
    Pending,
    /// Local data deleted; the name stays so sync doesn't add it back.
    Purged,
    /// Being synced back after a purge.
    Restoring,
}

/// A course in "Removed courses".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemovedCourse {
    /// What `restore_course` / `forget_removed_course` take.
    pub removed_id: String,
    pub source_id: String,
    pub source_kind: SourceKind,
    pub external_id: String,
    pub course_id: String,
    pub code: Option<String>,
    pub name: String,
    pub reason: RemovalReason,
    pub state: TombstoneState,
    pub removed_at: Timestamp,
    /// When the local data will be deleted (pending only).
    pub purge_after: Option<Timestamp>,
    pub purged_at: Option<Timestamp>,
    /// Whole days until the purge ("deleted in 3 days"); None once purged.
    pub purge_in_days: Option<u32>,
    pub keep_files: bool,
    /// Moving the downloaded files to the Trash failed; retried later.
    pub files_pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemovalReport {
    pub removed: Vec<RemovedCourse>,
    pub purged_now: bool,
    pub backup_deleted: bool,
}

/// Why a purged course didn't come back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestoreFailure {
    /// The LMS no longer lists the course.
    NotListed,
    /// The LMS restricts the course by date.
    AccessRestricted,
    Offline,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RestoreOutcome {
    pub restored: bool,
    pub course_id: Option<String>,
    pub failure: Option<RestoreFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PurgeReport {
    /// `removed_id`s purged now.
    pub purged: Vec<String>,
    /// `removed_id`s whose files couldn't be moved to the Trash (kept; retried later).
    pub files_pending: Vec<String>,
}
