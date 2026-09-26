//! Canvas LMS source (read-only).
//!
//! POLICY GUARDRAILS — these are product requirements, not suggestions:
//! - Personal-token mode is for the token owner's own use / development only. Instructure's
//!   OAuth docs state that asking other users to manually generate a token for your app
//!   violates the Canvas API Policy; multi-user distribution must use OAuth2 with an
//!   institution-issued Developer Key (future work).
//! - Only HTTP GET. The client must make it impossible to issue POST/PUT/DELETE
//!   (no submission, quiz answers, discussion posts, "mark done", etc.).
//! - Endpoint allow-list (all under `{base_url}/api/v1`):
//!
//!   ```text
//!   GET /users/self
//!   GET /courses?enrollment_state=active&include[]=term&include[]=syllabus_body&per_page=100
//!   GET /courses/:id/tabs
//!   GET /courses/:id/modules?include[]=items&include[]=content_details&per_page=100
//!   GET /courses/:id/modules/:module_id/items?include[]=content_details&per_page=100 (fallback)
//!   GET /courses/:id/files?per_page=100          (skip if Files tab hidden / 401 / 403)
//!   GET /courses/:id/files/:file_id               (metadata for module file items)
//!   GET /courses/:id/pages?per_page=100 and /courses/:id/pages/:url_or_id (body)
//!   GET /courses/:id/assignments?per_page=100     (name + due_at + html_url ONLY; never
//!                                                  store assignment descriptions)
//!   GET /announcements?context_codes[]=course_:id&start_date=…&per_page=100
//!   GET /planner/items?start_date=…&end_date=…&per_page=100
//!   file download URL from the file object (follow redirects; never forward the
//!     Authorization header to another host)
//!   ```
//!
//! - Canvas is never called from the MCP server; only `studentos sync` calls this crate.
//! - Pagination: follow `Link: <…>; rel="next"` as an opaque URL (must stay on base host).
//! - Throttling: at most 2 concurrent requests; if `X-Rate-Limit-Remaining` < 100 slow down;
//!   on 403 with body containing "Rate Limit Exceeded" or on 429, exponential backoff
//!   (1s, 2s, 4s … max 5 tries).
//! - Tokens come from `studentos_core::secrets` and must never be logged.
//!
//! Mapping to the store (ids per `studentos_core::model` conventions, source id
//! `canvas:<host>`): courses → `CourseUpsert` (term dates from `term.start_at/end_at`,
//! syllabus_body → text); modules → `Module` (week_hint via `timeline::parse_week_hint`);
//! module items of type File/Page/ExternalUrl + course files + pages → `MaterialUpsert`;
//! announcements → `MaterialUpsert { kind: Announcement }` indexed via `ingest::index_html`;
//! assignments/quizzes due dates + planner items → `Event`s.

use std::path::PathBuf;

use studentos_core::Store;

#[derive(Clone, Debug)]
pub struct CanvasConfig {
    /// e.g. "https://q.utoronto.ca" (no trailing slash, no /api/v1)
    pub base_url: String,
    pub token: String,
}

#[derive(Clone, Debug)]
pub struct SyncOptions {
    /// Download files and index their text. When false, files are recorded as NotDownloaded.
    pub download_files: bool,
    /// Skip files larger than this (bytes). Default 50 MB.
    pub max_file_bytes: u64,
    /// Where downloaded files are cached (normally `paths::files_dir()`).
    pub files_dir: PathBuf,
    /// Only sync these course ids/codes (empty = all active courses).
    pub only_courses: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SyncReport {
    pub courses: usize,
    pub modules: usize,
    pub materials: usize,
    pub files_downloaded: usize,
    pub files_indexed: usize,
    pub events: usize,
    /// Non-fatal problems (e.g. "CSC413: Files tab hidden, used module items only").
    pub warnings: Vec<String>,
}

/// Source id for a Canvas base URL: `canvas:<host>`.
pub fn source_id(base_url: &str) -> String {
    let _ = base_url;
    todo!()
}

/// Validate a token by calling `GET /api/v1/users/self`; returns the user's display name.
pub async fn check_token(config: &CanvasConfig) -> anyhow::Result<String> {
    let _ = config;
    todo!()
}

/// Full sync of all active courses into the store. The source row must already exist.
/// Store writes happen in short transactions per course; the caller records the sync
/// outcome with `Store::record_sync`.
pub async fn sync(
    store: &Store,
    config: &CanvasConfig,
    options: &SyncOptions,
) -> anyhow::Result<SyncReport> {
    let _ = (store, config, options);
    todo!()
}
