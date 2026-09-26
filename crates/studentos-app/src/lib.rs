//! Application services — the ONE facade that user interfaces call.
//!
//! Both `apps/studentos-cli` and the desktop app (`apps/desktop/src-tauri`, Tauri commands)
//! call only this crate (plus `studentos_core::model` / `studentos_core::views` types). This
//! keeps the backend/frontend contract in one place: if the desktop app needs something, it
//! is added here first.
//!
//! All public types returned here derive `Serialize + JsonSchema`; `studentos schema` (CLI)
//! prints them as one JSON Schema document (`json_schema()`) so the frontend can generate
//! TypeScript types (json-schema-to-typescript) instead of hand-writing them.
//!
//! Responsibilities:
//! - source management: add/remove Canvas (validates token, stores it in the keychain),
//!   folder, iCal (feed URL in keychain); removing a source deletes its secret;
//!   `update_source_secret` replaces an expired token / changed feed URL in place.
//! - sync orchestration: `sync_all` / `sync_source` run the right source crate, record the
//!   outcome with `Store::record_sync`, and emit progress events. An advisory lock file
//!   (`<data_dir>/sync.lock`) prevents CLI and desktop app from syncing concurrently
//!   (`AppErrorKind::Busy`).
//! - read views shared with the MCP server — implemented in `studentos_core::views`.
//! - course settings: AI policy, term override, hidden.
//! - MCP client configuration snippets for Claude Desktop, Claude Code, Codex (and generic).
//!
//! Every method returns `Result<T, AppError>`; `AppError` serialises as
//! `{ "kind": "...", "message": "..." }` and its message never contains a secret.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studentos_core::model::{
    AiMaterialsState, AiPolicy, SearchHit, SourceErrorKind, SourceKind, SourceRecord, StoreCounts,
    StoredStudyPlan, TermSource, Timestamp,
};
use studentos_core::views::{CourseOverview, CourseSummary, Deadline, WeekMaterials};

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

/// What kind of failure happened; UIs branch on this, never on `message`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorKind {
    /// Token/feed URL rejected (expired, revoked, wrong).
    Auth,
    /// Could not reach the server (DNS, TLS, timeout, refused).
    Network,
    /// Bad input from the user (malformed URL/date, folder is not a directory, …).
    Invalid,
    /// Course/source/material does not exist.
    NotFound,
    /// A course reference matched several courses; `message` lists them.
    Ambiguous,
    /// Another process (CLI or desktop app) is syncing (holds `sync.lock`).
    Busy,
    /// Anything else (database, keychain, I/O, bugs).
    Internal,
}

/// Error returned by every facade method. Serialised as-is by the Tauri commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AppError {
    pub kind: AppErrorKind,
    /// User-presentable; never contains a secret.
    pub message: String,
}

impl AppError {
    pub fn new(kind: AppErrorKind, message: impl Into<String>) -> Self {
        AppError {
            kind,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl From<studentos_core::Error> for AppError {
    fn from(err: studentos_core::Error) -> Self {
        let _ = err;
        todo!(
            "NotFound/NotInitialised → not_found, Ambiguous → ambiguous, Invalid → invalid, rest → internal"
        )
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

// ---------------------------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AppStatus {
    /// Version of this StudentOS build.
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
    pub sources: Vec<SourceRecord>,
    pub counts: StoreCounts,
    /// Most recent successful sync over all sources.
    pub last_synced_at: Option<Timestamp>,
    /// True while any process holds `sync.lock`.
    pub sync_in_progress: bool,
}

// ---------------------------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------------------------

/// Options for a sync run. All fields have defaults, so `{}` is a valid request.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SyncRequest {
    /// Download LMS files and index their text (Canvas). When false, files are listed only.
    pub download_files: bool,
    /// Skip LMS files larger than this many megabytes.
    pub max_file_mb: u32,
    /// Only sync these courses (ids or codes); empty = all.
    pub only_courses: Vec<String>,
}

impl Default for SyncRequest {
    fn default() -> Self {
        SyncRequest {
            download_files: true,
            max_file_mb: 50,
            only_courses: Vec::new(),
        }
    }
}

/// Progress stream of a sync run (desktop forwards these through a `tauri::ipc::Channel`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncEvent {
    SourceStarted {
        source_id: String,
        label: String,
    },
    Progress {
        source_id: String,
        message: String,
        current: Option<u32>,
        total: Option<u32>,
    },
    Warning {
        source_id: String,
        message: String,
    },
    SourceFinished {
        source_id: String,
        ok: bool,
        error: Option<String>,
        error_kind: Option<SourceErrorKind>,
    },
}

/// Outcome of syncing one source. A failing source is `ok: false` (not an `Err`), so
/// `sync_all` can report partial success.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourceSyncResult {
    pub source_id: String,
    pub label: String,
    pub kind: SourceKind,
    pub ok: bool,
    pub error: Option<String>,
    pub error_kind: Option<SourceErrorKind>,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    pub courses: u32,
    pub modules: u32,
    pub materials: u32,
    pub files_downloaded: u32,
    pub files_indexed: u32,
    pub events: u32,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SyncSummary {
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
    /// True when every source synced successfully.
    pub ok: bool,
    pub results: Vec<SourceSyncResult>,
}

// ---------------------------------------------------------------------------------------------
// "Connect your AI app"
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpClient {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Generic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    /// Merge `content` into a JSON config file.
    JsonSnippet,
    /// Run `content` in a terminal.
    ShellCommand,
    /// Append `content` to a TOML config file.
    TomlSnippet,
}

/// How an MCP client launches the server — the single source every snippet (and, later,
/// a `.mcpb` Desktop Extension manifest) is generated from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpLaunch {
    /// Absolute path of the `studentos` binary.
    pub command: String,
    pub args: Vec<String>,
    /// Extra environment (only `STUDENTOS_HOME` when the data dir is not the default).
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct McpClientConfig {
    pub client: McpClient,
    /// e.g. "Claude Desktop".
    pub title: String,
    pub install_kind: InstallKind,
    /// Where the snippet goes, e.g. "~/Library/Application Support/Claude/claude_desktop_config.json".
    pub config_path_hint: Option<String>,
    /// The snippet / command to copy.
    pub content: String,
    /// Facts the student should know (plan availability, restart the app, …), in English.
    pub notes: Vec<String>,
    /// Machine-readable code of each entry of `notes` (same length, same order) so UIs can
    /// localise.
    pub note_codes: Vec<McpNoteCode>,
    pub launch: McpLaunch,
}

/// Stable codes for `McpClientConfig.notes` (one code per note).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpNoteCode {
    /// Claude Desktop: works on every Claude plan, including Free.
    WorksOnAllClaudePlans,
    /// Claude Desktop: admins of Team/Enterprise/Education workspaces can disable extensions.
    AdminsMayDisableExtensions,
    /// Claude Code: needs a paid Claude plan (Pro or higher).
    NeedsPaidClaudePlan,
    /// Codex: the ChatGPT desktop app (Work/Codex mode) reads the same ~/.codex/config.toml.
    CodexConfigSharedWithChatgptDesktop,
    /// Codex: documented for ChatGPT Plus and higher, plus Edu.
    CodexPlusAndEduDocumented,
    /// Codex: Free/Go support is undocumented.
    FreeGoUndocumented,
    /// Restart / reload the AI app after changing its config.
    RestartClientAfterChange,
    /// The snippet sets STUDENTOS_HOME because a non-default data directory is in use.
    CustomDataDir,
    /// Generic stdio MCP client: adapt the command/args/env to that client's config format.
    GenericStdioClient,
}

// ---------------------------------------------------------------------------------------------
// The facade
// ---------------------------------------------------------------------------------------------

/// Cheap to clone; holds only the data directory. Every call opens its own short-lived
/// SQLite connection (see docs/ARCHITECTURE.md §4).
#[derive(Clone, Debug)]
pub struct App {
    data_dir: PathBuf,
}

impl App {
    /// Default data dir (`STUDENTOS_HOME` respected). Creates it and migrates the DB.
    pub fn open() -> Result<App> {
        todo!()
    }

    /// Explicit data dir (tests, portable installs). Creates it and migrates the DB.
    pub fn open_at(data_dir: PathBuf) -> Result<App> {
        let _ = data_dir;
        todo!()
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `<data_dir>/studentos.db`
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("studentos.db")
    }

    // ----- status & sources ----------------------------------------------------------------

    pub fn status(&self) -> Result<AppStatus> {
        todo!()
    }

    pub fn list_sources(&self) -> Result<Vec<SourceRecord>> {
        todo!()
    }

    /// Validates the token (`GET /api/v1/users/self`), stores it in the keychain, creates the
    /// source `canvas:<host>`. Personal-use only — callers must show the notice.
    pub async fn add_canvas_source(&self, base_url: &str, token: &str) -> Result<SourceRecord> {
        let _ = (base_url, token);
        todo!()
    }

    pub fn add_folder_source(
        &self,
        path: &Path,
        term_start: Option<NaiveDate>,
        label: Option<&str>,
    ) -> Result<SourceRecord> {
        let _ = (path, term_start, label);
        todo!()
    }

    /// Validates the feed by fetching it, stores the URL in the keychain.
    pub async fn add_ical_source(
        &self,
        feed_url: &str,
        label: Option<&str>,
    ) -> Result<SourceRecord> {
        let _ = (feed_url, label);
        todo!()
    }

    /// Removes the source, everything synced from it, and its keychain secret.
    pub fn remove_source(&self, source_id: &str) -> Result<()> {
        let _ = source_id;
        todo!()
    }

    /// Replace an expired/revoked Canvas token or a changed feed URL without removing the
    /// source (remove would cascade to courses + user overrides). Validates like `add_*`,
    /// overwrites the keychain entry, clears last_error/last_error_kind.
    pub async fn update_source_secret(
        &self,
        source_id: &str,
        secret: &str,
    ) -> Result<SourceRecord> {
        let _ = (source_id, secret);
        todo!()
    }

    // ----- sync ------------------------------------------------------------------------------

    /// Sync every source in order. Holds `sync.lock` for the whole run (`Busy` if taken).
    pub async fn sync_all(
        &self,
        req: SyncRequest,
        on_event: impl Fn(SyncEvent) + Send + Sync,
    ) -> Result<SyncSummary> {
        let _ = (req, on_event);
        todo!()
    }

    pub async fn sync_source(
        &self,
        source_id: &str,
        req: SyncRequest,
        on_event: impl Fn(SyncEvent) + Send + Sync,
    ) -> Result<SourceSyncResult> {
        let _ = (source_id, req, on_event);
        todo!()
    }

    // ----- read views ------------------------------------------------------------------------

    /// All courses INCLUDING hidden ones (check `course.hidden`); the MCP server never lists
    /// hidden courses.
    pub fn list_courses(&self) -> Result<Vec<CourseSummary>> {
        todo!()
    }

    pub fn course_overview(&self, course: &str) -> Result<CourseOverview> {
        let _ = course;
        todo!()
    }

    pub fn week_materials(&self, course: &str, week: Option<u32>) -> Result<WeekMaterials> {
        let _ = (course, week);
        todo!()
    }

    pub fn list_deadlines(
        &self,
        course: Option<&str>,
        days_ahead: u32,
        days_back: u32,
    ) -> Result<Vec<Deadline>> {
        let _ = (course, days_ahead, days_back);
        todo!()
    }

    pub fn search(&self, query: &str, course: Option<&str>, limit: u32) -> Result<Vec<SearchHit>> {
        let _ = (query, course, limit);
        todo!()
    }

    pub fn latest_study_plan(&self) -> Result<Option<StoredStudyPlan>> {
        todo!()
    }

    // ----- course settings (course = id or code; hidden courses are addressable) -------------

    pub fn set_course_policy(
        &self,
        course: &str,
        policy: AiPolicy,
        note: Option<&str>,
    ) -> Result<()> {
        let _ = (course, policy, note);
        todo!()
    }

    pub fn set_course_term(
        &self,
        course: &str,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
    ) -> Result<()> {
        let _ = (course, start, end);
        todo!()
    }

    /// The per-course switch "Let my AI app read this course's materials" (docs/ARCHITECTURE.md
    /// §3 rule 8). A `prohibited` AI policy withholds text regardless of this switch.
    pub fn set_course_ai_access(&self, course: &str, allowed: bool) -> Result<()> {
        let _ = (course, allowed);
        todo!()
    }

    pub fn set_course_hidden(&self, course: &str, hidden: bool) -> Result<()> {
        let _ = (course, hidden);
        todo!()
    }

    // ----- "connect your AI app" ------------------------------------------------------------

    /// One config per client (claude_desktop, claude_code, codex, generic) for launching
    /// `<studentos_binary> mcp`.
    pub fn mcp_client_configs(&self, studentos_binary: &Path) -> Vec<McpClientConfig> {
        let _ = studentos_binary;
        todo!()
    }
}

// ---------------------------------------------------------------------------------------------
// JSON Schema export (`studentos schema`)
// ---------------------------------------------------------------------------------------------

/// Container whose only purpose is to pull every facade type into one schema document
/// (`$defs`), so json-schema-to-typescript emits one TS file with all of them.
#[allow(dead_code)]
#[derive(JsonSchema)]
#[schemars(title = "StudentOsAppTypes")]
struct AppTypes {
    app_error: AppError,
    app_status: AppStatus,
    source_record: SourceRecord,
    source_error_kind: SourceErrorKind,
    sync_request: SyncRequest,
    sync_event: SyncEvent,
    sync_summary: SyncSummary,
    source_sync_result: SourceSyncResult,
    course_summary: CourseSummary,
    course_overview: CourseOverview,
    week_materials: WeekMaterials,
    deadline: Deadline,
    search_hit: SearchHit,
    stored_study_plan: StoredStudyPlan,
    ai_policy: AiPolicy,
    ai_materials_state: AiMaterialsState,
    term_source: TermSource,
    mcp_client_config: McpClientConfig,
}

/// JSON Schema (draft 2020-12) of every type crossing the facade, as one document.
pub fn json_schema() -> serde_json::Value {
    let mut settings = schemars::generate::SchemaSettings::draft2020_12();
    settings.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".into());
    let schema = settings.into_generator().into_root_schema_for::<AppTypes>();
    schema.to_value()
}
