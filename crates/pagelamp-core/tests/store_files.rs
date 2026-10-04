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
        lms: Default::default(),
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
        Err(Error::SchemaTooOld {
            found: 1,
            supported: SCHEMA_VERSION
        })
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

#[cfg(unix)]
#[test]
fn the_database_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, path) = temp_db();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    drop(Store::open(&path).unwrap());
    assert_eq!(mode(&path) & 0o077, 0);
    // Made readable by an older version (or a shared folder's defaults): fixed on open.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let store = Store::open(&path).unwrap();
    store.upsert_source(&demo_source("folder:demo")).unwrap();
    assert_eq!(mode(&path), 0o600);
    let wal = path.with_extension("db-wal");
    if wal.exists() {
        assert_eq!(mode(&wal) & 0o077, 0);
    }
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

/// A database written by a future version: `SCHEMA_VERSION + 1`, with `min_reader_version`
/// set to `min_reader` (`None`: no `schema_meta` row at all).
fn future_db(path: &Path, min_reader: Option<i64>) {
    let store = Store::open(path).unwrap();
    let conn = store.conn();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    conn.execute("DELETE FROM schema_meta", []).unwrap();
    if let Some(min) = min_reader {
        conn.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('min_reader_version', ?1)",
            [min.to_string()],
        )
        .unwrap();
    }
}

fn assert_too_new<T>(result: Result<T, Error>) {
    match result {
        Err(Error::SchemaTooNew { found, supported }) => {
            assert_eq!(found, SCHEMA_VERSION + 1);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        Err(other) => panic!("expected SchemaTooNew, got {other:?}"),
        Ok(_) => panic!("expected SchemaTooNew, got Ok"),
    }
}

#[test]
fn a_newer_database_is_readable_when_it_allows_this_reader_but_never_writable() {
    let (_dir, path) = temp_db();
    future_db(&path, Some(SCHEMA_VERSION));
    let reader = Store::open_read_only(&path).unwrap();
    assert_eq!(reader.schema_version().unwrap(), SCHEMA_VERSION + 1);
    assert_eq!(reader.min_reader_version().unwrap(), Some(SCHEMA_VERSION));
    assert!(reader.list_courses(true).unwrap().is_empty());
    drop(reader);
    assert_eq!(Store::upgrade_existing(&path).unwrap(), None);
    assert_too_new(Store::open(&path));
}

#[test]
fn a_newer_database_that_needs_a_newer_reader_is_refused_by_both_open_modes() {
    for min_reader in [Some(SCHEMA_VERSION + 1), None] {
        let (_dir, path) = temp_db();
        future_db(&path, min_reader);
        assert_too_new(Store::open_read_only(&path));
        assert_too_new(Store::upgrade_existing(&path));
        assert_too_new(Store::open(&path));
    }
}

/// A database at schema 2 (v0.1.0) with one Canvas course whose term override is `user`.
fn version_2_db(path: &Path, user: (&str, &str)) {
    let plain = rusqlite::Connection::open(path).unwrap();
    plain.execute_batch(SCHEMA_V1).unwrap();
    plain
        .execute_batch(pagelamp_core::store::SCHEMA_V2)
        .unwrap();
    plain.pragma_update(None, "user_version", 2).unwrap();
    plain
        .execute(
            "INSERT INTO sources (id, kind, label) VALUES ('canvas:demo', 'canvas', 'Demo LMS')",
            [],
        )
        .unwrap();
    plain
        .execute(
            "INSERT INTO courses (id, source_id, external_id, code, name, term_start, term_end,
                                  user_term_start, user_term_end, updated_at)
             VALUES ('canvas:demo/course/101', 'canvas:demo', '101', 'DEMO101', 'Intro',
                     '2026-05-01', '2026-12-31', ?1, ?2, '2026-09-20T00:00:00Z')",
            [user.0, user.1],
        )
        .unwrap();
}

#[test]
fn migrating_backs_up_the_database_first_and_keeps_only_the_newest_copy() {
    let (dir, path) = temp_db();
    version_2_db(&path, ("2026-05-01", "2026-12-31"));
    // A copy from an earlier update, and one a crashed process never finished.
    std::fs::write(dir.path().join("pagelamp.db.v1.bak"), b"old").unwrap();
    std::fs::write(dir.path().join("pagelamp.db.v1.bak.4242.tmp"), b"").unwrap();

    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    drop(store);
    let backup = pagelamp_core::store::database_backup(&path).expect("a backup");
    assert_eq!(backup.path, dir.path().join("pagelamp.db.v2.bak"));
    assert_eq!(backup.schema_version, 2);
    assert!(
        !dir.path().join("pagelamp.db.v1.bak").exists(),
        "only the newest"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&backup.path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "private: it holds course text");
    }
    // The copy is the database as it was: schema 2, with its course.
    let copy = rusqlite::Connection::open(&backup.path).unwrap();
    let version: i64 = copy
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let courses: i64 = copy
        .query_row("SELECT COUNT(*) FROM courses", [], |row| row.get(0))
        .unwrap();
    assert_eq!(courses, 1);
    drop(copy);

    // Opening again (nothing to migrate) makes no new copy; deleting removes every copy.
    Store::open(&path).unwrap();
    assert_eq!(
        pagelamp_core::store::database_backup(&path).unwrap().path,
        backup.path
    );
    assert_eq!(
        pagelamp_core::store::delete_database_backups(&path).unwrap(),
        2
    );
    assert!(pagelamp_core::store::database_backup(&path).is_none());
    assert!(!dir.path().join("pagelamp.db.v1.bak.4242.tmp").exists());
}

#[test]
fn the_backup_outcome_of_a_migration_is_recorded() {
    let (_dir, path) = temp_db();
    version_2_db(&path, ("2026-09-08", "2026-12-18"));
    let store = Store::open(&path).unwrap();
    let record = store.last_migration_backup().unwrap().expect("recorded");
    assert_eq!(
        (record.from_version, record.to_version),
        (2, SCHEMA_VERSION)
    );
    assert_eq!(record.outcome, MigrationBackupOutcome::Ok);
}

#[test]
fn a_failed_backup_is_recorded_and_the_migration_goes_ahead() {
    let (dir, path) = temp_db();
    version_2_db(&path, ("2026-09-08", "2026-12-18"));
    // Something that isn't a file sits where the backup goes.
    std::fs::create_dir(dir.path().join("pagelamp.db.v2.bak")).unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    let record = store.last_migration_backup().unwrap().expect("recorded");
    match record.outcome {
        MigrationBackupOutcome::Failed { code } => {
            assert!(
                !code.is_empty() && !code.contains('/'),
                "a code, not a path: {code}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    // No half-written copy is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_new_database_is_not_backed_up() {
    let (dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    assert!(pagelamp_core::store::database_backup(&path).is_none());
    assert_eq!(
        store.last_migration_backup().unwrap(),
        None,
        "nothing was migrated"
    );
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".bak"))
        .collect();
    assert!(entries.is_empty(), "{entries:?}");
}

#[test]
fn the_v3_migration_drops_term_overrides_that_are_only_the_old_prefill() {
    // CAL-7: the v0.1 dates form saved the synced dates as overrides.
    for (user, expected) in [
        (("2026-05-01", "2026-12-31"), (None, None)),
        (("2026-09-08", "2026-12-31"), (Some("2026-09-08"), None)),
        (
            ("2026-09-08", "2026-12-18"),
            (Some("2026-09-08"), Some("2026-12-18")),
        ),
    ] {
        let (_dir, path) = temp_db();
        version_2_db(&path, user);
        let store = Store::open(&path).unwrap();
        let (start, end): (Option<String>, Option<String>) = store
            .conn()
            .query_row(
                "SELECT user_term_start, user_term_end FROM courses",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (start.as_deref(), end.as_deref()),
            expected,
            "user dates {user:?}"
        );
        assert_eq!(store.min_reader_version().unwrap(), Some(3));
    }
}

#[test]
fn settings_are_json_values_by_key() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.setting::<bool>("updates.auto_check").unwrap(), None);
    store.set_setting("updates.auto_check", &false).unwrap();
    store
        .set_setting("updates.channel", &serde_json::json!({"channel": "beta"}))
        .unwrap();
    store.set_setting("updates.auto_check", &true).unwrap();
    assert_eq!(
        store.setting::<bool>("updates.auto_check").unwrap(),
        Some(true)
    );
    assert_eq!(
        store
            .setting::<serde_json::Value>("updates.channel")
            .unwrap()
            .unwrap()["channel"],
        "beta"
    );
    // A value this version can't read is an error the caller can fall back from.
    assert!(matches!(
        store.setting::<u32>("updates.channel"),
        Err(Error::Invalid(_))
    ));
    store.remove_setting("updates.channel").unwrap();
    assert_eq!(store.setting::<bool>("updates.channel").unwrap(), None);
    // Readers see settings too.
    drop(store);
    let reader = Store::open_read_only(&path).unwrap();
    assert_eq!(
        reader.setting::<bool>("updates.auto_check").unwrap(),
        Some(true)
    );
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

// ----- chunks exist only for readable materials --------------------------------------------------

/// A folder course with one material of `text`, indexed (`ok`), at `local_path`.
fn indexed(store: &Store, local_path: &str, text: &str) -> String {
    store
        .upsert_source(&SourceRecord {
            id: "folder:demo".into(),
            kind: SourceKind::Folder,
            label: "Demo".into(),
            config: serde_json::json!({ "path": "/demo" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: "folder:demo/course/DEMO101".into(),
            source_id: "folder:demo".into(),
            external_id: "DEMO101".into(),
            code: Some("DEMO101".into()),
            name: "Demo".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    let material = material_at(local_path);
    store.upsert_material(&material).unwrap();
    store
        .set_text_state(&material.id, TextStatus::Ok, None, Some("hash"))
        .unwrap();
    store
        .replace_chunks(
            &material.id,
            &[Chunk {
                material_id: material.id.clone(),
                ord: 0,
                locator: Some("p. 1".into()),
                text: text.into(),
            }],
        )
        .unwrap();
    material.id
}

fn material_at(local_path: &str) -> MaterialUpsert {
    MaterialUpsert {
        id: "folder:demo/course/DEMO101/file/notes".into(),
        course_id: "folder:demo/course/DEMO101".into(),
        module_id: None,
        kind: MaterialKind::File,
        title: "Notes".into(),
        url: None,
        local_path: Some(local_path.into()),
        mime: None,
        published_at: None,
        week_hint: None,
    }
}

#[test]
fn a_material_that_stops_being_readable_loses_its_text() {
    for status in [
        TextStatus::NotDownloaded,
        TextStatus::Error,
        TextStatus::Unsupported,
        TextStatus::Pending,
    ] {
        let (_dir, path) = temp_db();
        let store = Store::open(&path).unwrap();
        let id = indexed(&store, "/demo/DEMO101/notes.md", "Stomata open in light.");
        assert_eq!(store.chunk_count(&id).unwrap(), 1);
        assert_eq!(store.search("stomata", None, 5).unwrap().len(), 1);

        store.set_text_state(&id, status, None, None).unwrap();
        assert_eq!(store.chunk_count(&id).unwrap(), 0, "{status:?}");
        assert!(
            store.search("stomata", None, 5).unwrap().is_empty(),
            "{status:?}"
        );
        // What MCP read_material serves: nothing.
        let read = pagelamp_core::views::read_material(&store, &id, 0, 12_000).unwrap();
        assert!(
            read.chunks.is_empty() && read.total_chunks == 0,
            "{status:?}"
        );
    }

    // Setting `ok` again (a re-index) keeps what the ingest just wrote.
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let id = indexed(&store, "/demo/DEMO101/notes.md", "Stomata open in light.");
    store
        .set_text_state(&id, TextStatus::Ok, None, Some("hash"))
        .unwrap();
    assert_eq!(store.chunk_count(&id).unwrap(), 1);
}

#[test]
fn only_a_readable_material_takes_chunks() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let id = indexed(&store, "/demo/DEMO101/notes.md", "Stomata open in light.");
    store
        .set_text_state(&id, TextStatus::NotDownloaded, None, None)
        .unwrap();
    let chunk = Chunk {
        material_id: id.clone(),
        ord: 0,
        locator: None,
        text: "Stale text.".into(),
    };
    assert!(matches!(
        store.replace_chunks(&id, std::slice::from_ref(&chunk)),
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.chunk_count(&id).unwrap(), 0);
    store.replace_chunks(&id, &[]).unwrap();
}

#[test]
fn a_material_that_moved_has_no_text_until_it_is_read_again() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let id = indexed(&store, "/demo/DEMO101/notes.md", "Stomata open in light.");

    // The same file again: text kept.
    store
        .upsert_material(&material_at("/demo/DEMO101/notes.md"))
        .unwrap();
    assert_eq!(store.chunk_count(&id).unwrap(), 1);

    // Another file now: pending, and the old text is gone until the ingest reads it.
    store
        .upsert_material(&material_at("/demo/DEMO101/renamed.md"))
        .unwrap();
    let material = store.get_material(&id).unwrap().unwrap();
    assert_eq!(material.text_status, TextStatus::Pending);
    assert_eq!(store.chunk_count(&id).unwrap(), 0);
    assert!(store.search("stomata", None, 5).unwrap().is_empty());
}

#[test]
fn upgrading_drops_the_text_of_materials_that_are_not_readable() {
    let (_dir, path) = temp_db();
    version_2_db(&path, ("2026-05-01", "2026-12-31"));
    let plain = rusqlite::Connection::open(&path).unwrap();
    for (id, status) in [
        ("locked", "not_downloaded"),
        ("indexed", "ok"),
        ("failed", "error"),
    ] {
        plain
            .execute(
                "INSERT INTO materials (id, course_id, kind, title, text_status, updated_at)
                 VALUES (?1, 'canvas:demo/course/101', 'file', ?1, ?2, '2026-09-20T00:00:00Z')",
                [id, status],
            )
            .unwrap();
        plain
            .execute(
                "INSERT INTO chunks (material_id, ord, text) VALUES (?1, 0, ?2)",
                [id, &format!("Stomata text of {id}")],
            )
            .unwrap();
    }
    drop(plain);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.chunk_count("indexed").unwrap(), 1);
    assert_eq!(store.chunk_count("locked").unwrap(), 0);
    assert_eq!(store.chunk_count("failed").unwrap(), 0);
    let hits = store.search("stomata", None, 5).unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| h.material_id.as_str())
            .collect::<Vec<_>>(),
        ["indexed"]
    );
    // Opening again finds nothing more to do.
    drop(store);
    assert_eq!(
        Store::open(&path).unwrap().chunk_count("indexed").unwrap(),
        1
    );
}

#[test]
fn an_unparseable_setting_is_absent_but_a_failed_read_is_an_error() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    store.set_setting("demo.flag", &true).unwrap();
    assert_eq!(
        store.setting_or_absent::<bool>("demo.flag").unwrap(),
        Some(true)
    );
    assert_eq!(store.setting_or_absent::<bool>("demo.none").unwrap(), None);

    // Another version's shape: absent (the strict read still says so).
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE settings SET value = 'not json' WHERE key = 'demo.flag'",
        [],
    )
    .unwrap();
    assert!(matches!(
        store.setting::<bool>("demo.flag"),
        Err(Error::Invalid(_))
    ));
    assert_eq!(store.setting_or_absent::<bool>("demo.flag").unwrap(), None);

    // A failed read (here the table is gone) is an error, never "absent".
    raw.execute_batch("ALTER TABLE settings RENAME TO settings_away")
        .unwrap();
    assert!(matches!(
        store.setting_or_absent::<bool>("demo.flag"),
        Err(Error::Db(_))
    ));
}

// ----- access parameters in stored text (`pagelamp_extract::scrub`) ----------------------------

const WITH_VERIFIER: &str =
    "handout (https://lms.example.edu/courses/101/files/7/download?verifier=Ab12Cd34Zz&wrap=1)";

/// One material with one chunk, a syllabus and a saved plan, each holding an address with an
/// access parameter, written as an earlier version could have stored them.
fn store_text_with_access_parameters(conn: &rusqlite::Connection) {
    conn.execute_batch(&format!(
        "INSERT INTO materials (id, course_id, kind, title, text_status, updated_at)
         VALUES ('canvas:demo/page/1', 'canvas:demo/course/101', 'page', 'Week 1', 'ok',
                 '2026-09-20T00:00:00Z');
         INSERT INTO chunks (material_id, ord, locator, text)
         VALUES ('canvas:demo/page/1', 0,
                 '§ Notes https://lms.example.edu/files/8/preview?verifier=Ab12Cd34Zz',
                 'zebrafish {WITH_VERIFIER} and more');
         UPDATE courses SET syllabus_text =
             'Outline: https://media.example.edu/v?t=5&access_token=Ab12Cd34Zz';
         INSERT INTO study_plans (created_at, plan_json)
         VALUES ('2026-09-20T00:00:00Z',
                 '{{\"horizon_start\":\"2026-09-21\",\"horizon_end\":\"2026-09-27\",\"items\":[],\"notes\":\"read https://lms.example.edu/pages/3?sf_verifier=Ab12Cd34Zz&x=1\"}}');"
    ))
    .unwrap();
}

/// Whether the file holds `what`, as written or in lowercase (the search index stores its
/// tokens in lowercase).
fn file_holds(path: &Path, what: &str) -> bool {
    let bytes = std::fs::read(path).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    text.contains(what) || text.contains(&what.to_lowercase())
}

const ADDRESS: &str =
    "https://lms.example.edu/courses/101/files/7/download?verifier=Ab12Cd34Zz&wrap=1";

/// Where in `ADDRESS` an old chunk can begin: on the "?", inside the parameter's name, on its
/// "=", and inside its value.
fn cuts_inside_the_address() -> [(&'static str, usize); 4] {
    let name = ADDRESS.find("verifier").unwrap();
    [
        ("question", ADDRESS.find('?').unwrap()),
        ("name", name + 3),
        ("equals", name + "verifier".len()),
        ("value", ADDRESS.find("Ab12").unwrap() + 4),
    ]
}

/// A material whose text has no spaces, cut into chunks as an earlier version did: without
/// cleaning it first, so the second chunk begins `into_address` characters into the address
/// (in text without spaces the chunker starts the overlap in the middle of a word). Returns
/// the whole text.
fn old_chunks_cut_inside_the_address(
    conn: &rusqlite::Connection,
    material: &str,
    into_address: usize,
) -> String {
    let chunk_chars = pagelamp_extract::DEFAULT_CHUNK_CHARS;
    let second_chunk_starts = chunk_chars - chunk_chars / 10;
    let text = format!(
        "{}{ADDRESS}，并在课前阅读{}",
        "字".repeat(second_chunk_starts - into_address),
        "文".repeat(600)
    );
    let chunks = pagelamp_extract::chunk_segments(
        &[pagelamp_extract::Segment {
            locator: Some("p. 1".into()),
            text: text.clone(),
        }],
        chunk_chars,
    );
    assert!(chunks.len() >= 2);
    assert!(chunks[0].text.contains(ADDRESS), "the first chunk holds it");
    assert!(
        chunks[1].text.starts_with(&ADDRESS[into_address..]),
        "the second begins inside it: {}",
        &chunks[1].text[..40]
    );
    conn.execute(
        "INSERT INTO materials (id, course_id, kind, title, text_status, updated_at)
         VALUES (?1, 'canvas:demo/course/101', 'page', 'Notes', 'ok', '2026-09-20T00:00:00Z')",
        [material],
    )
    .unwrap();
    for (ord, chunk) in chunks.iter().enumerate() {
        conn.execute(
            "INSERT INTO chunks (material_id, ord, locator, text) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![material, ord as i64, chunk.locator, chunk.text],
        )
        .unwrap();
    }
    text
}

#[test]
fn the_backup_made_before_a_migration_holds_no_access_parameter() {
    let (_dir, path) = temp_db();
    version_2_db(&path, ("2026-05-01", "2026-12-31"));
    {
        let plain = rusqlite::Connection::open(&path).unwrap();
        store_text_with_access_parameters(&plain);
        for (name, into_address) in cuts_inside_the_address() {
            old_chunks_cut_inside_the_address(
                &plain,
                &format!("canvas:demo/page/cut-{name}"),
                into_address,
            );
        }
    }
    assert!(file_holds(&path, "Ab12Cd34Zz"));

    // The first open of the new version cleans the text, then copies the database, then
    // migrates it.
    let store = Store::open(&path).unwrap();
    let backup = pagelamp_core::store::database_backup(&path).expect("a backup");
    // Not the value, not a piece of it at the start of a chunk, and not its token in the
    // search index (lowercase).
    for gone in ["Ab12Cd34Zz", "Cd34Zz", "verifier=", "access_token="] {
        assert!(!file_holds(&backup.path, gone), "{gone}");
    }
    // The copy is still the old database, with its text otherwise as it was.
    let copy = rusqlite::Connection::open(&backup.path).unwrap();
    let version: i64 = copy
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let text: String = copy
        .query_row(
            "SELECT text FROM chunks WHERE material_id = 'canvas:demo/page/1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        text,
        "zebrafish handout (https://lms.example.edu/courses/101/files/7/download) and more"
    );
    // The recorded clean-up still runs afterwards (nothing left to change here).
    assert_eq!(store.scrubbed_text_version().unwrap(), 0);
    assert!(store.scrub_stored_text_once().unwrap());
    assert_eq!(
        store.scrubbed_text_version().unwrap(),
        pagelamp_core::scrub::VERSION
    );
}

#[test]
fn stored_text_is_cleaned_once_per_version_of_the_rules() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    store.upsert_source(&demo_source("canvas:demo")).unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO courses (id, source_id, external_id, code, name, updated_at)
             VALUES ('canvas:demo/course/101', 'canvas:demo', '101', 'DEMO101', 'Intro',
                     '2026-09-20T00:00:00Z')",
            [],
        )
        .unwrap();
    // Two plans saved before the last one: one whose JSON escapes a line break before the parameter (cleaning
    // the JSON text would miss it, or break the JSON), and one that isn't JSON.
    store
        .conn()
        .execute_batch(
            r#"INSERT INTO study_plans (created_at, plan_json) VALUES
               ('2026-09-18T00:00:00Z',
                '{"horizon_start":"2026-09-14","horizon_end":"2026-09-20","items":[],"notes":"read https://lms.example.edu/pages/4?\nverifier=Ab12Cd34Zz&x=1\nthen rest"}'),
               ('2026-09-17T00:00:00Z', 'not a plan: verifier=Ab12Cd34Zz');"#,
        )
        .unwrap();

    store_text_with_access_parameters(store.conn());
    assert_eq!(store.search("Ab12Cd34Zz", None, 5).unwrap().len(), 1);

    assert!(store.scrub_stored_text_once().unwrap());
    let plans: Vec<String> = {
        let mut statement = store
            .conn()
            .prepare("SELECT plan_json FROM study_plans ORDER BY id")
            .unwrap();
        let rows = statement.query_map([], |row| row.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(plans[1], "not a plan: ");
    let escaped: serde_json::Value = serde_json::from_str(&plans[0]).expect("still JSON");
    assert_eq!(
        escaped["notes"],
        "read https://lms.example.edu/pages/4?\nx=1\nthen rest"
    );
    assert!(!plans.join(" ").contains("Ab12Cd34Zz"));
    let all: String = store
        .conn()
        .query_row(
            "SELECT (SELECT text || ' | ' || locator FROM chunks) || ' | '
                 || (SELECT syllabus_text FROM courses) || ' | '
                 || (SELECT plan_json FROM study_plans ORDER BY id DESC LIMIT 1)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for gone in ["Ab12Cd34Zz", "verifier", "access_token"] {
        assert!(!all.contains(gone), "{gone}: {all}");
    }
    assert!(
        all.contains("§ Notes https://lms.example.edu/files/8/preview |"),
        "{all}"
    );
    assert!(all.contains("https://media.example.edu/v?t=5 |"), "{all}");
    assert!(
        all.contains("read https://lms.example.edu/pages/3?x=1"),
        "{all}"
    );
    // The search index followed, and the plan is still a plan.
    assert!(store.search("Ab12Cd34Zz", None, 5).unwrap().is_empty());
    assert_eq!(store.search("zebrafish", None, 5).unwrap().len(), 1);
    let plan = store.latest_study_plan().unwrap().unwrap().plan;
    assert_eq!(
        plan.notes.as_deref(),
        Some("read https://lms.example.edu/pages/3?x=1")
    );

    // Done once: text written past the rules afterwards isn't looked for again.
    store
        .conn()
        .execute(
            "UPDATE courses SET syllabus_text = 'https://lms.example.edu/files/9/download?verifier=Later'",
            [],
        )
        .unwrap();
    assert!(!store.scrub_stored_text_once().unwrap());
    assert!(
        store
            .course_syllabus_text("canvas:demo/course/101")
            .unwrap()
            .unwrap()
            .contains("verifier=Later")
    );
}

/// An earlier version cut a text into chunks without cleaning it. In text without spaces a
/// chunk then begins in the middle of an address: on the "?", inside the parameter's name, on
/// its "=" or inside its value, where nothing shows that what follows gives access to a file.
/// The clean-up puts each material's text together again, cleans it and cuts it again.
#[test]
fn a_chunk_that_begins_inside_an_address_is_cleaned_with_its_material() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    store.upsert_source(&demo_source("canvas:demo")).unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO courses (id, source_id, external_id, code, name, updated_at)
             VALUES ('canvas:demo/course/101', 'canvas:demo', '101', 'DEMO101', 'Intro',
                     '2026-09-20T00:00:00Z')",
            [],
        )
        .unwrap();
    let mut texts = Vec::new();
    for (name, into_address) in cuts_inside_the_address() {
        let material = format!("canvas:demo/page/cut-{name}");
        let text = old_chunks_cut_inside_the_address(store.conn(), &material, into_address);
        texts.push((material, text));
    }
    // A material without an address is left exactly as it is.
    let untouched = "canvas:demo/page/plain";
    store
        .conn()
        .execute_batch(&format!(
            "INSERT INTO materials (id, course_id, kind, title, text_status, updated_at)
             VALUES ('{untouched}', 'canvas:demo/course/101', 'page', 'Plain', 'ok',
                     '2026-09-20T00:00:00Z');
             INSERT INTO chunks (material_id, ord, locator, text)
             VALUES ('{untouched}', 0, NULL, 'no address here'),
                    ('{untouched}', 5, NULL, 'and an odd ord that stays');"
        ))
        .unwrap();
    // The first chunk of each material holds the value, and so does the second of the three
    // that begin before it.
    assert_eq!(store.search("Ab12Cd34Zz", None, 10).unwrap().len(), 7);

    assert!(store.scrub_stored_text_once().unwrap());

    for (material, text) in &texts {
        let chunks = store.get_chunks(material, 0, None).unwrap();
        assert!(chunks.len() >= 2, "{material}");
        for chunk in &chunks {
            let lower = chunk.text.to_lowercase();
            // Not the value, not its second half, not the name, not what followed it.
            for gone in ["ab12cd34zz", "cd34zz", "verifier", "wrap=1"] {
                assert!(
                    !lower.contains(gone),
                    "{material} chunk {}: {gone}",
                    chunk.ord
                );
            }
            assert_eq!(chunk.locator.as_deref(), Some("p. 1"));
        }
        // The text is otherwise what it was: the address without its query, and every
        // character around it.
        let parts = pagelamp_core::calendar::text::rebuild_parts(&chunks);
        assert_eq!(parts.len(), 1, "{material}");
        assert_eq!(
            parts[0].text,
            text.replace("?verifier=Ab12Cd34Zz&wrap=1", ""),
            "{material}"
        );
        // Cut as this version cuts new text: the same chunks as indexing the clean text.
        let fresh = pagelamp_extract::chunk_segments(
            &[pagelamp_extract::Segment {
                locator: Some("p. 1".into()),
                text: parts[0].text.clone(),
            }],
            pagelamp_extract::DEFAULT_CHUNK_CHARS,
        );
        assert_eq!(
            chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            fresh.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            "{material}"
        );
    }
    let plain = store.get_chunks(untouched, 0, None).unwrap();
    assert_eq!(
        plain
            .iter()
            .map(|c| (c.ord, c.text.as_str()))
            .collect::<Vec<_>>(),
        [(0, "no address here"), (5, "and an odd ord that stays")]
    );
    // The search index followed: the value finds nothing, the text is still found.
    assert!(store.search("Ab12Cd34Zz", None, 10).unwrap().is_empty());
    assert!(store.search("Cd34Zz", None, 10).unwrap().is_empty());
    assert!(!store.search("download", None, 10).unwrap().is_empty());
    // And its older segments were merged away: a copy of the file holds no token of it.
    let copy = path.with_extension("copy");
    store
        .conn()
        .execute("VACUUM INTO ?1", [copy.to_str().unwrap()])
        .unwrap();
    for gone in ["Ab12Cd34Zz", "Cd34Zz"] {
        assert!(!file_holds(&copy, gone), "{gone}");
    }
}

/// A store with one course, and a way to put a material's segments into it cut as an earlier
/// version cut them (without cleaning). Returns the stored chunks as (locator, text).
fn store_with_course(path: &Path) -> Store {
    let store = Store::open(path).unwrap();
    store.upsert_source(&demo_source("canvas:demo")).unwrap();
    store
        .conn()
        .execute(
            "INSERT INTO courses (id, source_id, external_id, code, name, updated_at)
             VALUES ('canvas:demo/course/101', 'canvas:demo', '101', 'DEMO101', 'Intro',
                     '2026-09-20T00:00:00Z')",
            [],
        )
        .unwrap();
    store
}

fn old_material(
    store: &Store,
    material: &str,
    segments: &[(&str, String)],
) -> Vec<(Option<String>, String)> {
    let segments: Vec<pagelamp_extract::Segment> = segments
        .iter()
        .map(|(locator, text)| pagelamp_extract::Segment {
            locator: Some((*locator).to_string()),
            text: text.clone(),
        })
        .collect();
    let chunks = pagelamp_extract::chunk_segments(&segments, pagelamp_extract::DEFAULT_CHUNK_CHARS);
    store
        .conn()
        .execute(
            "INSERT INTO materials (id, course_id, kind, title, text_status, updated_at)
             VALUES (?1, 'canvas:demo/course/101', 'page', 'Notes', 'ok', '2026-09-20T00:00:00Z')",
            [material],
        )
        .unwrap();
    for (ord, chunk) in chunks.iter().enumerate() {
        store
            .conn()
            .execute(
                "INSERT INTO chunks (material_id, ord, locator, text) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![material, ord as i64, chunk.locator, chunk.text],
            )
            .unwrap();
    }
    chunks
        .into_iter()
        .map(|chunk| (chunk.locator, chunk.text))
        .collect()
}

fn stored_chunks(store: &Store, material: &str) -> Vec<(Option<String>, String)> {
    store
        .get_chunks(material, 0, None)
        .unwrap()
        .into_iter()
        .map(|chunk| (chunk.locator, chunk.text))
        .collect()
}

/// Sentences that don't repeat, about `chars` characters of them.
fn prose(seed: usize, chars: usize) -> String {
    let mut text = String::new();
    let mut n = seed;
    while text.len() < chars {
        n += 1;
        text.push_str(&format!(
            "Sentence {n} of part {seed} explains idea {} with example {}. ",
            n * 7 % 13,
            n * 11 % 17
        ));
        if n.is_multiple_of(6) {
            // (No space before a blank line: the chunker trims what it cuts there.)
            text.truncate(text.trim_end().len());
            text.push_str("\n\n");
        }
    }
    text.trim().to_string()
}

/// An ordinary material: several pages, two sections with the same heading, one address with
/// a parameter. After the clean-up its text is what it was without the parameter, its
/// locators are the same, and every chunk the address isn't in is the same character for
/// character.
#[test]
fn an_ordinary_material_is_rewritten_without_losing_a_character() {
    let (_dir, path) = temp_db();
    let store = store_with_course(&path);
    let material = "canvas:demo/page/english";
    let with_address = format!(
        "{} The handout is at {ADDRESS} for everyone. {}",
        prose(1, 2500),
        prose(2, 2200)
    );
    let segments = [
        ("p. 1", with_address.clone()),
        ("p. 2", prose(3, 4100)),
        ("§ Notes", prose(4, 900)),
        ("§ Notes", prose(5, 1300)),
    ];
    let before = old_material(&store, material, &segments);
    assert!(before.len() >= 7, "{}", before.len());
    let holding: Vec<usize> = (0..before.len())
        .filter(|i| before[*i].1.contains("Ab12Cd34Zz"))
        .collect();
    assert!(!holding.is_empty());

    assert!(
        store
            .in_transaction(|store| store.scrub_stored_text())
            .unwrap()
            >= 1
    );

    let after = stored_chunks(&store, material);
    let chunks = store.get_chunks(material, 0, None).unwrap();
    assert_eq!(
        chunks.iter().map(|c| c.ord).collect::<Vec<_>>(),
        (0..after.len() as u32).collect::<Vec<_>>()
    );
    for (_, text) in &after {
        assert!(!text.to_lowercase().contains("ab12cd34zz"), "{text}");
        assert!(!text.contains("verifier"), "{text}");
    }
    // The text, part by part: the first page lost the query and nothing else, the others
    // are as they were, and the two sections with one heading are still two.
    let parts = pagelamp_core::calendar::text::rebuild_parts(&chunks);
    let expected: Vec<(Option<String>, String)> = segments
        .iter()
        .map(|(locator, text)| {
            (
                Some((*locator).to_string()),
                text.replace("?verifier=Ab12Cd34Zz&wrap=1", ""),
            )
        })
        .collect();
    assert_eq!(
        parts
            .into_iter()
            .map(|part| (part.locator, part.text))
            .collect::<Vec<_>>(),
        expected
    );
    // Chunk by chunk: everything before the address, and every other part, is untouched.
    let first = holding[0];
    assert_eq!(after[..first], before[..first]);
    let other_parts = |chunks: &[(Option<String>, String)]| -> Vec<(Option<String>, String)> {
        chunks
            .iter()
            .filter(|(locator, _)| locator.as_deref() != Some("p. 1"))
            .cloned()
            .collect()
    };
    assert_eq!(other_parts(&after), other_parts(&before));
}

/// Putting chunks together again is a guess at where they overlap. In text that repeats (a
/// column of numbers set apart by spaces) a longer overlap matches than the real one, and
/// cutting that guess again would drop text. Such a material is cleaned chunk by chunk
/// instead, and loses nothing.
#[test]
fn a_material_that_cannot_be_put_together_exactly_is_cleaned_chunk_by_chunk() {
    let (_dir, path) = temp_db();
    let store = store_with_course(&path);
    let material = "canvas:demo/page/table";
    let column = "0.00   ".repeat(400);
    let before = old_material(
        &store,
        material,
        &[
            ("p. 1", column.clone()),
            (
                "p. 2",
                format!("The table above is from the handout ({ADDRESS})."),
            ),
        ],
    );
    // The guess is wrong here: this is the case the check is for.
    let guess =
        pagelamp_core::calendar::text::rebuild_parts(&store.get_chunks(material, 0, None).unwrap());
    assert_ne!(
        guess[0].text,
        column.trim(),
        "the overlap is guessed too long"
    );

    assert_eq!(
        store
            .in_transaction(|store| store.scrub_stored_text())
            .unwrap(),
        1
    );

    let after = stored_chunks(&store, material);
    assert_eq!(after.len(), before.len());
    let last = before.len() - 1;
    assert_eq!(after[..last], before[..last], "the column is untouched");
    assert_eq!(
        after[last],
        (
            Some("p. 2".to_string()),
            "The table above is from the handout \
             (https://lms.example.edu/courses/101/files/7/download)."
                .to_string()
        )
    );
    assert!(store.search("Ab12Cd34Zz", None, 5).unwrap().is_empty());
}

/// An address wrapped by a gateway carries the parameter percent-encoded, inside the value of
/// a pair. Cut by a chunk's end in the middle of the name, neither chunk shows it: the first
/// ends before the name is complete, the second begins inside the value, where nothing says
/// what follows. Only the text put together again does.
#[test]
fn a_parameter_cut_by_a_chunks_end_is_found_in_the_text_put_together() {
    let (_dir, path) = temp_db();
    let store = store_with_course(&path);
    let material = "canvas:demo/page/wrapped";
    let before_name = format!(
        "https://gw.example.org/?url=https%3A%2F%2Flms.example.edu%2F{}files%2F7%3F",
        "a%2F".repeat(60)
    );
    let chunk_chars = pagelamp_extract::DEFAULT_CHUNK_CHARS;
    // The first chunk ends five characters into "verifier".
    let text = format!(
        "{}{before_name}verifier%3DAb12Cd34Zz%26wrap%3D1，并在课前阅读{}",
        "字".repeat(chunk_chars - before_name.len() - 5),
        "文".repeat(600)
    );
    let before = old_material(&store, material, &[("p. 1", text.clone())]);
    assert!(
        before[0].1.ends_with("%3Fverif"),
        "{}",
        &before[0].1[before[0].1.len() - 30..]
    );
    assert!(before[1].1.contains("verifier%3DAb12Cd34Zz"));
    for (_, chunk) in &before {
        assert!(
            matches!(
                pagelamp_core::scrub::scrub_text(chunk),
                std::borrow::Cow::Borrowed(_)
            ),
            "no chunk shows it by itself"
        );
    }

    assert_eq!(
        store
            .in_transaction(|store| store.scrub_stored_text())
            .unwrap(),
        1
    );

    let chunks = store.get_chunks(material, 0, None).unwrap();
    for chunk in &chunks {
        assert!(!chunk.text.contains("Ab12Cd34Zz"), "chunk {}", chunk.ord);
        assert!(!chunk.text.contains("verif"), "chunk {}", chunk.ord);
    }
    let parts = pagelamp_core::calendar::text::rebuild_parts(&chunks);
    assert_eq!(parts.len(), 1);
    assert!(
        parts[0]
            .text
            .contains("字https://gw.example.org/，并在课前阅读文"),
        "the pair that carried it is gone, the rest stays"
    );
}

/// A title that is nothing but an access parameter is an empty title once it is cleaned: the
/// plan is refused, not stored with an empty title.
#[test]
fn a_plan_is_validated_as_it_will_be_stored() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let mut plan = demo_plan();
    plan.items.push(StudyPlanItem {
        date: plan.horizon_start,
        course_id: None,
        title: "sf_verifier=abc".into(),
        description: None,
        material_ids: Vec::new(),
        minutes: Some(30),
        done: false,
    });
    let refused = store.save_study_plan(&plan).unwrap_err();
    assert!(matches!(refused, Error::Invalid(_)), "{refused}");
    assert!(store.latest_study_plan().unwrap().is_none());
}

/// While another connection holds the write lock the clean-up is refused and nothing is
/// recorded; afterwards it runs.
#[test]
fn a_cleanup_that_cannot_get_the_database_is_refused_and_not_recorded() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let other = rusqlite::Connection::open(&path).unwrap();
    other.execute_batch("BEGIN IMMEDIATE").unwrap();
    let refused = store.scrub_stored_text_once().unwrap_err();
    assert!(refused.is_database_busy(), "{refused}");
    other.execute_batch("ROLLBACK").unwrap();
    assert_eq!(store.scrubbed_text_version().unwrap(), 0);
    assert!(store.scrub_stored_text_once().unwrap());
    assert!(!store.scrub_stored_text_once().unwrap());
}

/// A plan an AI app saves with a copied address is stored, and returned, without the
/// parameter.
#[test]
fn a_saved_plan_keeps_no_access_parameter() {
    let (_dir, path) = temp_db();
    let store = Store::open(&path).unwrap();
    let mut plan = demo_plan();
    plan.notes = Some(format!("Start with the {WITH_VERIFIER}."));
    let saved = store.save_study_plan(&plan).unwrap();
    let clean = "Start with the handout (https://lms.example.edu/courses/101/files/7/download).";
    assert_eq!(saved.plan.notes.as_deref(), Some(clean));
    let stored = store.latest_study_plan().unwrap().unwrap();
    assert_eq!(stored.plan.notes.as_deref(), Some(clean));
    assert_eq!(stored.plan.items.len(), plan.items.len());
    drop(store);
    assert!(!file_holds(&path, "Ab12Cd34Zz"));
}
