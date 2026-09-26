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
use studentos_app::{
    AppError, AppStatus, McpClientConfig, SourceSyncResult, SyncEvent, SyncRequest, SyncSummary,
};
use studentos_core::model::{AiPolicy, SearchHit, SourceRecord, StoredStudyPlan};
use studentos_core::views::{CourseOverview, CourseSummary, Deadline, WeekMaterials};
use tauri::State;
use tauri::ipc::Channel;
use tauri_plugin_opener::OpenerExt;

use crate::backend::{Backend, internal, studentos_binary};

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

/// The binary path is decided here, never by the UI (see `backend::studentos_binary`).
#[tauri::command]
pub async fn mcp_client_configs(backend: State<'_, Backend>) -> CmdResult<Vec<McpClientConfig>> {
    backend
        .blocking(|app| Ok(app.mcp_client_configs(&studentos_binary())))
        .await
}

// ----- desktop helpers (not facade methods) ----------------------------------------------------------

/// Show the data directory in Finder / Explorer. Done here (not from JS) so the webview needs
/// no filesystem-reveal permission at all.
#[tauri::command]
pub async fn reveal_data_dir(
    backend: State<'_, Backend>,
    window: tauri::WebviewWindow,
) -> CmdResult<()> {
    let app = backend.app()?;
    window
        .opener()
        .reveal_item_in_dir(app.data_dir())
        .map_err(|err| internal(format!("couldn't open the data folder: {err}")))
}
