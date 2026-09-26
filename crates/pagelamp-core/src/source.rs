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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::SourceErrorKind;

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
}

impl SourceError {
    pub fn new(kind: SourceErrorKind, message: impl Into<String>) -> Self {
        SourceError {
            kind,
            message: message.into(),
            invalid_input: false,
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
        SourceError::other(err.to_string())
    }
}

impl From<std::io::Error> for SourceError {
    fn from(err: std::io::Error) -> Self {
        SourceError::other(err.to_string())
    }
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
    },
    /// A non-fatal problem, e.g. "DEMO101: Files tab hidden, used module items only".
    Warning(String),
}

/// Callback receiving progress reports. Must be cheap; called from sync tasks/threads.
pub type ProgressFn<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

/// A `ProgressFn` that ignores everything (tests, callers without a UI).
pub fn no_progress(_: SyncProgress) {}
