//! Plumbing shared by every sync source (Canvas, folder, iCal): a typed error whose kind the
//! UI branches on, and a progress callback.
//!
//! Classification rules (docs/ARCHITECTURE.md §5, `SourceErrorKind`): Canvas 401 / invalid
//! token and feed-URL 401/403 → `AuthExpiredOrRevoked`; DNS/TLS/timeout/connection refused →
//! `Network`; missing folder, feed 404 or Canvas host 404 → `NotFound`; Canvas throttling
//! still failing after all retries → `RateLimited`; anything else → `Other`.
//!
//! Messages are shown to the student as-is: make them actionable and NEVER include a secret
//! (token, feed URL) — not even inside a URL.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::SourceErrorKind;

/// A "stop" request for a running sync (`App::cancel_sync`, mac request F4): set once, seen by
/// every clone. The folder sync checks it between files, the Canvas sync between courses and
/// downloads, and the extraction worker's watchdog while a file is read.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> CancelFlag {
        CancelFlag::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// The flag itself, for code that polls it (the extraction worker's parent loop).
    pub fn as_atomic(&self) -> &AtomicBool {
        &self.0
    }
}

/// What one sync did for one course (the `pagelamp sync` summary; desktop status screens).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CourseSyncSummary {
    /// Course code, or name when it has no code.
    pub course: String,
    pub modules: u32,
    pub pages: u32,
    /// Files listed (downloaded or not).
    pub files: u32,
    /// Deadlines/events read for this course.
    pub events: u32,
    pub warnings: u32,
    /// Of `pages`, those no list or module gave: a text PageLamp read links to them (Canvas,
    /// a full sync; `coverage`).
    #[serde(default)]
    pub linked_pages: u32,
    /// Of `files`, those found through a link in a text only.
    #[serde(default)]
    pub linked_files: u32,
    /// How many of the things the sync noted as not read went wrong or are for the student to
    /// act on: a page a module asks them to view, a failed request, what a limit left out,
    /// what Canvas locks, a file Canvas no longer has. Not what PageLamp never reads by rule,
    /// not the hidden lists (named by the two flags below), and not files that aren't
    /// downloaded. `CourseOverview::coverage` lists everything, with reasons.
    #[serde(default)]
    pub not_read: u32,
    /// The course's navigation hides its Pages list: only pages that modules and links lead
    /// to were read.
    #[serde(default)]
    pub pages_hidden: bool,
    /// The same for its Files list.
    #[serde(default)]
    pub files_hidden: bool,
}

/// Why a source failed to sync (or failed validation when being added).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceError {
    pub kind: SourceErrorKind,
    /// User-presentable, secret-free.
    pub message: String,
    /// The entered address itself can't be used (e.g. its server redirects to plain http);
    /// UIs report that as invalid input rather than a failure. `kind` stays `Other`.
    pub invalid_input: bool,
    /// The student stopped the sync (`CancelFlag`): not a failure, and not recorded as one.
    pub cancelled: bool,
}

impl SourceError {
    pub fn new(kind: SourceErrorKind, message: impl Into<String>) -> Self {
        SourceError {
            kind,
            message: message.into(),
            invalid_input: false,
            cancelled: false,
        }
    }
    /// See `cancelled`.
    pub fn cancelled() -> Self {
        SourceError {
            cancelled: true,
            ..Self::other("The sync was stopped.")
        }
    }
    /// See `invalid_input`.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        SourceError {
            invalid_input: true,
            ..Self::other(message)
        }
    }
    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(SourceErrorKind::AuthExpiredOrRevoked, message)
    }
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(SourceErrorKind::Network, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(SourceErrorKind::NotFound, message)
    }
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(SourceErrorKind::RateLimited, message)
    }
    pub fn other(message: impl Into<String>) -> Self {
        Self::new(SourceErrorKind::Other, message)
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

/// Store/IO problems during a sync are `Other` (the message never contains secrets: the
/// store never sees them).
impl From<crate::Error> for SourceError {
    fn from(err: crate::Error) -> Self {
        match err {
            crate::Error::Cancelled => SourceError::cancelled(),
            other => SourceError::other(other.to_string()),
        }
    }
}

impl From<std::io::Error> for SourceError {
    fn from(err: std::io::Error) -> Self {
        SourceError::other(err.to_string())
    }
}

/// What a sync step is doing, as a code the UIs translate (with `course`, `current` and `total`
/// filled in); the step's English `message` stays for the CLI, logs and older clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncStage {
    /// Canvas: "Checking the Canvas token".
    CheckingAccess,
    /// Canvas: "Listing courses".
    ListingCourses,
    /// Canvas: "<course>: reading"; `current`/`total` count courses.
    ReadingCourse,
    /// Canvas: "<course>: downloading files"; `current`/`total` count files.
    DownloadingFiles,
    /// Folder: "<course>: scanning files".
    ScanningFiles,
    /// Folder: "<course>: indexing files"; `current`/`total` count files.
    IndexingFiles,
    /// Calendar feed: "Downloading the calendar feed".
    DownloadingFeed,
    /// Calendar feed: "Saving <total> calendar events".
    SavingEvents,
}

/// A progress report from a running source sync. The App facade turns these into
/// `SyncEvent::Progress` / `SyncEvent::Warning` for the UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncProgress {
    /// What is happening now, e.g. "DEMO101: indexing files", optionally with a counter.
    Step {
        message: String,
        current: Option<u32>,
        total: Option<u32>,
        /// The same step as a code (`None` from sources that don't say).
        stage: Option<SyncStage>,
        /// The course the step is about (its code, else its name), as in `message`.
        course: Option<String>,
    },
    /// A non-fatal problem, e.g. "DEMO101: Lecture 3.mp4 skipped (larger than the download
    /// limit)".
    Warning(String),
}

/// Callback receiving progress reports. Must be cheap; called from sync tasks/threads.
pub type ProgressFn<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

/// A `ProgressFn` that ignores everything (tests, callers without a UI).
pub fn no_progress(_: SyncProgress) {}
