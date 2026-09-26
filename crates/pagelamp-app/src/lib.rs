//! Application services — the ONE facade that user interfaces call.
//!
//! Both `apps/pagelamp-cli` and the desktop app (`apps/desktop/src-tauri`, Tauri commands)
//! call only this crate (plus `pagelamp_core::model` / `pagelamp_core::views` types). This
//! keeps the backend/frontend contract in one place: if the desktop app needs something, it
//! is added here first.
//!
//! All public types returned here derive `Serialize + JsonSchema`; `pagelamp schema` (CLI)
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
//! - read views shared with the MCP server — implemented in `pagelamp_core::views`.
//! - course settings: AI policy, term override, hidden.
//! - MCP client configuration snippets for Claude Desktop, Claude Code, Codex (and generic).
//!
//! Every method returns `Result<T, AppError>`; `AppError` serialises as
//! `{ "kind": "...", "message": "..." }` and its message never contains a secret.

pub mod diagnostics;
mod lock;
mod mcp_config;
mod sync;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::NaiveDate;
use pagelamp_canvas::CanvasConfig;
use pagelamp_core::model::{
    AiMaterialsState, AiPolicy, SearchHit, SourceErrorKind, SourceKind, SourceRecord, StoreCounts,
    StoredStudyPlan, TermSource, Timestamp,
};
use pagelamp_core::paths;
use pagelamp_core::secrets::{KeychainSecrets, SecretBackend};
use pagelamp_core::source::{CourseSyncSummary, SourceError};
use pagelamp_core::store::Store;
use pagelamp_core::views::{self, AsOf, CourseOverview, CourseSummary, Deadline, WeekMaterials};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

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

impl From<pagelamp_core::Error> for AppError {
    fn from(err: pagelamp_core::Error) -> Self {
        use pagelamp_core::Error as E;
        let kind = match &err {
            E::NotFound(_) | E::NotInitialised(_) => AppErrorKind::NotFound,
            E::Ambiguous { .. } => AppErrorKind::Ambiguous,
            E::Invalid(_) => AppErrorKind::Invalid,
            E::Db(_)
            | E::Json(_)
            | E::Io(_)
            | E::SchemaTooNew { .. }
            | E::NoDataDir
            | E::Secret(_) => AppErrorKind::Internal,
        };
        // Core error messages never contain secrets (the store never sees them and
        // `secrets` redacts keychain errors).
        AppError::new(kind, err.to_string())
    }
}

impl From<SourceError> for AppError {
    fn from(err: SourceError) -> Self {
        let kind = match err.kind {
            SourceErrorKind::AuthExpiredOrRevoked => AppErrorKind::Auth,
            SourceErrorKind::Network | SourceErrorKind::RateLimited => AppErrorKind::Network,
            SourceErrorKind::NotFound => AppErrorKind::NotFound,
            SourceErrorKind::Other => AppErrorKind::Internal,
        };
        AppError::new(kind, err.message)
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

// ---------------------------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AppStatus {
    /// Version of this PageLamp build.
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
    /// Download LMS (Canvas) files and index their text. Default FALSE: downloading a file
    /// through Canvas counts as viewing it (it can complete "must view" module requirements
    /// and shows up in instructor analytics), so files are listed as `not_downloaded` unless
    /// the student explicitly asks — see `App::download_course_files`. Folder sources are
    /// local and always indexed.
    pub download_files: bool,
    /// Skip LMS files larger than this many megabytes.
    pub max_file_mb: u32,
    /// Only sync these courses (ids or codes); empty = all.
    pub only_courses: Vec<String>,
}

impl Default for SyncRequest {
    fn default() -> Self {
        SyncRequest {
            download_files: false,
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
    /// Per-course details (Canvas; empty for other sources).
    pub course_summaries: Vec<CourseSyncSummary>,
    /// HTTP requests made (Canvas; None for other sources).
    pub requests: Option<u32>,
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
    /// Absolute path of the `pagelamp` binary.
    pub command: String,
    pub args: Vec<String>,
    /// Extra environment (only `PAGELAMP_HOME` when the data dir is not the default).
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
    /// The snippet sets PAGELAMP_HOME because a non-default data directory is in use.
    CustomDataDir,
    /// Generic stdio MCP client: adapt the command/args/env to that client's config format.
    GenericStdioClient,
    /// PageLamp runs from a place its binary won't be found at later (macOS: the disk image
    /// or an App Translocation copy → move it to Applications and copy again; Linux: inside an
    /// AppImage → use the .deb/.rpm or the command-line archive). Always the first note.
    RunFromTemporaryLocation,
}

// ---------------------------------------------------------------------------------------------
// The facade
// ---------------------------------------------------------------------------------------------

/// Cheap to clone; holds the data directory and the secret backend. Every call opens its
/// own short-lived SQLite connection (see docs/ARCHITECTURE.md §4), so one `App` can be shared
/// by all threads/tasks without a mutex.
#[derive(Clone)]
pub struct App {
    data_dir: PathBuf,
    secrets: Arc<dyn SecretBackend>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("data_dir", &self.data_dir)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Default data dir (`PAGELAMP_HOME` respected). Creates it and migrates the DB.
    pub fn open() -> Result<App> {
        App::open_at(paths::data_dir()?)
    }

    /// Explicit data dir (tests, portable installs). Creates it and migrates the DB.
    pub fn open_at(data_dir: PathBuf) -> Result<App> {
        App::open_at_with_secrets(data_dir, Arc::new(KeychainSecrets))
    }

    /// Like `open_at`, with a custom secret backend — for tests and embedders
    /// (`pagelamp_core::secrets::MemorySecrets` keeps everything in memory).
    pub fn open_at_with_secrets(data_dir: PathBuf, secrets: Arc<dyn SecretBackend>) -> Result<App> {
        paths::ensure_dirs_in(&data_dir).map_err(|err| {
            AppError::new(
                AppErrorKind::Internal,
                format!(
                    "could not create the data folder {}: {err}",
                    data_dir.display()
                ),
            )
        })?;
        let app = App { data_dir, secrets };
        Store::open(&app.db_path())?; // create + migrate, then close
        Ok(app)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `<data_dir>/pagelamp.db`
    pub fn db_path(&self) -> PathBuf {
        paths::db_path_in(&self.data_dir)
    }

    fn read_store(&self) -> Result<Store> {
        Ok(Store::open_read_only(&self.db_path())?)
    }

    fn write_store(&self) -> Result<Store> {
        Ok(Store::open(&self.db_path())?)
    }

    // ----- status & sources ----------------------------------------------------------------

    pub fn status(&self) -> Result<AppStatus> {
        let store = self.read_store()?;
        let sources = store.list_sources()?;
        Ok(AppStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            data_dir: self.data_dir.display().to_string(),
            db_path: self.db_path().display().to_string(),
            last_synced_at: sources.iter().filter_map(|s| s.last_synced_at).max(),
            counts: store.counts()?,
            sources,
            sync_in_progress: lock::is_locked(&paths::sync_lock_path_in(&self.data_dir)),
        })
    }

    pub fn list_sources(&self) -> Result<Vec<SourceRecord>> {
        Ok(self.read_store()?.list_sources()?)
    }

    /// Validates the token (`GET /api/v1/users/self`), stores it in the keychain, creates the
    /// source `canvas:<host>`. Personal-use only — callers must show the notice.
    pub async fn add_canvas_source(&self, base_url: &str, token: &str) -> Result<SourceRecord> {
        let base_url = pagelamp_canvas::normalize_base_url(base_url)
            .map_err(|err| AppError::new(AppErrorKind::Invalid, err.message))?;
        let config = CanvasConfig {
            base_url: base_url.clone(),
            token: non_empty_secret(token, "Canvas access token")?,
        };
        // The display name only (never email or ids) — shown as "Connected as …".
        let account_name = pagelamp_canvas::check_token(&config).await?;
        let id = pagelamp_canvas::source_id(&base_url);
        let label = base_url
            .split_once("://")
            .map_or(base_url.as_str(), |(_, host)| host)
            .to_string();
        self.save_source_with_secret(
            SourceRecord {
                id,
                kind: SourceKind::Canvas,
                label,
                config: json!({ "base_url": base_url, "account_name": account_name }),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            },
            &config.token,
        )
    }

    /// Adds a local course folder (`<root>/<COURSE>/…`). Adding the same folder again returns
    /// the existing source (updating its label/term start when given).
    pub fn add_folder_source(
        &self,
        path: &Path,
        term_start: Option<NaiveDate>,
        label: Option<&str>,
    ) -> Result<SourceRecord> {
        let invalid = || {
            AppError::new(
                AppErrorKind::Invalid,
                format!("'{}' is not a folder", path.display()),
            )
        };
        if !path.is_dir() {
            return Err(invalid());
        }
        let root = std::fs::canonicalize(path).map_err(|_| invalid())?;
        let root_text = root.to_string_lossy().to_string();
        let id = format!("folder:{}", short_hash(&root_text));
        let store = self.write_store()?;
        let existing = store.get_source(&id)?;
        let mut config = existing
            .as_ref()
            .map_or_else(|| json!({}), |s| s.config.clone());
        config["path"] = json!(root_text);
        if let Some(start) = term_start {
            config["term_start"] = json!(start.format("%Y-%m-%d").to_string());
        }
        let label = match (label.map(str::trim).filter(|l| !l.is_empty()), &existing) {
            (Some(label), _) => label.to_string(),
            (None, Some(existing)) => existing.label.clone(),
            // The folder's own name: short, and doesn't put the local path (user name) into
            // labels that the MCP server shows to the AI app. The full path is in `config`.
            (None, None) => root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| display_path(&root)),
        };
        store.upsert_source(&SourceRecord {
            id: id.clone(),
            kind: SourceKind::Folder,
            label,
            config,
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })?;
        Ok(store.get_source(&id)?.expect("source was just saved"))
    }

    /// Validates the feed by fetching it, stores the URL in the keychain.
    pub async fn add_ical_source(
        &self,
        feed_url: &str,
        label: Option<&str>,
    ) -> Result<SourceRecord> {
        let feed_url = normalized_feed_url(feed_url)?;
        pagelamp_local::fetch_ical(&feed_url).await?;
        let label = label
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .unwrap_or("Calendar feed")
            .to_string();
        self.save_source_with_secret(
            SourceRecord {
                id: format!("ical:{}", short_hash(&feed_url)),
                kind: SourceKind::Ical,
                label,
                config: json!({}),
                last_synced_at: None,
                last_error: None,
                last_error_kind: None,
            },
            &feed_url,
        )
    }

    /// Removes the source, everything synced from it, and its keychain secret.
    pub fn remove_source(&self, source_id: &str) -> Result<()> {
        let store = self.write_store()?;
        if store.get_source(source_id)?.is_none() {
            return Err(unknown_source(source_id));
        }
        store.remove_source(source_id)?;
        self.secrets.delete(source_id)?;
        Ok(())
    }

    /// Replace an expired/revoked Canvas token or a changed feed URL without removing the
    /// source (remove would cascade to courses + user overrides). Validates like `add_*`,
    /// overwrites the keychain entry, clears last_error/last_error_kind.
    pub async fn update_source_secret(
        &self,
        source_id: &str,
        secret: &str,
    ) -> Result<SourceRecord> {
        let source = self
            .read_store()?
            .get_source(source_id)?
            .ok_or_else(|| unknown_source(source_id))?;
        let mut secret = non_empty_secret(secret, "secret")?;
        let mut account_name = None;
        match source.kind {
            SourceKind::Folder => {
                return Err(AppError::new(
                    AppErrorKind::Invalid,
                    "folder sources have no secret",
                ));
            }
            SourceKind::Canvas => {
                let base_url = canvas_base_url(&source)?;
                // The new token may belong to another account: refresh "Connected as …".
                account_name = Some(
                    pagelamp_canvas::check_token(&CanvasConfig {
                        base_url,
                        token: secret.clone(),
                    })
                    .await?,
                );
            }
            SourceKind::Ical => {
                secret = normalized_feed_url(&secret)?;
                pagelamp_local::fetch_ical(&secret).await?;
            }
        }
        self.secrets.set(source_id, &secret)?;
        let store = self.write_store()?;
        if let Some(name) = account_name {
            let mut updated = source;
            updated.config["account_name"] = json!(name);
            store.upsert_source(&updated)?; // label/config only; sync state untouched
        }
        store.clear_source_error(source_id)?;
        store
            .get_source(source_id)?
            .ok_or_else(|| unknown_source(source_id))
    }

    /// Save `source` and its secret: secret first, and removed again if saving the source
    /// fails, so a secret never exists without its source row.
    fn save_source_with_secret(&self, source: SourceRecord, secret: &str) -> Result<SourceRecord> {
        self.secrets.set(&source.id, secret)?;
        let saved = self.write_store().and_then(|store| {
            store.upsert_source(&source)?;
            Ok(store
                .get_source(&source.id)?
                .expect("source was just saved"))
        });
        if saved.is_err() {
            // Best effort; the original error is the one to report.
            let _ = self.secrets.delete(&source.id);
        }
        saved
    }

    // ----- read views ------------------------------------------------------------------------
    // Hidden courses are included/addressable here (the desktop app shows them behind a
    // toggle); the MCP server calls the views with `include_hidden = false`.

    /// All courses INCLUDING hidden ones (check `course.hidden`).
    pub fn list_courses(&self) -> Result<Vec<CourseSummary>> {
        Ok(views::list_courses(
            &self.read_store()?,
            true,
            AsOf::now_local(),
        )?)
    }

    pub fn course_overview(&self, course: &str) -> Result<CourseOverview> {
        Ok(views::course_overview(
            &self.read_store()?,
            course,
            true,
            AsOf::now_local(),
        )?)
    }

    pub fn week_materials(&self, course: &str, week: Option<u32>) -> Result<WeekMaterials> {
        Ok(views::week_materials(
            &self.read_store()?,
            course,
            week,
            true,
            AsOf::now_local(),
        )?)
    }

    /// With a course: that course's events (hidden courses too). Without: every non-hidden
    /// course's events plus events not linked to a course.
    pub fn list_deadlines(
        &self,
        course: Option<&str>,
        days_ahead: u32,
        days_back: u32,
    ) -> Result<Vec<Deadline>> {
        let store = self.read_store()?;
        Ok(views::deadlines(
            &store,
            course,
            days_ahead,
            days_back,
            true,
            AsOf::now_local(),
        )?)
    }

    /// The student's own search over every non-hidden course (whatever its AI access).
    pub fn search(&self, query: &str, course: Option<&str>, limit: u32) -> Result<Vec<SearchHit>> {
        Ok(views::search(&self.read_store()?, query, course, limit)?)
    }

    pub fn latest_study_plan(&self) -> Result<Option<StoredStudyPlan>> {
        Ok(self.read_store()?.latest_study_plan()?)
    }

    // ----- course settings (course = id or code; hidden courses are addressable) -------------

    pub fn set_course_policy(
        &self,
        course: &str,
        policy: AiPolicy,
        note: Option<&str>,
    ) -> Result<()> {
        let store = self.write_store()?;
        let course = store.resolve_course_with(course, true)?;
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        Ok(store.set_course_policy(&course.id, policy, note)?)
    }

    /// Set/clear the student's term override (`None, None` falls back to the synced dates).
    pub fn set_course_term(
        &self,
        course: &str,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
    ) -> Result<()> {
        let store = self.write_store()?;
        let course = store.resolve_course_with(course, true)?;
        Ok(store.set_course_term(&course.id, start, end)?)
    }

    /// The per-course switch "Let my AI app read this course's materials" (docs/ARCHITECTURE.md
    /// §3 rule 8). A `prohibited` AI policy withholds text regardless of this switch.
    pub fn set_course_ai_access(&self, course: &str, allowed: bool) -> Result<()> {
        let store = self.write_store()?;
        let course = store.resolve_course_with(course, true)?;
        Ok(store.set_course_ai_access(&course.id, allowed)?)
    }

    pub fn set_course_hidden(&self, course: &str, hidden: bool) -> Result<()> {
        let store = self.write_store()?;
        let course = store.resolve_course_with(course, true)?;
        Ok(store.set_course_hidden(&course.id, hidden)?)
    }

    // ----- diagnostics (see the `diagnostics` module; these use this App's data dir) ----------

    /// `<data_dir>/logs` (created if missing) — for "Open logs folder".
    pub fn logs_dir(&self) -> Result<PathBuf> {
        diagnostics::logs_dir_in(&self.data_dir)
    }

    pub fn last_crash(&self) -> Result<Option<diagnostics::CrashReport>> {
        Ok(pagelamp_core::diagnostics::last_crash(&self.data_dir)?)
    }

    pub fn clear_last_crash(&self) -> Result<()> {
        Ok(pagelamp_core::diagnostics::clear_last_crash(
            &self.data_dir,
        )?)
    }

    pub fn doctor(&self) -> Result<diagnostics::DoctorReport> {
        Ok(diagnostics::doctor_in(
            &self.data_dir,
            self.secrets.as_ref(),
        ))
    }

    /// Markdown for an issue (doctor + last crash + recent log lines), redacted and with
    /// course names pseudonymised. Shown to the student before they share it.
    pub fn diagnostic_report(&self) -> Result<String> {
        Ok(diagnostics::report_in(
            &self.data_dir,
            self.secrets.as_ref(),
        ))
    }

    // ----- "connect your AI app" ------------------------------------------------------------

    /// One config per client (claude_desktop, claude_code, codex, generic) for launching
    /// `<pagelamp_binary> mcp`. `PAGELAMP_HOME` is included only when this App's data dir
    /// is not the platform default.
    pub fn mcp_client_configs(&self, pagelamp_binary: &Path) -> Vec<McpClientConfig> {
        let launch = self.mcp_launch(pagelamp_binary);
        let temporary =
            mcp_config::temporary_location(&launch.command, std::env::var_os("APPIMAGE").is_some());
        mcp_config::client_configs(
            &launch,
            mcp_config::Shell::current(),
            mcp_config::claude_desktop_config_hint(),
            temporary,
        )
    }

    /// How an MCP client launches this App's server (the source of every snippet).
    pub fn mcp_launch(&self, pagelamp_binary: &Path) -> McpLaunch {
        let command =
            std::path::absolute(pagelamp_binary).unwrap_or_else(|_| pagelamp_binary.to_path_buf());
        let mut env = BTreeMap::new();
        if !same_dir(Some(&self.data_dir), paths::platform_data_dir().as_deref()) {
            env.insert(
                paths::HOME_ENV.to_string(),
                self.data_dir.display().to_string(),
            );
        }
        McpLaunch {
            command: command.display().to_string(),
            args: vec!["mcp".to_string()],
            env,
        }
    }
}

// ----- facade helpers ---------------------------------------------------------------------------

fn unknown_source(source_id: &str) -> AppError {
    AppError::new(AppErrorKind::NotFound, format!("no source '{source_id}'"))
}

/// Trimmed secret, or `Invalid` when empty. The message names the field, never the value.
fn non_empty_secret(secret: &str, what: &str) -> Result<String> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(AppError::new(
            AppErrorKind::Invalid,
            format!("the {what} is empty"),
        ));
    }
    Ok(secret.to_string())
}

/// A usable feed URL (`webcal://` → `https://`), or `Invalid` (the message never repeats it).
fn normalized_feed_url(input: &str) -> Result<String> {
    let input = non_empty_secret(input, "calendar feed URL")?;
    pagelamp_local::normalize_feed_url(&input)
        .map_err(|err| AppError::new(AppErrorKind::Invalid, err.message))
}

fn canvas_base_url(source: &SourceRecord) -> Result<String> {
    source
        .config
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::new(
                AppErrorKind::Internal,
                format!("source '{}' has no base_url", source.id),
            )
        })
}

/// First 12 hex chars of SHA-256 — short, stable source ids that don't reveal the input
/// (feed URLs are secrets).
fn short_hash(text: &str) -> String {
    pagelamp_core::ingest::sha256_hex(text.as_bytes())[..12].to_string()
}

/// `path` with the home directory shortened to `~` (labels only).
fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Same directory, comparing canonical forms when they exist.
fn same_dir(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a == b
                || matches!(
                    (std::fs::canonicalize(a), std::fs::canonicalize(b)),
                    (Ok(a), Ok(b)) if a == b
                )
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// JSON Schema export (`pagelamp schema`)
// ---------------------------------------------------------------------------------------------

/// Container whose only purpose is to pull every facade type into one schema document
/// (`$defs`), so json-schema-to-typescript emits one TS file with all of them.
#[allow(dead_code)]
#[derive(JsonSchema)]
#[schemars(title = "PageLampAppTypes")]
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
    doctor_report: diagnostics::DoctorReport,
    crash_report: diagnostics::CrashReport,
    process_kind: diagnostics::ProcessKind,
    course_sync_summary: CourseSyncSummary,
}

/// JSON Schema (draft 2020-12) of every type crossing the facade, as one document.
pub fn json_schema() -> serde_json::Value {
    let mut settings = schemars::generate::SchemaSettings::draft2020_12();
    settings.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".into());
    let schema = settings.into_generator().into_root_schema_for::<AppTypes>();
    schema.to_value()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The desktop app and the CLI both use `App::open()`, i.e. `paths::data_dir()`; config
    /// snippets for the default location must therefore NOT pin PAGELAMP_HOME, so the MCP
    /// server started by an AI app resolves the same folder by itself.
    #[test]
    fn default_data_dir_needs_no_pagelamp_home() {
        let temp = tempfile::tempdir().unwrap();
        let custom = temp.path().join("data");
        std::fs::create_dir_all(&custom).unwrap();
        assert!(same_dir(Some(&custom), Some(&custom)));
        assert!(same_dir(Some(&custom), Some(&temp.path().join("data/./"))));
        assert!(!same_dir(Some(&custom), Some(temp.path())));
        assert!(!same_dir(Some(&custom), None));

        let app = App {
            data_dir: custom.clone(),
            secrets: Arc::new(pagelamp_core::secrets::MemorySecrets::new()),
        };
        let launch = app.mcp_launch(Path::new("/demo/pagelamp"));
        let expected_env = !same_dir(Some(&custom), paths::platform_data_dir().as_deref());
        assert_eq!(launch.env.contains_key(paths::HOME_ENV), expected_env);
    }
}
