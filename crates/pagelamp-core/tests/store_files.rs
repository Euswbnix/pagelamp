//! File-backed `Store` tests: open modes, schema versions and WAL concurrency
//! (readers alongside the single writer). All data is synthetic and lives in temp dirs.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use pagelamp_core::Error;
use pagelamp_core::model::*;
use pagelamp_core::store::{SCHEMA_V1, SCHEMA_VERSION, Store};
use tempfile::TempDir;

/// Well below `busy_timeout` (5 s): a read that took this long was blocked by the writer.
const NOT_BLOCKED: Duration = Duration::from_secs(2);

fn temp_db() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pagelamp.db");
    (dir, path)
}

fn demo_source(id: &str) -> SourceRecord {
    SourceRecord {
        id: id.to_string(),
        kind: SourceKind::Folder,
        label: format!("Demo {id}"),
        config: serde_json::json!({ "path": "/demo/courses" }),
        last_synced_at: None,
        last_error: None,
        last_error_kind: None,
    }
}

fn demo_course() -> CourseUpsert {
    CourseUpsert {
        id: "folder:demo/course/DEMO101".to_string(),
        source_id: "folder:demo".to_string(),
        external_id: "DEMO101".to_string(),
        code: Some("DEMO101".to_string()),
        name: "Intro to Demo Studies".to_string(),
        term_start: NaiveDate::from_ymd_opt(2026, 9, 8),
        term_end: None,
        url: None,
        syllabus_text: None,
    }
}

fn demo_plan() -> StudyPlan {
    let day = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
    StudyPlan {
        horizon_start: day,
        horizon_end: day,
        items: vec![],
        notes: Some("demo".to_string()),
    }
}

fn pragma_text(store: &Store, name: &str) -> String {
    store
        .conn()
        .query_row(&format!("PRAGMA {name}"), [], |row| {
            row.get::<_, rusqlite::types::Value>(0)
        })
        .map(|value| match value {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(text) => text,
            other => format!("{other:?}"),
        })
        .unwrap()
}

/// A DB written by `Store::open` with one source and one course; the writer is closed again.
fn populated_db(path: &Path) {
    let writer = Store::open(path).unwrap();
    writer.upsert_source(&demo_source("folder:demo")).unwrap();
    writer.upsert_course(&demo_course()).unwrap();
}

#[test]
fn open_creates_db_with_expected_settings() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    assert!(path.is_file());
    assert_eq!(pragma_text(&store, "journal_mode"), "wal");
    assert_eq!(pragma_text(&store, "foreign_keys"), "1");
    assert_eq!(pragma_text(&store, "busy_timeout"), "5000");
    assert_eq!(pragma_text(&store, "synchronous"), "1"); // NORMAL
    assert_eq!(
        pragma_text(&store, "user_version"),
        SCHEMA_VERSION.to_string()
    );
}

#[test]
fn a_version_1_database_is_migrated_by_open_and_keeps_its_events() {
    let (_dir, path) = temp_db();
    let plain = rusqlite::Connection::open(&path).unwrap();
    plain.execute_batch(SCHEMA_V1).unwrap();
    plain.pragma_update(None, "user_version", 1).unwrap();
    plain
        .execute_batch(
            "INSERT INTO sources (id, kind, label) \
               VALUES ('ical:demo', 'ical', 'Demo feed'); \
             INSERT INTO events (id, source_id, kind, title, due_at, updated_at) \
               VALUES ('ical:demo/event/1', 'ical:demo', 'quiz_due', 'Demo quiz', \
                       '2026-10-01T10:00:00Z', '2026-09-20T00:00:00Z');",
        )
        .unwrap();
    drop(plain);
    // Readers never migrate: an older schema needs a read-write open first — the app,
    // a sync, or `upgrade_existing` (which the MCP server runs at startup).
    assert!(matches!(
        Store::open_read_only(&path),
        Err(Error::NotInitialised(_))
    ));
    assert_eq!(Store::upgrade_existing(&path).unwrap(), Some(1));
    assert_eq!(Store::upgrade_existing(&path).unwrap(), None, "only once");

    let store = Store::open_read_only(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    let events = store
        .list_events(
            chrono::DateTime::<chrono::Utc>::MIN_UTC,
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            None,
        )
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].title, "Demo quiz");
    assert_eq!(events[0].course_hint, None);
}

#[test]
fn upgrade_existing_never_creates_or_touches_new_databases() {
    let (_dir, path) = temp_db();
    assert_eq!(Store::upgrade_existing(&path).unwrap(), None);
    assert!(!path.exists(), "no file created");
    // Uninitialised (version 0): left for a real `open`.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE unrelated (x INTEGER)")
        .unwrap();
    assert_eq!(Store::upgrade_existing(&path).unwrap(), None);
    assert!(matches!(
        Store::open_read_only(&path),
        Err(Error::NotInitialised(_))
    ));
}

#[test]
fn reopening_keeps_data() {
    let (_dir, path) = temp_db();
    populated_db(&path);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.list_sources().unwrap().len(), 1);
    assert_eq!(store.list_courses(true).unwrap().len(), 1);
}

#[test]
fn read_only_open_of_missing_file_is_not_initialised() {
    let (_dir, path) = temp_db();
    assert!(matches!(
        Store::open_read_only(&path),
        Err(Error::NotInitialised(_))
    ));
    assert!(!path.exists(), "read-only open must not create the DB");
}

#[test]
fn read_only_open_of_uninitialised_db_is_not_initialised() {
    let (_dir, path) = temp_db();
    // A zero-byte file, then a SQLite file with user_version 0.
    std::fs::write(&path, b"").unwrap();
    assert!(matches!(
        Store::open_read_only(&path),
        Err(Error::NotInitialised(_))
    ));
    let plain = rusqlite::Connection::open(&path).unwrap();
    plain
        .execute_batch("CREATE TABLE unrelated (x INTEGER)")
        .unwrap();
    drop(plain);
    assert!(matches!(
        Store::open_read_only(&path),
        Err(Error::NotInitialised(_))
    ));
}

#[test]
fn newer_schema_is_rejected_by_both_open_modes() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    store
        .conn()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    drop(store);

    for result in [Store::open_read_only(&path), Store::open(&path)] {
        match result {
            Err(Error::SchemaTooNew { found, supported }) => {
                assert_eq!(found, SCHEMA_VERSION + 1);
                assert_eq!(supported, SCHEMA_VERSION);
            }
            Err(other) => panic!("expected SchemaTooNew, got {other:?}"),
            Ok(_) => panic!("expected SchemaTooNew, got a store"),
        }
    }
}

#[test]
fn read_only_open_after_writer_closed() {
    let (_dir, path) = temp_db();
    populated_db(&path);
    // The last connection is closed, so SQLite has checkpointed and removed the -wal/-shm
    // files. A read-only open must still work without them.
    let side_file = |suffix: &str| PathBuf::from(format!("{}{suffix}", path.display()));
    assert!(!side_file("-wal").exists());
    assert!(!side_file("-shm").exists());

    let reader = Store::open_read_only(&path).unwrap();
    // Read-only connection settings: wait for locks like the writer, and refuse writes even
    // if the open flags were ever changed.
    assert_eq!(pragma_text(&reader, "busy_timeout"), "5000");
    assert_eq!(pragma_text(&reader, "query_only"), "1");
    let sources = reader.list_sources().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].id, "folder:demo");
    let course = reader.resolve_course("demo101").unwrap();
    assert_eq!(course.name, "Intro to Demo Studies");
    assert_eq!(reader.counts().unwrap().courses, 1);
    assert!(reader.search("demo", None, 5).unwrap().is_empty());
}

#[test]
fn read_only_store_rejects_writes() {
    let (_dir, path) = temp_db();
    populated_db(&path);
    let reader = Store::open_read_only(&path).unwrap();
    assert!(reader.upsert_source(&demo_source("folder:other")).is_err());
    assert!(reader.save_study_plan(&demo_plan()).is_err());
    assert_eq!(reader.list_sources().unwrap().len(), 1);
}

#[test]
fn readers_are_not_blocked_by_an_open_write_transaction() {
    let (_dir, path) = temp_db();
    populated_db(&path);
    let writer = Store::open(&path).unwrap();

    // `in_transaction` = BEGIN IMMEDIATE: the writer holds the write lock inside the closure.
    writer
        .in_transaction(|w| {
            w.upsert_source(&demo_source("folder:uncommitted"))?;

            let started = Instant::now();
            let reader = Store::open_read_only(&path)?;
            let ids: Vec<String> = reader.list_sources()?.into_iter().map(|s| s.id).collect();
            // Sees the last committed state only.
            assert_eq!(ids, ["folder:demo"]);
            // A read-write open (as MCP does for save_study_plan) also needs no write lock.
            let second_writer = Store::open(&path)?;
            assert_eq!(second_writer.list_sources()?.len(), 1);
            assert!(started.elapsed() < NOT_BLOCKED, "reader was blocked");
            Ok(())
        })
        .unwrap();

    let reader = Store::open_read_only(&path).unwrap();
    assert_eq!(reader.list_sources().unwrap().len(), 2);
}

#[test]
fn reader_sees_raw_begin_immediate_isolation() {
    let (_dir, path) = temp_db();
    populated_db(&path);
    let writer = Store::open(&path).unwrap();
    let reader = Store::open_read_only(&path).unwrap();

    writer.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
        .upsert_source(&demo_source("folder:pending"))
        .unwrap();
    let started = Instant::now();
    assert_eq!(reader.list_sources().unwrap().len(), 1);
    assert!(started.elapsed() < NOT_BLOCKED, "reader was blocked");
    writer.conn().execute_batch("COMMIT").unwrap();

    // The same reader connection sees the commit on its next query.
    assert_eq!(reader.list_sources().unwrap().len(), 2);
}

#[test]
fn a_second_writer_waits_for_the_lock_instead_of_failing() {
    // No wall-clock thresholds (they are flaky on a loaded machine). Instead: the save must
    // succeed (without busy_timeout it would fail with "database is locked"), and it can only
    // have finished after the sync released its lock.
    let (_dir, path) = temp_db();
    populated_db(&path);

    // A "sync" holds the write lock on another thread until the MCP side is about to write,
    // then a little longer, and reports the last instant at which it still held the lock.
    let (locked_tx, locked_rx) = mpsc::channel();
    let (saving_tx, saving_rx) = mpsc::channel();
    let sync_path = path.clone();
    let sync = thread::spawn(move || {
        let writer = Store::open(&sync_path).unwrap();
        writer
            .in_transaction(|w| {
                w.upsert_source(&demo_source("folder:syncing"))?;
                locked_tx.send(()).unwrap();
                saving_rx.recv().unwrap();
                thread::sleep(Duration::from_millis(300));
                Ok(Instant::now()) // the lock is released after this, on commit
            })
            .unwrap()
    });
    locked_rx.recv().unwrap();

    // Meanwhile an MCP process saves a study plan: busy_timeout makes it wait, then succeed.
    let mcp = Store::open(&path).unwrap();
    saving_tx.send(()).unwrap();
    let saved = mcp.save_study_plan(&demo_plan()).unwrap();
    let saved_at = Instant::now();
    let still_locked_at = sync.join().unwrap();
    assert!(
        saved_at >= still_locked_at,
        "the save finished while the sync still held the write lock"
    );

    assert_eq!(mcp.latest_study_plan().unwrap().unwrap().id, saved.id);
    assert_eq!(mcp.list_sources().unwrap().len(), 2);
}
