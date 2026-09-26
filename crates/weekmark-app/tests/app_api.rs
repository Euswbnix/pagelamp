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
use serde_json::json;
use weekmark_app::{App, AppError, AppErrorKind, McpClient, McpNoteCode, SyncEvent, SyncRequest};
use weekmark_core::model::*;
use weekmark_core::secrets::{MemorySecrets, SecretBackend};
use weekmark_core::source::SourceError;
use weekmark_core::store::Store;

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
    app.remove_source("canvas:lms.example.edu").unwrap();
    assert!(app.list_sources().unwrap().is_empty());
    assert!(app.list_courses().unwrap().is_empty());
    assert_eq!(secrets.get("canvas:lms.example.edu").unwrap(), None);
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
    let configs = app.mcp_client_configs(Path::new("bin/weekmark"));
    assert_eq!(configs.len(), 4);
    for config in &configs {
        assert!(Path::new(&config.launch.command).is_absolute());
        // Compare path components, not strings (Windows uses backslashes).
        assert!(Path::new(&config.launch.command).ends_with(Path::new("bin").join("weekmark")));
        assert_eq!(config.launch.args, ["mcp"]);
        // A temp data dir is never the platform default → WEEKMARK_HOME is set.
        assert_eq!(
            config.launch.env.get("WEEKMARK_HOME").map(String::as_str),
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
    use weekmark_core::Error as E;
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
    let schema = weekmark_app::json_schema();
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

#[tokio::test]
async fn canvas_source_is_validated_before_its_token_is_stored() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let canvas = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/users/self"))
        .and(header("authorization", "Bearer demo-good-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name": "Demo Student"})))
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
        .add_canvas_source(&format!("{}/courses/1", canvas.uri()), " demo-good-token ")
        .await
        .unwrap();
    assert!(source.id.starts_with("canvas:127.0.0.1:"));
    assert_eq!(source.config["base_url"], json!(canvas.uri()));
    assert_eq!(
        secrets.get(&source.id).unwrap().as_deref(),
        Some("demo-good-token")
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
    app.update_source_secret(&source.id, "demo-good-token")
        .await
        .unwrap();
}
