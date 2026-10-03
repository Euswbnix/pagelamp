//! Automatic sync through the facade: the setting, `StartupTasks.sync_due`, and what a run
//! PageLamp starts by itself does differently (`SyncRequest.automatic`). Temporary data dirs,
//! in-memory secrets, a local mock server for the calendar feed; synthetic data only.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeDelta, Utc};
use pagelamp_app::{
    App, AppErrorKind, AutoSync, AutoSyncTrigger, SyncDue, SyncEvent, SyncPrefs, SyncRequest,
};
use pagelamp_core::auto_sync::{AUTO_SYNC_ATTEMPTS_KEY, AutoSyncAttempts};
use pagelamp_core::model::SourceErrorKind;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::Store;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn open(data: &Path) -> App {
    App::open_at_with_secrets(data.to_path_buf(), Arc::new(MemorySecrets::new())).unwrap()
}

fn data_dir(temp: &tempfile::TempDir) -> PathBuf {
    temp.path().join("data")
}

fn store(data: &Path) -> Store {
    Store::open(&data.join("pagelamp.db")).unwrap()
}

/// `<dir>/<name>/DEMO101 Intro/notes.txt`; returns the folder to add as a source.
fn course_folder(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(root.join("DEMO101 Intro")).unwrap();
    std::fs::write(root.join("DEMO101 Intro/notes.txt"), "demo").unwrap();
    root
}

/// The source's last successful sync was `hours` ago.
fn synced_hours_ago(data: &Path, source_id: &str, hours: i64) {
    store(data)
        .record_sync(source_id, Utc::now() - TimeDelta::hours(hours), None)
        .unwrap();
}

fn attempts(data: &Path) -> AutoSyncAttempts {
    store(data)
        .setting_or_absent(AUTO_SYNC_ATTEMPTS_KEY)
        .unwrap()
        .unwrap_or_default()
}

fn set(app: &App, auto_sync: AutoSync) {
    app.set_sync_prefs(SyncPrefs { auto_sync }).unwrap();
}

fn automatic() -> SyncRequest {
    SyncRequest {
        automatic: Some(AutoSyncTrigger::Unattended),
        ..SyncRequest::default()
    }
}

/// A calendar feed with one event, answering at `/feed.ics`.
const FEED: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Demo//EN\r\nBEGIN:VEVENT\r\n\
                    UID:ps1\r\nDTSTART:20300930T035900Z\r\nDTEND:20300930T035900Z\r\n\
                    SUMMARY:Problem Set 1\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

/// Whether an automatic sync is due at `at`. Both triggers answer the same today.
fn due(app: &App, at: DateTime<Utc>) -> bool {
    let due = app.startup_tasks(at).unwrap().sync_due;
    assert_eq!(due.unattended, due.attended);
    due.unattended
}

#[test]
fn the_setting_is_twice_a_day_until_chosen_and_a_failed_read_is_never_the_default() {
    let temp = tempfile::tempdir().unwrap();
    let data = data_dir(&temp);
    let app = open(&data);
    assert_eq!(app.sync_prefs().unwrap().auto_sync, AutoSync::TwiceDaily);
    assert_eq!(app.status().unwrap().auto_sync, AutoSync::TwiceDaily);
    for setting in [AutoSync::Off, AutoSync::Daily, AutoSync::TwiceDaily] {
        set(&app, setting);
        assert_eq!(open(&data).sync_prefs().unwrap().auto_sync, setting);
        assert_eq!(app.status().unwrap().auto_sync, setting);
    }
    // A value another version wrote reads as the default; a table that can't be read is an
    // error: the student's "off" must never come back as "on".
    store(&data)
        .set_setting("sync.prefs", &serde_json::json!({ "auto_sync": "hourly" }))
        .unwrap();
    assert_eq!(app.sync_prefs().unwrap(), SyncPrefs::default());
    set(&app, AutoSync::Off);
    let raw = rusqlite::Connection::open(data.join("pagelamp.db")).unwrap();
    raw.execute_batch("ALTER TABLE settings RENAME TO settings_away")
        .unwrap();
    assert!(app.sync_prefs().is_err());
    assert!(app.startup_tasks(Utc::now()).is_err());
    assert!(app.status().is_err());
}

#[test]
fn sync_due_follows_the_setting_the_clock_and_a_running_sync() {
    let temp = tempfile::tempdir().unwrap();
    let data = data_dir(&temp);
    let app = open(&data);
    let now = Utc::now();
    assert!(!due(&app, now), "no sources");

    let source = app
        .add_folder_source(&course_folder(temp.path(), "Courses"), None, None)
        .unwrap();
    assert!(due(&app, now), "never synced");
    synced_hours_ago(&data, &source.id, 11);
    assert!(!due(&app, now), "11 h old, twice a day");
    assert!(due(&app, now + TimeDelta::hours(1)), "12 h old");
    set(&app, AutoSync::Daily);
    assert!(
        !due(&app, now + TimeDelta::hours(12)),
        "23 h old, once a day"
    );
    assert!(due(&app, now + TimeDelta::hours(13)), "24 h old");
    set(&app, AutoSync::Off);
    assert!(!due(&app, now + TimeDelta::days(30)), "off");

    // Nothing while a sync runs, in this process or another (`sync.lock`).
    set(&app, AutoSync::TwiceDaily);
    synced_hours_ago(&data, &source.id, 13);
    assert!(due(&app, now));
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(!due(&app, now), "a sync is running");
    drop(lock);
    assert!(due(&app, now));
}

/// An upgrader reads What's new (where the row lets them turn it off) before the first
/// automatic run: nothing is due while it waits, a run asked for anyway is refused and
/// counted, and the setting can be changed meanwhile.
#[tokio::test]
async fn nothing_runs_while_whats_new_waits() {
    let temp = tempfile::tempdir().unwrap();
    let data = data_dir(&temp);
    open(&data)
        .add_folder_source(&course_folder(temp.path(), "Courses"), None, None)
        .unwrap();
    // 0.1 never recorded its version; a new process starts.
    store(&data).remove_setting("app.last_run_version").unwrap();
    let app = open(&data);
    let now = Utc::now();
    let tasks = app.startup_tasks(now).unwrap();
    assert!(tasks.whats_new.is_some());
    assert_eq!(
        tasks.sync_due,
        SyncDue::default(),
        "not while What's new waits"
    );

    let started = Mutex::new(0);
    let refused = app
        .sync_all(automatic(), |_| *started.lock().unwrap() += 1)
        .await
        .unwrap_err();
    assert_eq!(refused.kind, AppErrorKind::Invalid);
    assert_eq!(*started.lock().unwrap(), 0, "nothing started");
    assert_eq!(attempts(&data).failed, 1, "a refusal is counted");

    // The row's control works before the sheet is closed.
    set(&app, AutoSync::Off);
    app.acknowledge_whats_new().unwrap();
    assert!(!due(&app, now + TimeDelta::days(2)), "turned off there");
    set(&app, AutoSync::TwiceDaily);
    assert!(!due(&app, now), "the refusal's wait: an hour");
    assert!(due(&app, now + TimeDelta::minutes(61)));
}

#[tokio::test]
async fn an_automatic_run_is_counted_first_stays_quiet_and_leaves_what_needs_the_student() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/feed.ics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(FEED))
        .mount(&server)
        .await;
    let temp = tempfile::tempdir().unwrap();
    let data = data_dir(&temp);
    let app = open(&data);
    let folder = app
        .add_folder_source(&course_folder(temp.path(), "Courses"), None, None)
        .unwrap();
    let gone = app
        .add_folder_source(&course_folder(temp.path(), "Gone"), None, None)
        .unwrap();
    let feed = app
        .add_ical_source(&format!("{}/feed.ics", server.uri()), Some("Feed"))
        .await
        .unwrap();
    assert!(
        app.sync_all(SyncRequest::default(), |_| {})
            .await
            .unwrap()
            .ok
    );
    let now = Utc::now();
    assert!(!due(&app, now), "just synced");
    // A sync that isn't due does nothing and counts nothing.
    let nothing = app.sync_all(automatic(), |_| {}).await.unwrap();
    assert!(nothing.ok && nothing.results.is_empty());
    assert_eq!(attempts(&data), AutoSyncAttempts::default());

    // A folder disappears, and the student's own sync says so: it needs the student now.
    std::fs::remove_dir_all(temp.path().join("Gone")).unwrap();
    let failed = app
        .sync_source(&gone.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(failed.error_kind, Some(SourceErrorKind::NotFound));
    // Everything is 13 hours old, and the feed's server is having a bad day.
    for id in [&folder.id, &feed.id] {
        synced_hours_ago(&data, id, 13);
    }
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/feed.ics"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert!(due(&app, Utc::now()));

    let tried = Mutex::new(Vec::new());
    let summary = app
        .sync_all(automatic(), |event| {
            if let SyncEvent::SourceStarted { source_id, .. } = event {
                tried.lock().unwrap().push(source_id);
            }
        })
        .await
        .unwrap();
    // The missing folder was left alone; the feed failed and the folder synced.
    assert_eq!(*tried.lock().unwrap(), [folder.id.clone(), feed.id.clone()]);
    assert!(!summary.ok);
    let feed_result = &summary.results[1];
    assert_eq!(feed_result.error_kind, Some(SourceErrorKind::Other));
    // Quiet: the feed isn't marked failed (nothing asks for the student's attention), and it
    // is still as old as it was.
    let sources = app.list_sources().unwrap();
    let row = |id: &str| sources.iter().find(|s| s.id == id).unwrap();
    assert_eq!(
        (
            row(&feed.id).last_error.clone(),
            row(&feed.id).last_error_kind
        ),
        (None, None)
    );
    assert!(Utc::now() - row(&feed.id).last_synced_at.unwrap() >= TimeDelta::hours(13));
    assert!(Utc::now() - row(&folder.id).last_synced_at.unwrap() < TimeDelta::minutes(5));
    assert_eq!(
        row(&gone.id).last_error_kind,
        Some(SourceErrorKind::NotFound),
        "still shown on Sources"
    );

    // Counted, so nothing retries back to back: not due now, due again an hour later.
    let counted = attempts(&data);
    assert_eq!((counted.failed, counted.last_ok_at), (1, None));
    assert_eq!(counted.by_source.len(), 2, "the two it tried");
    let now = Utc::now();
    assert!(!due(&app, now));
    let again = app.sync_all(automatic(), |_| {}).await.unwrap();
    assert!(again.results.is_empty(), "not due: nothing ran");
    assert_eq!(attempts(&data).failed, 1, "and nothing was counted");
    assert!(!due(&app, now + TimeDelta::minutes(55)));
    assert!(due(&app, now + TimeDelta::minutes(61)));

    // A sync the student starts says what failed, as always.
    let feed_error = || {
        app.list_sources()
            .unwrap()
            .into_iter()
            .find(|s| s.id == feed.id)
            .unwrap()
            .last_error_kind
    };
    let manual = app
        .sync_source(&feed.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(!manual.ok);
    assert_eq!(feed_error(), Some(SourceErrorKind::Other));

    // The feed's link is revoked: the next automatic run records that (only the student can
    // fix it), and after it the feed is left alone.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/feed.ics"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let wait_is_over = |data: &Path| {
        let mut record = attempts(data);
        record.last_at = Some(Utc::now() - TimeDelta::hours(13));
        store(data)
            .set_setting(AUTO_SYNC_ATTEMPTS_KEY, &record)
            .unwrap();
    };
    wait_is_over(&data);
    let revoked = app.sync_all(automatic(), |_| {}).await.unwrap();
    assert_eq!(revoked.results.len(), 2, "every source it can try");
    assert_eq!(
        revoked.results[1].error_kind,
        Some(SourceErrorKind::AuthExpiredOrRevoked)
    );
    assert_eq!(feed_error(), Some(SourceErrorKind::AuthExpiredOrRevoked));
    wait_is_over(&data);
    assert!(
        !due(&app, Utc::now()),
        "the folder is fresh and the feed needs the student"
    );
    synced_hours_ago(&data, &folder.id, 13);
    let later = app.sync_all(automatic(), |_| {}).await.unwrap();
    assert!(later.ok);
    assert_eq!(later.results.len(), 1);
    assert_eq!(later.results[0].source_id, folder.id);
}

#[tokio::test]
async fn a_run_that_ends_well_clears_the_wait_and_one_refused_by_a_running_sync_is_counted() {
    let temp = tempfile::tempdir().unwrap();
    let data = data_dir(&temp);
    let app = open(&data);
    let folder = app
        .add_folder_source(&course_folder(temp.path(), "Courses"), None, None)
        .unwrap();

    // Another process is syncing: refused as busy, and counted before the lock was tried.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let busy = app.sync_all(automatic(), |_| {}).await.unwrap_err();
    assert_eq!(busy.kind, AppErrorKind::Busy);
    assert_eq!(attempts(&data).failed, 1);
    drop(lock);
    let now = Utc::now();
    assert!(!due(&app, now), "the refusal's wait");

    // An hour later the run happens and ends well: no wait is left.
    let mut record = attempts(&data);
    record.last_at = Some(now - TimeDelta::minutes(61));
    store(&data)
        .set_setting(AUTO_SYNC_ATTEMPTS_KEY, &record)
        .unwrap();
    let summary = app.sync_all(automatic(), |_| {}).await.unwrap();
    assert!(summary.ok);
    assert_eq!(summary.results.len(), 1);
    let record = attempts(&data);
    assert_eq!(record.failed, 0);
    assert!(record.last_ok_at.is_some());
    assert_eq!(record.by_source[&folder.id].len(), 2);
    assert!(!due(&app, Utc::now()), "fresh");
    assert!(due(&app, Utc::now() + TimeDelta::hours(12)));

    // Only `sync_all` runs by itself: the flag changes nothing for one source.
    let one = app
        .sync_source(&folder.id, automatic(), |_| {})
        .await
        .unwrap();
    assert!(one.ok);
    assert_eq!(attempts(&data), record);
}
