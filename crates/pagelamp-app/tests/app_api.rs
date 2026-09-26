//! Tests of the App facade over temporary data dirs with in-memory secrets.
//! All data is synthetic ("DEMO101 Intro to Demo Studies" and friends).
//!
//! Syncing real sources (folder / iCal / Canvas) is covered by the source crates' own tests
//! and by `sync_*` tests once those crates are wired in; here only the orchestration edges
//! that don't reach a source are tested.

use std::fs::OpenOptions;
use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, Utc};
use pagelamp_app::{App, AppError, AppErrorKind, McpClient, McpNoteCode, SyncEvent, SyncRequest};
use pagelamp_core::model::*;
use pagelamp_core::secrets::{MemorySecrets, SecretBackend};
use pagelamp_core::source::SourceError;
use pagelamp_core::store::Store;
use serde_json::json;

fn app_in(dir: &Path) -> (App, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::new());
    let app = App::open_at_with_secrets(dir.join("data"), secrets.clone()).unwrap();
    (app, secrets)
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

/// A Canvas-like source with DEMO101 (visible) and DEMO303 (hidden), seeded directly.
fn seed(app: &App) {
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_source(&SourceRecord {
            id: "canvas:lms.example.edu".into(),
            kind: SourceKind::Canvas,
            label: "lms.example.edu".into(),
            config: json!({ "base_url": "https://lms.example.edu" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    for (external, code, name) in [
        ("101", "DEMO101", "Intro to Demo Studies"),
        ("303", "DEMO303", "Hidden Demo Seminar"),
    ] {
        store
            .upsert_course(&CourseUpsert {
                id: format!("canvas:lms.example.edu/course/{external}"),
                source_id: "canvas:lms.example.edu".into(),
                external_id: external.into(),
                code: Some(code.into()),
                name: name.into(),
                term_start: Some(date("2026-09-07")),
                term_end: Some(date("2026-12-18")),
                url: None,
                syllabus_text: None,
            })
            .unwrap();
    }
    store
        .set_course_hidden("canvas:lms.example.edu/course/303", true)
        .unwrap();
    let material_id = "canvas:lms.example.edu/file/1".to_string();
    store
        .upsert_material(&MaterialUpsert {
            id: material_id.clone(),
            course_id: "canvas:lms.example.edu/course/101".into(),
            module_id: None,
            kind: MaterialKind::File,
            title: "Lecture 1 slides".into(),
            url: None,
            local_path: None,
            mime: None,
            published_at: Some(Utc::now()),
            week_hint: Some(1),
        })
        .unwrap();
    store
        .replace_chunks(
            &material_id,
            &[Chunk {
                material_id: material_id.clone(),
                ord: 0,
                locator: Some("slide 1".into()),
                text: "photosynthesis converts light".into(),
            }],
        )
        .unwrap();
    store
        .set_text_state(&material_id, TextStatus::Ok, None, Some("h"))
        .unwrap();
}

fn kind<T: std::fmt::Debug>(result: Result<T, AppError>) -> AppErrorKind {
    result.unwrap_err().kind
}

// ----- opening & status -----------------------------------------------------------------------

#[test]
fn open_at_creates_the_data_dir_and_database() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    assert!(app.db_path().is_file());
    assert!(app.data_dir().join("files").is_dir());

    let status = app.status().unwrap();
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert!(status.sources.is_empty());
    assert_eq!(status.counts, StoreCounts::default());
    assert_eq!(status.last_synced_at, None);
    assert!(!status.sync_in_progress);
    // Re-opening an existing data dir is fine.
    App::open_at_with_secrets(app.data_dir().to_path_buf(), Arc::new(MemorySecrets::new()))
        .unwrap();
}

#[test]
fn app_is_cheap_to_share() {
    fn assert_traits<T: Clone + Send + Sync + 'static>() {}
    assert_traits::<App>();
}

#[test]
fn status_reports_a_sync_in_progress_while_the_lock_is_held() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(app.data_dir().join("sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(app.status().unwrap().sync_in_progress);
    drop(lock);
    assert!(!app.status().unwrap().sync_in_progress);
}

// ----- sources --------------------------------------------------------------------------------

#[test]
fn add_folder_source_validates_and_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let courses = temp.path().join("My Courses");
    std::fs::create_dir(&courses).unwrap();

    assert_eq!(
        kind(app.add_folder_source(&temp.path().join("missing"), None, None)),
        AppErrorKind::Invalid
    );
    let file = temp.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(
        kind(app.add_folder_source(&file, None, None)),
        AppErrorKind::Invalid
    );

    let first = app.add_folder_source(&courses, None, None).unwrap();
    assert!(first.id.starts_with("folder:") && first.id.len() == "folder:".len() + 12);
    assert_eq!(first.kind, SourceKind::Folder);
    assert!(first.label.ends_with("My Courses"), "{}", first.label);
    let canonical = std::fs::canonicalize(&courses).unwrap();
    assert_eq!(first.config["path"], json!(canonical.to_string_lossy()));

    let again = app
        .add_folder_source(&courses, Some(date("2026-09-08")), Some(" Fall term "))
        .unwrap();
    assert_eq!(again.id, first.id);
    assert_eq!(again.label, "Fall term");
    assert_eq!(again.config["term_start"], json!("2026-09-08"));
    // Unchanged fields survive a later add without them.
    let third = app.add_folder_source(&courses, None, None).unwrap();
    assert_eq!(third.label, "Fall term");
    assert_eq!(third.config["term_start"], json!("2026-09-08"));
    assert_eq!(app.list_sources().unwrap().len(), 1);
}

#[test]
fn remove_source_deletes_rows_and_secret() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    seed(&app);
    secrets
        .set("canvas:lms.example.edu", "demo-not-a-real-token")
        .unwrap();

    assert_eq!(
        kind(app.remove_source("canvas:nope")),
        AppErrorKind::NotFound
    );
    // Downloaded copies of both courses (DEMO303 is hidden) go too; other dirs stay.
    let files = app.data_dir().join("files");
    for dir in ["DEMO101-101", "DEMO303-303", "OTHER-9"] {
        std::fs::create_dir_all(files.join(dir)).unwrap();
        std::fs::write(files.join(dir).join("1-slides.txt"), "demo").unwrap();
    }

    // Not while a sync runs (it could be writing those files).
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(app.data_dir().join("sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert_eq!(
        kind(app.remove_source("canvas:lms.example.edu")),
        AppErrorKind::Busy
    );
    drop(lock);

    app.remove_source("canvas:lms.example.edu").unwrap();
    assert!(app.list_sources().unwrap().is_empty());
    assert!(app.list_courses().unwrap().is_empty());
    assert_eq!(secrets.get("canvas:lms.example.edu").unwrap(), None);
    assert!(!files.join("DEMO101-101").exists());
    assert!(!files.join("DEMO303-303").exists());
    assert!(files.join("OTHER-9/1-slides.txt").is_file());
}

#[test]
fn removal_cleans_old_download_dirs_and_skips_files_links_and_shared_dirs() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed(&app);
    let files = app.data_dir().join("files");
    let store = Store::open(&app.db_path()).unwrap();
    // DEMO101 was called OLD101 when its slides were downloaded.
    std::fs::create_dir_all(files.join("OLD101-101")).unwrap();
    std::fs::write(files.join("OLD101-101/1-slides.txt"), "demo").unwrap();
    store
        .upsert_material(&MaterialUpsert {
            id: "canvas:lms.example.edu/file/2".into(),
            course_id: "canvas:lms.example.edu/course/101".into(),
            module_id: None,
            kind: MaterialKind::File,
            title: "Old slides".into(),
            url: None,
            local_path: Some(files.join("OLD101-101/1-slides.txt").display().to_string()),
            mime: None,
            published_at: None,
            week_hint: None,
        })
        .unwrap();
    // A stray regular file where DEMO303's directory would be.
    std::fs::write(files.join("DEMO303-303"), "not a directory").unwrap();
    // Another Canvas uses "demo101-101" too (same name on a case-insensitive disk).
    store
        .upsert_source(&SourceRecord {
            id: "canvas:other.example.edu".into(),
            kind: SourceKind::Canvas,
            label: "other.example.edu".into(),
            config: json!({}),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: "canvas:other.example.edu/course/101".into(),
            source_id: "canvas:other.example.edu".into(),
            external_id: "101".into(),
            code: Some("demo101".into()),
            name: "Other demo".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
        })
        .unwrap();
    std::fs::create_dir_all(files.join("demo101-101")).unwrap();
    std::fs::write(files.join("demo101-101/1-other.txt"), "other").unwrap();
    drop(store);

    app.remove_source("canvas:lms.example.edu").unwrap();
    assert!(!files.join("OLD101-101").exists(), "old code dir removed");
    assert!(
        files.join("DEMO303-303").is_file(),
        "a stray file is left alone"
    );
    assert!(
        files.join("demo101-101/1-other.txt").is_file(),
        "shared dir kept"
    );
}

#[cfg(unix)]
#[test]
fn removal_deletes_a_linked_download_dir_but_not_its_target() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed(&app);
    let files = app.data_dir().join("files");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), "keep").unwrap();
    std::os::unix::fs::symlink(&outside, files.join("DEMO101-101")).unwrap();
    app.remove_source("canvas:lms.example.edu").unwrap();
    assert!(std::fs::symlink_metadata(files.join("DEMO101-101")).is_err());
    assert!(outside.join("keep.txt").is_file());
}

/// Secrets that must never be deleted (folder sources have none).
struct NoDelete(MemorySecrets);

impl SecretBackend for NoDelete {
    fn get(&self, source_id: &str) -> pagelamp_core::Result<Option<String>> {
        self.0.get(source_id)
    }
    fn set(&self, source_id: &str, secret: &str) -> pagelamp_core::Result<()> {
        self.0.set(source_id, secret)
    }
    fn delete(&self, source_id: &str) -> pagelamp_core::Result<()> {
        panic!("keychain delete for {source_id}")
    }
}

#[test]
fn removing_a_folder_source_leaves_the_keychain_and_the_folder_alone() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open_at_with_secrets(
        temp.path().join("data"),
        Arc::new(NoDelete(MemorySecrets::new())),
    )
    .unwrap();
    let root = demo_course_folder(temp.path());
    let folder = app.add_folder_source(&root, None, None).unwrap();
    app.remove_source(&folder.id).unwrap();
    assert!(app.list_sources().unwrap().is_empty());
    assert!(root.join("DEMO101H1 Intro/week1.txt").is_file());
}

#[tokio::test]
async fn update_source_secret_rejects_folders_unknown_ids_and_empty_values() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let folder = app.add_folder_source(temp.path(), None, None).unwrap();
    assert_eq!(
        kind(app.update_source_secret(&folder.id, "anything").await),
        AppErrorKind::Invalid
    );
    assert_eq!(
        kind(
            app.update_source_secret("ical:nope", "https://x.example.edu")
                .await
        ),
        AppErrorKind::NotFound
    );
    seed(&app);
    let err = app
        .update_source_secret("canvas:lms.example.edu", "   ")
        .await
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Invalid);
}

// ----- read views & settings ------------------------------------------------------------------

#[test]
fn read_views_include_hidden_courses_for_the_desktop() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed(&app);

    let courses = app.list_courses().unwrap();
    assert_eq!(courses.len(), 2);
    assert!(courses.iter().any(|c| c.course.hidden));
    // A hidden course's page still works.
    let hidden = app.course_overview("DEMO303").unwrap();
    assert!(hidden.course.hidden);
    assert!(app.week_materials("DEMO303", Some(1)).is_ok());
    assert!(
        app.list_deadlines(Some("DEMO303"), 21, 0)
            .unwrap()
            .is_empty()
    );

    let week = app.week_materials("demo101", Some(1)).unwrap();
    assert_eq!(week.materials[0].title, "Lecture 1 slides");
    assert_eq!(app.search("photosynthesis", None, 10).unwrap().len(), 1);
    assert_eq!(app.latest_study_plan().unwrap().map(|p| p.id), None);
    assert_eq!(kind(app.course_overview("NOPE999")), AppErrorKind::NotFound);
    assert_eq!(kind(app.course_overview("DEMO")), AppErrorKind::Ambiguous);
}

#[test]
fn course_settings_address_hidden_courses_by_code() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed(&app);

    app.set_course_policy("DEMO303", AiPolicy::Prohibited, Some("  syllabus §3 "))
        .unwrap();
    app.set_course_ai_access("demo303", false).unwrap();
    app.set_course_term("DEMO303", Some(date("2026-09-01")), None)
        .unwrap();
    app.set_course_hidden("DEMO303", false).unwrap();

    let course = app.course_overview("DEMO303").unwrap();
    assert!(!course.course.hidden);
    assert_eq!(course.course.ai_policy, AiPolicy::Prohibited);
    assert_eq!(course.course.ai_policy_note.as_deref(), Some("syllabus §3"));
    assert!(!course.course.ai_access);
    assert_eq!(course.ai_materials, AiMaterialsState::WithheldByPolicy);
    assert_eq!(course.course.term_source, TermSource::User);

    app.set_course_term("DEMO303", None, None).unwrap();
    let course = app.course_overview("DEMO303").unwrap();
    assert_eq!(course.course.term_source, TermSource::Synced);

    assert_eq!(
        kind(app.set_course_term(
            "DEMO101",
            Some(date("2026-12-01")),
            Some(date("2026-09-01"))
        )),
        AppErrorKind::Invalid
    );
    assert_eq!(
        kind(app.set_course_hidden("NOPE999", true)),
        AppErrorKind::NotFound
    );
}

// ----- sync edges -----------------------------------------------------------------------------

#[tokio::test]
async fn sync_is_busy_while_another_process_holds_the_lock() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(app.data_dir().join("sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let busy = app
        .sync_all(SyncRequest::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(busy.kind, AppErrorKind::Busy);
    assert_eq!(
        kind(
            app.sync_source("folder:x", SyncRequest::default(), |_| {})
                .await
        ),
        AppErrorKind::Busy
    );
    drop(lock);
    let summary = app.sync_all(SyncRequest::default(), |_| {}).await.unwrap();
    assert!(summary.ok && summary.results.is_empty());
    assert_eq!(
        kind(
            app.sync_source("folder:x", SyncRequest::default(), |_| {})
                .await
        ),
        AppErrorKind::NotFound
    );
}

#[tokio::test]
async fn missing_secret_is_reported_as_an_auth_failure_and_recorded() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed(&app); // Canvas source, but no token stored.
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let result = app
        .sync_source("canvas:lms.example.edu", SyncRequest::default(), move |e| {
            sink.lock().unwrap().push(e)
        })
        .await
        .unwrap();
    assert!(!result.ok);
    assert_eq!(
        result.error_kind,
        Some(SourceErrorKind::AuthExpiredOrRevoked)
    );
    let events = events.lock().unwrap();
    assert!(matches!(
        events.first(),
        Some(SyncEvent::SourceStarted { .. })
    ));
    assert!(matches!(
        events.last(),
        Some(SyncEvent::SourceFinished {
            ok: false,
            error_kind: Some(SourceErrorKind::AuthExpiredOrRevoked),
            ..
        })
    ));
    let source = &app.list_sources().unwrap()[0];
    assert_eq!(
        source.last_error_kind,
        Some(SourceErrorKind::AuthExpiredOrRevoked)
    );
    assert!(source.last_error.is_some());
}

#[tokio::test]
async fn download_course_files_is_for_canvas_courses_only() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let folder = app.add_folder_source(temp.path(), None, None).unwrap();
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: format!("{}/course/DEMO404", folder.id),
            source_id: folder.id.clone(),
            external_id: "DEMO404".into(),
            code: Some("DEMO404".into()),
            name: "Folder Demo".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
        })
        .unwrap();
    assert_eq!(
        kind(app.download_course_files("DEMO404", |_| {}).await),
        AppErrorKind::Invalid
    );
    assert_eq!(
        kind(app.download_course_files("NOPE", |_| {}).await),
        AppErrorKind::NotFound
    );
}

/// Tauri spawns these futures on a multi-threaded runtime: they must be `Send`.
#[test]
fn async_facade_futures_are_send() {
    fn assert_send<T: Send>(_: &T) {}
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let noop = |_: SyncEvent| {};
    assert_send(&app.add_canvas_source("https://lms.example.edu", "t"));
    assert_send(&app.add_ical_source("https://calendar.example.edu/feed.ics", None));
    assert_send(&app.update_source_secret("x", "y"));
    assert_send(&app.sync_all(SyncRequest::default(), noop));
    assert_send(&app.sync_source("x", SyncRequest::default(), noop));
    assert_send(&app.download_course_files("x", noop));
}

// ----- MCP client configs ---------------------------------------------------------------------

#[test]
fn mcp_configs_point_at_the_binary_and_the_custom_data_dir() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let configs = app.mcp_client_configs(Path::new("bin/pagelamp"));
    assert_eq!(configs.len(), 4);
    for config in &configs {
        assert!(Path::new(&config.launch.command).is_absolute());
        // Compare path components, not strings (Windows uses backslashes).
        assert!(Path::new(&config.launch.command).ends_with(Path::new("bin").join("pagelamp")));
        assert_eq!(config.launch.args, ["mcp"]);
        // A temp data dir is never the platform default → PAGELAMP_HOME is set.
        assert_eq!(
            config.launch.env.get("PAGELAMP_HOME").map(String::as_str),
            Some(app.data_dir().to_str().unwrap())
        );
        assert!(config.note_codes.contains(&McpNoteCode::CustomDataDir));
        assert_eq!(config.notes.len(), config.note_codes.len());
    }
    assert_eq!(configs[0].client, McpClient::ClaudeDesktop);
}

// ----- errors & schema ------------------------------------------------------------------------

#[test]
fn errors_map_to_stable_kinds() {
    use pagelamp_core::Error as E;
    let cases = [
        (E::NotFound("x".into()), AppErrorKind::NotFound),
        (E::NotInitialised("x".into()), AppErrorKind::NotFound),
        (
            E::Ambiguous {
                query: "d".into(),
                candidates: vec!["DEMO1".into(), "DEMO2".into()],
            },
            AppErrorKind::Ambiguous,
        ),
        (E::Invalid("x".into()), AppErrorKind::Invalid),
        (E::NoDataDir, AppErrorKind::Internal),
        (E::Secret("x".into()), AppErrorKind::Internal),
    ];
    for (err, expected) in cases {
        assert_eq!(AppError::from(err).kind, expected);
    }
    let cases = [
        (SourceError::auth("a"), AppErrorKind::Auth),
        (SourceError::network("n"), AppErrorKind::Network),
        (SourceError::rate_limited("r"), AppErrorKind::Network),
        (SourceError::not_found("f"), AppErrorKind::NotFound),
        (SourceError::other("o"), AppErrorKind::Internal),
    ];
    for (err, expected) in cases {
        assert_eq!(AppError::from(err).kind, expected);
    }
    let json = serde_json::to_value(AppError::new(AppErrorKind::Busy, "wait")).unwrap();
    assert_eq!(json, json!({ "kind": "busy", "message": "wait" }));
}

#[test]
fn json_schema_covers_the_facade_types() {
    let schema = pagelamp_app::json_schema();
    let defs = schema["$defs"].as_object().unwrap();
    for name in [
        "AppError",
        "AppStatus",
        "SourceRecord",
        "SyncRequest",
        "SyncEvent",
        "SyncSummary",
        "SourceSyncResult",
        "CourseSummary",
        "CourseOverview",
        "WeekMaterials",
        "Deadline",
        "SearchHit",
        "StoredStudyPlan",
        "McpClientConfig",
        "AiMaterialsState",
        "TermSource",
        "WeekNoteKind",
        "McpNoteCode",
    ] {
        assert!(defs.contains_key(name), "missing {name}");
    }
    assert_eq!(
        serde_json::to_value(SyncRequest::default()).unwrap()["download_files"],
        json!(false)
    );
}

// ----- end-to-end sync with the local sources ---------------------------------------------------

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[tokio::test]
async fn folder_sync_end_to_end_with_events_and_recorded_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let courses = temp.path().join("Courses");
    write(
        &courses,
        "DEMO101 Intro/Week 1/notes.md",
        "# Basics\nphotosynthesis basics",
    );
    write(&courses, "DEMO101 Intro/clip.mp4", "binary-ish");
    let source = app
        .add_folder_source(&courses, Some(date("2026-09-07")), None)
        .unwrap();

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let summary = app
        .sync_all(SyncRequest::default(), move |e| {
            sink.lock().unwrap().push(e)
        })
        .await
        .unwrap();
    assert!(summary.ok, "{summary:?}");
    let result = &summary.results[0];
    assert_eq!(
        (result.courses, result.materials, result.files_indexed),
        (1, 2, 1)
    );
    let events = events.lock().unwrap().clone();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SyncEvent::Progress { .. }))
    );
    assert!(matches!(
        events.last(),
        Some(SyncEvent::SourceFinished { ok: true, .. })
    ));

    let course = &app.list_courses().unwrap()[0];
    assert_eq!(course.course.code.as_deref(), Some("DEMO101"));
    assert_eq!(course.course.term_start, Some(date("2026-09-07")));
    assert_eq!(course.counts.indexed_materials, 1);
    assert_eq!(app.search("photosynthesis", None, 5).unwrap().len(), 1);
    let status = app.status().unwrap();
    assert!(status.sources[0].last_synced_at.is_some());

    // The folder disappears: the sync fails as not_found and keeps the data.
    std::fs::remove_dir_all(&courses).unwrap();
    let failed = app
        .sync_source(&source.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(!failed.ok);
    assert_eq!(failed.error_kind, Some(SourceErrorKind::NotFound));
    assert_eq!(app.list_courses().unwrap().len(), 1);
    let source = &app.list_sources().unwrap()[0];
    assert_eq!(source.last_error_kind, Some(SourceErrorKind::NotFound));
}

#[tokio::test]
async fn ical_source_is_validated_saved_synced_and_its_url_replaced() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let feed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Demo//EN\r\nBEGIN:VEVENT\r\n\
                UID:ps1\r\nDTSTART:20300930T035900Z\r\nDTEND:20300930T035900Z\r\n\
                SUMMARY:Problem Set 1\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    Mock::given(method("GET"))
        .and(path("/feed-secret-1.ics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/revoked.ics"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/downgrade.ics"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("Location", "http://calendar.example.edu/x.ics"),
        )
        .mount(&server)
        .await;

    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    assert_eq!(
        kind(app.add_ical_source("not a url feed-secret", None).await),
        AppErrorKind::Invalid
    );
    let revoked = app
        .add_ical_source(&format!("{}/revoked.ics", server.uri()), None)
        .await
        .unwrap_err();
    assert_eq!(revoked.kind, AppErrorKind::Auth);
    // A feed that redirects to plain http can't be used: invalid input, not an internal error.
    let downgrade = app
        .add_ical_source(&format!("{}/downgrade.ics", server.uri()), None)
        .await
        .unwrap_err();
    assert_eq!(
        downgrade.kind,
        AppErrorKind::Invalid,
        "{}",
        downgrade.message
    );
    assert!(
        app.list_sources().unwrap().is_empty(),
        "nothing saved on failure"
    );

    let url = format!("{}/feed-secret-1.ics", server.uri());
    let source = app
        .add_ical_source(&url, Some("Demo calendar"))
        .await
        .unwrap();
    assert!(source.id.starts_with("ical:"));
    assert!(!source.id.contains("feed-secret"));
    assert_eq!(source.label, "Demo calendar");
    assert_eq!(
        secrets.get(&source.id).unwrap().as_deref(),
        Some(url.as_str())
    );
    assert!(
        !serde_json::to_string(&source)
            .unwrap()
            .contains("feed-secret")
    );

    let result = app
        .sync_source(&source.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(result.ok, "{result:?}");
    assert_eq!(result.events, 1);
    let deadlines = app.list_deadlines(None, 365 * 10, 0).unwrap();
    assert_eq!(deadlines[0].event.title, "Problem Set 1");

    // Replacing the URL validates it first and keeps the source id.
    let bad = app
        .update_source_secret(&source.id, &format!("{}/revoked.ics", server.uri()))
        .await
        .unwrap_err();
    assert_eq!(bad.kind, AppErrorKind::Auth);
    assert_eq!(
        secrets.get(&source.id).unwrap().as_deref(),
        Some(url.as_str())
    );
    let updated = app.update_source_secret(&source.id, &url).await.unwrap();
    assert_eq!(updated.id, source.id);
}

/// A feed with one deadline for DEMO101H1 (Canvas-style bracket suffix), served by `server`.
async fn demo_feed(server: &wiremock::MockServer) -> String {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    let feed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Demo//EN\r\nBEGIN:VEVENT\r\n\
                UID:ps1\r\nDTSTART:20300930T035900Z\r\nDTEND:20300930T035900Z\r\n\
                SUMMARY:Problem Set 1 [DEMO101H1 F LEC0101]\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    Mock::given(method("GET"))
        .and(path("/demo-feed.ics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(feed))
        .mount(server)
        .await;
    format!("{}/demo-feed.ics", server.uri())
}

/// `<dir>/Courses/DEMO101H1 Intro/week1.txt`; returns the `Courses` root.
fn demo_course_folder(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("Courses");
    std::fs::create_dir_all(root.join("DEMO101H1 Intro")).unwrap();
    std::fs::write(root.join("DEMO101H1 Intro/week1.txt"), "Demo notes").unwrap();
    root
}

#[tokio::test]
async fn sync_all_runs_calendar_feeds_after_the_sources_that_create_courses() {
    let server = wiremock::MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    // "Calendar feed" sorts before "Courses": label order alone would sync the feed first,
    // before DEMO101H1 exists.
    let feed = app
        .add_ical_source(&demo_feed(&server).await, Some("Calendar feed"))
        .await
        .unwrap();
    let folder = app
        .add_folder_source(&demo_course_folder(temp.path()), None, None)
        .unwrap();
    assert_eq!(folder.label, "Courses");

    let events = Mutex::new(Vec::new());
    let summary = app
        .sync_all(SyncRequest::default(), |event| {
            if let SyncEvent::SourceStarted { source_id, .. } = event {
                events.lock().unwrap().push(source_id);
            }
        })
        .await
        .unwrap();
    assert!(summary.ok, "{summary:?}");
    assert_eq!(*events.lock().unwrap(), [folder.id, feed.id]);
    let deadlines = app.list_deadlines(Some("DEMO101H1"), 365 * 10, 0).unwrap();
    assert_eq!(deadlines.len(), 1, "{deadlines:?}");
    assert_eq!(deadlines[0].event.title, "Problem Set 1");
}

#[tokio::test]
async fn calendar_events_are_linked_to_courses_synced_later() {
    let server = wiremock::MockServer::start().await;
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let feed = app
        .add_ical_source(&demo_feed(&server).await, None)
        .await
        .unwrap();
    let result = app
        .sync_source(&feed.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(result.ok, "{result:?}");
    let unlinked = app.list_deadlines(None, 365 * 10, 0).unwrap();
    assert_eq!(unlinked.len(), 1);
    assert!(unlinked[0].event.course_id.is_none());

    // The folder is added and synced on its own; the feed is not fetched again.
    drop(server);
    let folder = app
        .add_folder_source(&demo_course_folder(temp.path()), None, None)
        .unwrap();
    let result = app
        .sync_source(&folder.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(result.ok, "{result:?}");
    let linked = app.list_deadlines(Some("DEMO101H1"), 365 * 10, 0).unwrap();
    assert_eq!(linked.len(), 1, "{linked:?}");
    assert_eq!(linked[0].event.title, "Problem Set 1");
}

#[tokio::test]
async fn canvas_source_is_validated_before_its_token_is_stored() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let canvas = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/self"))
        .and(header("authorization", "Bearer demo-good-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": 1, "name": "Demo Student"})),
        )
        .mount(&canvas)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/self"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"errors": [{"message": "Invalid access token."}]})),
        )
        .with_priority(10)
        .mount(&canvas)
        .await;

    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    assert_eq!(
        kind(
            app.add_canvas_source("ftp://lms.example.edu", "demo-good-token")
                .await
        ),
        AppErrorKind::Invalid
    );
    let rejected = app
        .add_canvas_source(&canvas.uri(), "demo-bad-token")
        .await
        .unwrap_err();
    assert_eq!(rejected.kind, AppErrorKind::Auth);
    assert!(!rejected.message.contains("demo-bad-token"));
    assert!(app.list_sources().unwrap().is_empty());

    let source = app
        .add_canvas_source(&format!("{}/", canvas.uri()), " demo-good-token ")
        .await
        .unwrap();
    assert!(source.id.starts_with("canvas:127.0.0.1:"));
    assert_eq!(source.config["base_url"], json!(canvas.uri()));
    assert_eq!(
        secrets.get(&source.id).unwrap().as_deref(),
        Some("demo-good-token")
    );
    assert_eq!(source.config["account_name"], json!("Demo Student"));
    // A pasted course link is not a Canvas address.
    assert_eq!(
        kind(
            app.add_canvas_source(&format!("{}/courses/1", canvas.uri()), "demo-good-token")
                .await
        ),
        AppErrorKind::Invalid
    );

    // Replacing an expired token validates the new one first.
    let bad = app
        .update_source_secret(&source.id, "demo-bad-token")
        .await
        .unwrap_err();
    assert_eq!(bad.kind, AppErrorKind::Auth);
    assert_eq!(
        secrets.get(&source.id).unwrap().as_deref(),
        Some("demo-good-token")
    );
    // A token of another account updates "Connected as …".
    Mock::given(method("GET"))
        .and(path("/api/v1/users/self"))
        .and(header("authorization", "Bearer demo-other-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": 2, "name": "Other Demo Student"})),
        )
        .mount(&canvas)
        .await;
    let updated = app
        .update_source_secret(&source.id, "demo-other-token")
        .await
        .unwrap();
    assert_eq!(updated.config["account_name"], json!("Other Demo Student"));
    assert_eq!(updated.config["base_url"], json!(canvas.uri()));
}
