//! StudentOS desktop shell.
//!
//! ─── Tauri boundary ──────────────────────────────────────────────────────────────────────────
//! The React UI (apps/desktop/src) calls the commands in `commands.rs` through
//! `src/api/tauri.ts`. Each command is a thin wrapper over one `studentos_app::App` method —
//! NO business logic lives here (docs/ARCHITECTURE.md §6). If the UI needs something new, it
//! is added to the facade in crates/studentos-app first.
//!
//! Permissions granted to the webview are in `capabilities/default.json`: the folder picker
//! (`dialog:allow-open`) and opening http(s) links. No shell and no filesystem access.
//! ─────────────────────────────────────────────────────────────────────────────────────────────

mod backend;
mod commands;

use backend::Backend;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Backend::open())
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::list_sources,
            commands::add_canvas_source,
            commands::add_folder_source,
            commands::add_ical_source,
            commands::update_source_secret,
            commands::remove_source,
            commands::sync_all,
            commands::sync_source,
            commands::download_course_files,
            commands::list_courses,
            commands::course_overview,
            commands::week_materials,
            commands::list_deadlines,
            commands::search,
            commands::latest_study_plan,
            commands::set_course_policy,
            commands::set_course_term,
            commands::set_course_ai_access,
            commands::set_course_hidden,
            commands::mcp_client_configs,
            commands::reveal_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the StudentOS desktop app");
}
