//! Read views shared by the App facade (desktop app, CLI) and the MCP server.
//!
//! Every function takes an open `Store` (read-only is enough) plus an `AsOf` ("now" and the
//! student's local calendar date) so results are deterministic in tests. Field names are part
//! of both the frontend contract (via `studentos schema`) and the MCP tool output — rename
//! with care and tell the frontend.
//!
//! Views return raw data. Presentation concerns that only MCP needs (the
//! `<course_material>` wrappers, the `guidance` string, output caps in characters) live in
//! `studentos-mcp`.

use chrono::{DateTime, Local, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::model::*;
use crate::store::Store;

/// Window for "recent" materials/announcements in overviews.
pub const RECENT_DAYS: u32 = 14;
/// Window for "upcoming" deadlines in overviews and course summaries.
pub const UPCOMING_DAYS: u32 = 21;
/// A source whose last successful sync is older than this is reported as stale.
pub const STALE_AFTER_HOURS: i64 = 24;

/// The moment a view is computed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsOf {
    pub now: Timestamp,
    /// The student's local calendar date (drives timeline/week inference).
    pub today: NaiveDate,
}

impl AsOf {
    /// Current instant; `today` in the machine's local timezone.
    pub fn now_local() -> Self {
        let now: DateTime<Utc> = Utc::now();
        AsOf {
            now,
            today: now.with_timezone(&Local).date_naive(),
        }
    }
}

/// A deadline/event enriched with its course's code and name (all `Event` fields are
/// flattened into the same JSON object).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Deadline {
    #[serde(flatten)]
    pub event: Event,
    pub course_code: Option<String>,
    pub course_name: Option<String>,
}

/// Per-course counts shown in course lists.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct CourseCounts {
    pub modules: u32,
    pub materials: u32,
    /// Materials with searchable text.
    pub indexed_materials: u32,
    /// Deadlines due in the next `UPCOMING_DAYS` days.
    pub upcoming_deadlines: u32,
}

/// One row of the course list ("where is each course this week").
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CourseSummary {
    pub course: Course,
    pub timeline: CourseTimeline,
    pub counts: CourseCounts,
    pub next_deadline: Option<Deadline>,
    /// Label of the source this course came from (e.g. "Quercus", "~/Courses").
    pub source_label: String,
    /// When that source last synced successfully (data freshness).
    pub last_synced_at: Option<Timestamp>,
}

/// A material as listed in views (no text; use `read_material` for text).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaterialView {
    pub id: String,
    pub course_id: String,
    pub title: String,
    pub kind: MaterialKind,
    pub module_id: Option<String>,
    pub module_name: Option<String>,
    pub week_hint: Option<u32>,
    pub published_at: Option<Timestamp>,
    /// Where the student can open the original (LMS URL or `file://` URL).
    pub url: Option<String>,
    pub text_status: TextStatus,
    pub text_error: Option<String>,
    /// Number of text chunks (pages/slides/sections) available via `read_material`.
    pub chunk_count: u32,
}

/// Everything needed to answer "what's going on in this course right now".
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CourseOverview {
    pub course: Course,
    pub timeline: CourseTimeline,
    pub current_modules: Vec<Module>,
    /// Materials published in the last `RECENT_DAYS` days, newest first (announcements excluded).
    pub recent_materials: Vec<MaterialView>,
    /// Deadlines due in the next `UPCOMING_DAYS` days, soonest first.
    pub upcoming_deadlines: Vec<Deadline>,
    /// Announcements posted in the last `RECENT_DAYS` days, newest first (titles + ids only).
    pub recent_announcements: Vec<MaterialView>,
    pub source_label: String,
    pub last_synced_at: Option<Timestamp>,
}

/// Materials of one teaching week.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WeekMaterials {
    pub course: Course,
    /// The week actually shown (None when no week could be determined; see `note`).
    pub week: Option<u32>,
    /// The week the caller asked for (None = "current week").
    pub requested_week: Option<u32>,
    pub timeline: CourseTimeline,
    /// Modules belonging to this week.
    pub modules: Vec<Module>,
    pub materials: Vec<MaterialView>,
    /// Every week that has at least one module or material (plus the current week), ascending —
    /// drives the ‹ Week › switcher.
    pub available_weeks: Vec<u32>,
    /// Explains any fallback, e.g. "current week unknown — showing materials of the last 14 days".
    pub note: Option<String>,
}

/// A window of a material's text chunks (pagination via `next_chunk`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MaterialText {
    pub material: MaterialView,
    pub course_code: Option<String>,
    pub chunks: Vec<Chunk>,
    pub from_chunk: u32,
    /// Pass as `from_chunk` to continue; None when the end was reached.
    pub next_chunk: Option<u32>,
    pub total_chunks: u32,
}

/// An announcement with its text.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Announcement {
    pub material: MaterialView,
    pub text: String,
    /// True when `text` was cut to the caller's limit.
    pub truncated: bool,
}

/// A source with a freshness verdict.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourceStatus {
    #[serde(flatten)]
    pub source: SourceRecord,
    /// Never synced, or last successful sync older than `STALE_AFTER_HOURS`.
    pub stale: bool,
}

/// Data freshness overview (MCP `sync_status`, desktop status screen).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SyncStatus {
    pub sources: Vec<SourceStatus>,
    pub counts: StoreCounts,
    /// Most recent successful sync over all sources.
    pub last_synced_at: Option<Timestamp>,
    /// True when there are no sources or any source is stale / failing.
    pub stale: bool,
}

// ---------------------------------------------------------------------------------------------
// View functions
// ---------------------------------------------------------------------------------------------

/// All courses (hidden ones included when `include_hidden`; the desktop app shows them with a
/// toggle, the MCP server never does), ordered like `Store::list_courses`.
pub fn list_courses(store: &Store, include_hidden: bool, at: AsOf) -> Result<Vec<CourseSummary>> {
    let _ = (store, include_hidden, at);
    todo!()
}

/// Timeline of one course from its modules/materials/events (see `timeline::infer_timeline`).
pub fn course_timeline(store: &Store, course: &Course, at: AsOf) -> Result<CourseTimeline> {
    let _ = (store, course, at);
    todo!()
}

/// `course` is resolved with `Store::resolve_course` (hidden courses excluded).
pub fn course_overview(store: &Store, course: &str, at: AsOf) -> Result<CourseOverview> {
    let _ = (store, course, at);
    todo!()
}

/// Materials of `week` (default: the inferred current week). A material belongs to week N
/// when its `week_hint` is N; or it has no week_hint and its module's week_hint is N; or it
/// has neither and the course term start is known and it was published during week N.
pub fn week_materials(
    store: &Store,
    course: &str,
    week: Option<u32>,
    at: AsOf,
) -> Result<WeekMaterials> {
    let _ = (store, course, week, at);
    todo!()
}

/// Events with `when()` in [now - days_back, now + days_ahead], soonest first, optionally for
/// one course. Events of hidden courses are excluded.
pub fn deadlines(
    store: &Store,
    course: Option<&str>,
    days_ahead: u32,
    days_back: u32,
    at: AsOf,
) -> Result<Vec<Deadline>> {
    let _ = (store, course, days_ahead, days_back, at);
    todo!()
}

/// Announcements of the last `days` days, newest first; each text capped at `max_chars`.
pub fn announcements(
    store: &Store,
    course: &str,
    days: u32,
    max_chars: usize,
    at: AsOf,
) -> Result<Vec<Announcement>> {
    let _ = (store, course, days, max_chars, at);
    todo!()
}

/// Chunks starting at `from_chunk` until adding the next chunk would exceed `max_chars`
/// (always at least one chunk when any remain). Materials of hidden courses → `NotFound`.
pub fn read_material(
    store: &Store,
    material_id: &str,
    from_chunk: u32,
    max_chars: usize,
) -> Result<MaterialText> {
    let _ = (store, material_id, from_chunk, max_chars);
    todo!()
}

/// Full-text search; `course` (optional) is resolved with `Store::resolve_course`.
pub fn search(
    store: &Store,
    query: &str,
    course: Option<&str>,
    limit: u32,
) -> Result<Vec<SearchHit>> {
    let _ = (store, query, course, limit);
    todo!()
}

pub fn sync_status(store: &Store, at: AsOf) -> Result<SyncStatus> {
    let _ = (store, at);
    todo!()
}
