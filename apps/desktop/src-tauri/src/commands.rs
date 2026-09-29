//! Tauri commands — one per facade method, same names (docs/ARCHITECTURE.md §5).
//!
//! Rules for this file:
//! - Each command only converts arguments and calls `App`; no logic, no extra validation.
//! - Errors are the facade's `AppError`, serialised as `{ kind, message }` for the UI.
//! - Secrets (Canvas token, feed URL) are passed straight to the facade, which stores them in
//!   the OS keychain. Never log or store them here.
//! - Argument names are snake_case here and camelCase in `src/api/tauri.ts` (Tauri maps them).

use std::path::PathBuf;

use chrono::NaiveDate;
use pagelamp_app::diagnostics::{self, CrashReport};
use pagelamp_app::{
    AppError, AppStatus, McpClientConfig, SourceSyncResult, SyncEvent, SyncRequest, SyncSummary,
};
use pagelamp_app::{StartupTasks, UpdateChannel, UpdateCheckRecord, UpdatePrefs};
use pagelamp_core::model::{AiPolicy, SearchHit, SourceRecord, StoredStudyPlan};
use pagelamp_core::views::{CourseOverview, CourseSummary, Deadline, WeekMaterials};
use tauri::State;
use tauri::ipc::Channel;
use tauri_plugin_opener::OpenerExt;

use crate::backend::{Backend, internal, pagelamp_binary};

type CmdResult<T> = Result<T, AppError>;

// ----- status & sources -------------------------------------------------------------------------

#[tauri::command]
pub async fn status(backend: State<'_, Backend>) -> CmdResult<AppStatus> {
    backend.blocking(|app| app.status()).await
}

#[tauri::command]
pub async fn list_sources(backend: State<'_, Backend>) -> CmdResult<Vec<SourceRecord>> {
    backend.blocking(|app| app.list_sources()).await
}

#[tauri::command]
pub async fn add_canvas_source(
    backend: State<'_, Backend>,
    base_url: String,
    token: String,
) -> CmdResult<SourceRecord> {
    backend
        .spawn(|app| async move { app.add_canvas_source(&base_url, &token).await })
        .await
}

#[tauri::command]
pub async fn add_folder_source(
    backend: State<'_, Backend>,
    path: PathBuf,
    term_start: Option<NaiveDate>,
    label: Option<String>,
) -> CmdResult<SourceRecord> {
    backend
        .blocking(move |app| app.add_folder_source(&path, term_start, label.as_deref()))
        .await
}

#[tauri::command]
pub async fn add_ical_source(
    backend: State<'_, Backend>,
    feed_url: String,
    label: Option<String>,
) -> CmdResult<SourceRecord> {
    backend
        .spawn(|app| async move { app.add_ical_source(&feed_url, label.as_deref()).await })
        .await
}

#[tauri::command]
pub async fn update_source_secret(
    backend: State<'_, Backend>,
    source_id: String,
    secret: String,
) -> CmdResult<SourceRecord> {
    backend
        .spawn(|app| async move { app.update_source_secret(&source_id, &secret).await })
        .await
}

#[tauri::command]
pub async fn remove_source(backend: State<'_, Backend>, source_id: String) -> CmdResult<()> {
    backend
        .blocking(move |app| app.remove_source(&source_id))
        .await
}

// ----- sync (progress is streamed to the UI through a Channel) ------------------------------------

#[tauri::command]
pub async fn sync_all(
    backend: State<'_, Backend>,
    req: SyncRequest,
    on_event: Channel<SyncEvent>,
) -> CmdResult<SyncSummary> {
    backend
        .spawn(|app| async move {
            app.sync_all(req, move |event| {
                // The UI may have gone away (window reload); the sync carries on regardless.
                let _ = on_event.send(event);
            })
            .await
        })
        .await
}

#[tauri::command]
pub async fn sync_source(
    backend: State<'_, Backend>,
    source_id: String,
    req: SyncRequest,
    on_event: Channel<SyncEvent>,
) -> CmdResult<SourceSyncResult> {
    backend
        .spawn(|app| async move {
            app.sync_source(&source_id, req, move |event| {
                let _ = on_event.send(event);
            })
            .await
        })
        .await
}

/// Explicit per-course "download & index files" (Canvas). The UI discloses first that a
/// download through Canvas can count as viewing the file.
#[tauri::command]
pub async fn download_course_files(
    backend: State<'_, Backend>,
    course: String,
    on_event: Channel<SyncEvent>,
) -> CmdResult<SourceSyncResult> {
    backend
        .spawn(|app| async move {
            app.download_course_files(&course, move |event| {
                let _ = on_event.send(event);
            })
            .await
        })
        .await
}

// ----- read views ---------------------------------------------------------------------------------

#[tauri::command]
pub async fn list_courses(backend: State<'_, Backend>) -> CmdResult<Vec<CourseSummary>> {
    backend.blocking(|app| app.list_courses()).await
}

#[tauri::command]
pub async fn course_overview(
    backend: State<'_, Backend>,
    course: String,
) -> CmdResult<CourseOverview> {
    backend
        .blocking(move |app| app.course_overview(&course))
        .await
}

#[tauri::command]
pub async fn week_materials(
    backend: State<'_, Backend>,
    course: String,
    week: Option<u32>,
) -> CmdResult<WeekMaterials> {
    backend
        .blocking(move |app| app.week_materials(&course, week))
        .await
}

#[tauri::command]
pub async fn list_deadlines(
    backend: State<'_, Backend>,
    course: Option<String>,
    days_ahead: u32,
    days_back: u32,
) -> CmdResult<Vec<Deadline>> {
    backend
        .blocking(move |app| app.list_deadlines(course.as_deref(), days_ahead, days_back))
        .await
}

#[tauri::command]
pub async fn search(
    backend: State<'_, Backend>,
    query: String,
    course: Option<String>,
    limit: u32,
) -> CmdResult<Vec<SearchHit>> {
    backend
        .blocking(move |app| app.search(&query, course.as_deref(), limit))
        .await
}

#[tauri::command]
pub async fn latest_study_plan(backend: State<'_, Backend>) -> CmdResult<Option<StoredStudyPlan>> {
    backend.blocking(|app| app.latest_study_plan()).await
}

// ----- course settings ----------------------------------------------------------------------------

#[tauri::command]
pub async fn set_course_policy(
    backend: State<'_, Backend>,
    course: String,
    policy: AiPolicy,
    note: Option<String>,
) -> CmdResult<()> {
    backend
        .blocking(move |app| app.set_course_policy(&course, policy, note.as_deref()))
        .await
}

#[tauri::command]
pub async fn set_course_term(
    backend: State<'_, Backend>,
    course: String,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
) -> CmdResult<()> {
    backend
        .blocking(move |app| app.set_course_term(&course, start, end))
        .await
}

/// "Let my AI app read this course's materials" (ARCHITECTURE §3 rule 8).
#[tauri::command]
pub async fn set_course_ai_access(
    backend: State<'_, Backend>,
    course: String,
    allowed: bool,
) -> CmdResult<()> {
    backend
        .blocking(move |app| app.set_course_ai_access(&course, allowed))
        .await
}

#[tauri::command]
pub async fn set_course_hidden(
    backend: State<'_, Backend>,
    course: String,
    hidden: bool,
) -> CmdResult<()> {
    backend
        .blocking(move |app| app.set_course_hidden(&course, hidden))
        .await
}

// ----- "connect your AI app" ---------------------------------------------------------------------

/// The binary path is decided here, never by the UI (see `backend::pagelamp_binary`).
#[tauri::command]
pub async fn mcp_client_configs(backend: State<'_, Backend>) -> CmdResult<Vec<McpClientConfig>> {
    backend
        .blocking(|app| Ok(app.mcp_client_configs(&pagelamp_binary())))
        .await
}

// ----- desktop helpers (not facade methods) ----------------------------------------------------------

/// Show the data directory in Finder / Explorer. Done here (not from JS) so the webview needs
/// no filesystem-reveal permission at all.
#[tauri::command]
pub async fn reveal_data_dir<R: tauri::Runtime>(
    backend: State<'_, Backend>,
    window: tauri::WebviewWindow<R>,
) -> CmdResult<()> {
    // Also when the core can't open (e.g. a database from another version): that's when the
    // student may need the folder, to restore a backup.
    let dir = backend
        .diagnostics(
            |app| Ok(app.data_dir().to_path_buf()),
            || pagelamp_core::paths::data_dir().map_err(AppError::from),
        )
        .await?;
    window
        .opener()
        .reveal_item_in_dir(dir)
        .map_err(|err| internal(format!("couldn't open the data folder: {err}")))
}

// ----- diagnostics (work even when the facade couldn't open) -------------------------------------

#[tauri::command]
pub async fn diagnostic_report(backend: State<'_, Backend>) -> CmdResult<String> {
    backend
        .diagnostics(
            |app| app.diagnostic_report(),
            diagnostics::diagnostic_report,
        )
        .await
}

#[tauri::command]
pub async fn last_crash(backend: State<'_, Backend>) -> CmdResult<Option<CrashReport>> {
    backend
        .diagnostics(|app| app.last_crash(), diagnostics::last_crash)
        .await
}

#[tauri::command]
pub async fn clear_last_crash(backend: State<'_, Backend>) -> CmdResult<()> {
    backend
        .diagnostics(|app| app.clear_last_crash(), diagnostics::clear_last_crash)
        .await
}

/// Opens the logs folder itself. It takes no path from the UI, so nothing else can be opened.
#[tauri::command]
pub async fn reveal_logs_dir<R: tauri::Runtime>(
    backend: State<'_, Backend>,
    window: tauri::WebviewWindow<R>,
) -> CmdResult<()> {
    let dir = backend
        .diagnostics(|app| app.logs_dir(), diagnostics::logs_dir)
        .await?;
    window
        .opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|err| internal(format!("couldn't open the logs folder: {err}")))
}

/// A screen crashed (the UI's error boundary): message and stack only, capped and redacted by
/// the core. Never fails, so logging can't cause a second error in the UI.
#[tauri::command]
pub async fn log_ui_error(message: String, stack: Option<String>) {
    diagnostics::log_ui_error(&message, stack.as_deref());
}

// ----- updates: preferences and what's due (the updater itself is in updates.rs) ----------------

#[tauri::command]
pub async fn update_prefs(backend: State<'_, Backend>) -> CmdResult<UpdatePrefs> {
    backend.blocking(|app| app.update_prefs()).await
}

#[tauri::command]
pub async fn set_update_prefs(backend: State<'_, Backend>, prefs: UpdatePrefs) -> CmdResult<()> {
    backend
        .blocking(move |app| app.set_update_prefs(prefs))
        .await
}

#[tauri::command]
pub async fn effective_update_channel(backend: State<'_, Backend>) -> CmdResult<UpdateChannel> {
    backend.blocking(|app| app.effective_update_channel()).await
}

#[tauri::command]
pub async fn startup_tasks(backend: State<'_, Backend>) -> CmdResult<StartupTasks> {
    backend
        .blocking(|app| app.startup_tasks(chrono::Utc::now()))
        .await
}

#[tauri::command]
pub async fn acknowledge_whats_new(backend: State<'_, Backend>) -> CmdResult<()> {
    backend.blocking(|app| app.acknowledge_whats_new()).await
}

#[tauri::command]
pub async fn acknowledge_update_disclosure(backend: State<'_, Backend>) -> CmdResult<()> {
    backend
        .blocking(|app| app.acknowledge_update_disclosure())
        .await
}

#[tauri::command]
pub async fn last_update_check(
    backend: State<'_, Backend>,
) -> CmdResult<Option<UpdateCheckRecord>> {
    backend.blocking(|app| app.last_update_check()).await
}
