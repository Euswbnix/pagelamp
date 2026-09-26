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

use std::path::{Path, PathBuf};

use studentos_core::source::{ProgressFn, SourceError};

/// Connection settings for token mode. `Debug` is implemented by hand so the token can never
/// end up in logs.
#[derive(Clone)]
pub struct CanvasConfig {
    /// e.g. "https://lms.example.edu" (no trailing slash, no /api/v1); see `normalize_base_url`.
    pub base_url: String,
    pub token: String,
}

impl std::fmt::Debug for CanvasConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasConfig")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct SyncOptions {
    /// Download files and index their text. When false (the default for Canvas — downloads
    /// count as views, see crate docs), files are recorded as NotDownloaded.
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
    /// Non-fatal problems (e.g. "DEMO101: Files tab hidden, used module items only").
    pub warnings: Vec<String>,
}

/// Validate and normalise a user-entered Canvas URL to `scheme://host[:port]` (no path,
/// no trailing slash). https only, except http for localhost/127.0.0.1 (tests).
pub fn normalize_base_url(input: &str) -> Result<String, SourceError> {
    let _ = input;
    todo!()
}

/// Source id for a (normalised) Canvas base URL: `canvas:<host>` (plus `:<port>` if any).
pub fn source_id(base_url: &str) -> String {
    let _ = base_url;
    todo!()
}

/// Validate a token by calling `GET /api/v1/users/self`; returns the user's display name.
pub async fn check_token(config: &CanvasConfig) -> Result<String, SourceError> {
    let _ = config;
    todo!()
}

/// Full sync of the active courses into the store at `db_path`. The source row must already
/// exist. The DB is opened per unit of work inside `spawn_blocking` (the future is `Send`);
/// store writes happen in short transactions per course; the caller records the outcome with
/// `Store::record_sync`.
pub async fn sync(
    db_path: &Path,
    config: &CanvasConfig,
    options: &SyncOptions,
    progress: ProgressFn<'_>,
) -> Result<SyncReport, SourceError> {
    let _ = (db_path, config, options, progress);
    todo!()
}
