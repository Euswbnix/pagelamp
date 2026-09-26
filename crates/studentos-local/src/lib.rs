//! LMS-independent sources. These need no LMS API access, so they are safe to ship to any
//! student at any school (Canvas, Brightspace, Moodle…).
//!
//! ## Folder source (`folder:<id>`)
//! Layout convention: `<root>/<COURSE>/**`. Each top-level directory is a course:
//! - course code = leading token matching `[A-Z]{2,4}\s?\d{3}[A-Z0-9]*` in the dir name,
//!   case-insensitive, upper-cased without the space (e.g. "DEMO101H1 Intro" → "DEMO101H1",
//!   "demo 101 notes" → "DEMO101"), else None; name = dir name.
//! - optional `<root>/<COURSE>/course.toml` or `course.json` may set `code`, `name`,
//!   `term_start`, `term_end` (YYYY-MM-DD). Source-level `term_start` config is the fallback.
//! - every sub-directory becomes a `Module` (week_hint from its name); nested deeper dirs
//!   belong to their top-level sub-directory's module.
//! - every file becomes a `MaterialUpsert { kind: File }` (title = file name), week_hint from
//!   its own name, else from the nearest enclosing folder whose name gives a week;
//!   published_at = file mtime; url = `file://` URL; local_path = absolute path.
//!   Unsupported files are recorded with text_status Unsupported (title only) so students
//!   still see them. Hidden files/dirs (".*") and `course.toml/json` are skipped.
//! - ids are derived from the path relative to root, so renaming the root folder is harmless:
//!   course `<source>/course/<dir>`, module `<source>/module/<dir>/<subdir>`, material
//!   `<source>/file/<dir>/<relative path with '/'>`.
//! - A folder that cannot be read never causes pruning (warning instead).
//! - Symlinks are not followed.
//!
//! ## iCal source (`ical:<id>`)
//! The feed URL (e.g. Canvas Calendar → "Calendar Feed") is a secret stored in the keychain.
//! Fetch with GET (`webcal://` = `https://`; timeout 30s, max 10 MB), parse VEVENTs →
//! `Event`s (never storing DESCRIPTION — rule 4):
//! - Canvas puts the course in the SUMMARY suffix, e.g. "Assignment 1 [DEMO101H1 F LEC0101]";
//!   match the bracketed code against known course codes (prefix match, case-insensitive)
//!   to set course_id; strip the bracket from the title.
//! - kind: summary/description mentioning assignment/due → AssignmentDue; quiz → QuizDue;
//!   exam/midterm/final → Exam; else ClassEvent. All-day events: due_at = end of that day in
//!   the local timezone converted to UTC.
//! - Canvas assignment events are all-day-less instants where DTSTART == DTEND: treat as due_at.
//! - Times with a TZID are converted with the IANA time zone database; floating times and
//!   unknown TZIDs are read as local time. RRULEs are not expanded (first occurrence only).
//! - id = `<source>/event/<UID>` (+ `#<RECURRENCE-ID>` for overrides; fallback: hash of
//!   summary+dtstart).
//!
//! Offline fallback: if the fetch fails, keep existing events and return an error so the
//! caller records `last_error`.

mod folder;
mod ical;

use std::path::Path;

use chrono::NaiveDate;
use studentos_core::Store;
use studentos_core::model::{Course, Event};
use studentos_core::source::{ProgressFn, SourceError};

#[derive(Clone, Debug, Default)]
pub struct FolderSyncReport {
    pub courses: usize,
    pub modules: usize,
    pub materials: usize,
    pub files_indexed: usize,
    pub files_unchanged: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct IcalSyncReport {
    pub events: usize,
    pub matched_to_courses: usize,
    pub warnings: Vec<String>,
}

/// Sync a course-folder root into the store under `source_id` (row must exist).
/// Prunes courses/materials that disappeared from disk. `default_term_start` is the
/// source-level fallback (`config.term_start`) for courses without their own course.toml/json.
///
/// Blocking (walks the disk, extracts text): async callers run it inside `spawn_blocking`
/// with their own `Store`. Must not be called inside `Store::in_transaction` (ingest opens
/// its own short transactions). Missing/unreadable root → `SourceErrorKind::NotFound`.
pub fn sync_folder(
    store: &Store,
    source_id: &str,
    root: &Path,
    default_term_start: Option<NaiveDate>,
    progress: ProgressFn<'_>,
) -> Result<FolderSyncReport, SourceError> {
    folder::sync_folder(store, source_id, root, default_term_start, progress)
}

/// Parse iCalendar text into events (pure; used by `sync_ical` and tests). All-day and
/// floating times use the machine's local time zone.
pub fn parse_ical(
    source_id: &str,
    ics: &str,
    known_courses: &[Course],
) -> Result<Vec<Event>, SourceError> {
    ical::parse_ical(source_id, ics, known_courses)
}

/// Check a user-entered feed URL: `webcal://` becomes `https://`; https is required (http
/// only for localhost). The error message never repeats the URL (it is a secret).
pub fn normalize_feed_url(input: &str) -> Result<String, SourceError> {
    ical::normalize_feed_url(input)
}

/// GET the feed (`webcal://` is treated as `https://`), timeout 30 s, at most 10 MB, and check
/// it is iCalendar. Used by `sync_ical` and by the App to validate a feed before saving it.
/// Errors are classified (401/403 → AuthExpiredOrRevoked, 404/410 → NotFound,
/// DNS/TLS/timeout → Network) and their messages NEVER contain the URL (it is a secret).
pub async fn fetch_ical(feed_url: &str) -> Result<String, SourceError> {
    ical::fetch_ical(feed_url).await
}

/// Fetch and sync an iCal feed under `source_id` (row must exist). The DB is opened
/// per unit of work inside `spawn_blocking` (never holds a connection across `.await`, so
/// the future is `Send`). On fetch failure existing events are kept and the error returned.
pub async fn sync_ical(
    db_path: &Path,
    source_id: &str,
    feed_url: &str,
    progress: ProgressFn<'_>,
) -> Result<IcalSyncReport, SourceError> {
    ical::sync_ical(db_path, source_id, feed_url, progress).await
}
