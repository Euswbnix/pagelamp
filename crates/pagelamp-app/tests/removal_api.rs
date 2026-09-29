//! Removing courses in two stages (docs/design/v0.3-course-calendar.md §8.3–§8.5): the preview,
//! stage 1 and undo, the purge with the Trash (a mock: the real one is never touched here),
//! restoring, forgetting, and what a sync does with removed courses. Synthetic data only.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use pagelamp_app::trash::FileTrash;
use pagelamp_app::{
    App, AppErrorKind, LostAfterPurge, RemovalReason, RemoveOptions, SyncRequest, TombstoneState,
};
use pagelamp_core::model::*;
use pagelamp_core::paths;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::Store;
use serde_json::json;

const CANVAS: &str = "canvas:lms.example.edu";

/// A Trash that moves into a folder of its own, and can be made to fail.
#[derive(Default)]
struct MockTrash {
    moved: Mutex<Vec<PathBuf>>,
    fail: AtomicBool,
    bin: PathBuf,
}

impl FileTrash for MockTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("couldn't be moved to the Trash".into());
        }
        let target = self
            .bin
            .join(format!("{}", self.moved.lock().unwrap().len()));
        std::fs::rename(path, &target).map_err(|e| e.to_string())?;
        self.moved.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    app: App,
    trash: Arc<MockTrash>,
    folder_source: String,
    /// The Canvas course's download folder.
    downloads: PathBuf,
}

fn options() -> RemoveOptions {
    RemoveOptions {
        reason: None,
        keep_downloaded_files: false,
        purge_now: false,
        delete_pre_update_backup: false,
    }
}

/// A folder source with DEMO101 and DEMO202, and a Canvas course DEMO303 with one downloaded
/// file and a deadline.
async fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open_at_with_secrets(temp.path().join("data"), Arc::new(MemorySecrets::new()))
        .unwrap();
    let bin = temp.path().join("trash-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let trash = Arc::new(MockTrash {
        bin,
        ..MockTrash::default()
    });
    app.set_trash(trash.clone());

    let courses = temp.path().join("Courses");
    for (dir, text) in [
        ("DEMO101 Intro", "Photosynthesis basics"),
        ("DEMO202 Methods", "Field sampling notes"),
    ] {
        std::fs::create_dir_all(courses.join(dir)).unwrap();
        std::fs::write(courses.join(dir).join("notes.txt"), text).unwrap();
    }
    let source = app.add_folder_source(&courses, None, None).unwrap();
    app.sync_source(&source.id, SyncRequest::default(), |_| {})
        .await
        .unwrap();

    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_source(&SourceRecord {
            id: CANVAS.into(),
            kind: SourceKind::Canvas,
            label: "Demo LMS".into(),
            config: json!({ "base_url": "https://lms.example.edu" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    let course_id = format!("{CANVAS}/course/303");
    store
        .upsert_course(&CourseUpsert {
            id: course_id.clone(),
            source_id: CANVAS.into(),
            external_id: "303".into(),
            code: Some("DEMO303".into()),
            name: "Demo Seminar".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    let downloads = pagelamp_canvas::course_files_dir(
        &paths::files_dir_in(app.data_dir()),
        Some("DEMO303"),
        "303",
    );
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(downloads.join("slides.txt"), "chlorophyll slides").unwrap();
    let material = format!("{CANVAS}/file/901");
    store
        .upsert_material(&MaterialUpsert {
            id: material.clone(),
            course_id: course_id.clone(),
            module_id: None,
            kind: MaterialKind::File,
            title: "slides.txt".into(),
            url: None,
            local_path: Some(downloads.join("slides.txt").to_string_lossy().into_owned()),
            mime: None,
            published_at: None,
            week_hint: None,
        })
        .unwrap();
    store
        .set_text_state(&material, TextStatus::Ok, None, Some("h"))
        .unwrap();
    store
        .replace_chunks(
            &material,
            &[Chunk {
                material_id: material.clone(),
                ord: 0,
                locator: None,
                text: "chlorophyll slides".into(),
            }],
        )
        .unwrap();
    store
        .replace_events(
            CANVAS,
            &[Event {
                id: format!("{CANVAS}/assignment/1"),
                source_id: CANVAS.into(),
                course_id: Some(course_id),
                kind: EventKind::AssignmentDue,
                title: "Essay".into(),
                starts_at: None,
                ends_at: None,
                due_at: Some(Utc::now() + Duration::days(3)),
                url: None,
                updated_at: Utc::now(),
                course_hint: None,
            }],
        )
        .unwrap();
    Fixture {
        temp,
        app,
        trash,
        folder_source: source.id,
        downloads,
    }
}

fn codes(app: &App) -> Vec<String> {
    app.list_courses()
        .unwrap()
        .into_iter()
        .filter_map(|s| s.course.code)
        .collect()
}

#[tokio::test]
async fn the_preview_says_what_goes_and_what_stays() {
    let f = fixture().await;
    let preview = f
        .app
        .removal_preview(vec!["DEMO101".into(), "DEMO303".into()])
        .unwrap();
    let folder = &preview.items[0];
    assert!(folder.own_folder_untouched);
    assert_eq!(folder.materials, 1);
    assert!(
        folder.lost_after_purge.is_empty(),
        "read again from the folder"
    );
    let canvas = &preview.items[1];
    assert_eq!((canvas.downloaded_files, canvas.downloaded_bytes), (1, 18));
    assert_eq!(canvas.deadlines, 1);
    assert!(
        canvas
            .lost_after_purge
            .contains(&LostAfterPurge::RedownloadCountsAsViewing)
    );
    assert!(!canvas.custom_settings);
    assert!(
        preview.backup.is_none(),
        "a fresh data dir has no pre-update backup"
    );
}

#[tokio::test]
async fn stage_one_hides_at_once_and_undo_puts_everything_back() {
    let f = fixture().await;
    f.app
        .set_course_policy("DEMO101", AiPolicy::Prohibited, None)
        .unwrap();
    let report = f
        .app
        .remove_courses(vec!["DEMO101".into()], options())
        .await
        .unwrap();
    let removed = &report.removed[0];
    assert_eq!(removed.state, TombstoneState::Pending);
    assert_eq!(removed.purge_in_days, Some(7));
    assert_eq!(removed.reason, RemovalReason::Other);
    assert!(!report.purged_now);
    assert!(!codes(&f.app).contains(&"DEMO101".to_string()));
    assert_eq!(
        f.app.course_overview("DEMO101").unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert_eq!(f.app.removed_courses().unwrap().len(), 1);
    // Removing it again: it isn't a course any more.
    let err = f
        .app
        .remove_courses(vec!["DEMO101".into()], options())
        .await
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::NotFound);
    // A sync leaves it out, and keeps it until its purge.
    f.app
        .sync_source(&f.folder_source, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(!codes(&f.app).contains(&"DEMO101".to_string()));
    // Undo: back as it was, settings included.
    let outcome = f.app.restore_course(&removed.removed_id).await.unwrap();
    assert!(outcome.restored);
    assert!(codes(&f.app).contains(&"DEMO101".to_string()));
    let course = f.app.course_overview("DEMO101").unwrap().course;
    assert_eq!(course.ai_policy, AiPolicy::Prohibited);
    assert!(!course.hidden);
    assert!(f.app.removed_courses().unwrap().is_empty());
}

#[tokio::test]
async fn purging_deletes_the_data_and_moves_downloads_to_the_trash() {
    let f = fixture().await;
    let report = f
        .app
        .remove_courses(
            vec!["DEMO303".into()],
            RemoveOptions {
                purge_now: true,
                ..options()
            },
        )
        .await
        .unwrap();
    assert!(report.purged_now);
    let removed = &report.removed[0];
    assert_eq!(removed.state, TombstoneState::Purged);
    assert!(!removed.files_pending);
    let store = Store::open(&f.app.db_path()).unwrap();
    assert!(store.get_course(&removed.course_id).unwrap().is_none());
    assert!(store.search("chlorophyll", None, 5).unwrap().is_empty());
    let events = store
        .list_events(
            chrono::DateTime::<Utc>::MIN_UTC,
            chrono::DateTime::<Utc>::MAX_UTC,
            None,
        )
        .unwrap();
    assert!(
        events.iter().all(|e| e.source_id != CANVAS),
        "its deadline is gone"
    );
    assert_eq!(
        *f.trash.moved.lock().unwrap(),
        std::slice::from_ref(&f.downloads)
    );
    assert!(!f.downloads.exists());
    // Folder courses' files are never touched.
    assert!(
        f.temp
            .path()
            .join("Courses/DEMO101 Intro/notes.txt")
            .exists()
    );
    // Only a purged course can be forgotten.
    f.app.forget_removed_course(&removed.removed_id).unwrap();
    assert!(f.app.removed_courses().unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_trash_keeps_the_files_until_the_student_decides() {
    let f = fixture().await;
    f.trash.fail.store(true, Ordering::SeqCst);
    let report = f
        .app
        .remove_courses(
            vec!["DEMO303".into()],
            RemoveOptions {
                purge_now: true,
                ..options()
            },
        )
        .await
        .unwrap();
    let id = report.removed[0].removed_id.clone();
    assert!(report.removed[0].files_pending);
    assert!(
        f.downloads.join("slides.txt").exists(),
        "never deleted without asking"
    );
    // Tried again at a sync's start (still failing): kept.
    f.app
        .sync_source(&f.folder_source, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert!(f.app.removed_courses().unwrap()[0].files_pending);
    let again = f
        .app
        .purge_removed_courses(Some(vec![id.clone()]), false)
        .await
        .unwrap();
    assert_eq!(again.files_pending, std::slice::from_ref(&id));
    // "Delete permanently".
    let done = f
        .app
        .purge_removed_courses(Some(vec![id]), true)
        .await
        .unwrap();
    assert!(done.files_pending.is_empty());
    assert!(!f.downloads.exists());
    assert!(!f.app.removed_courses().unwrap()[0].files_pending);
}

#[tokio::test]
async fn a_due_purge_runs_when_a_sync_starts_and_forgetting_needs_a_purge() {
    let f = fixture().await;
    let report = f
        .app
        .remove_courses(vec!["DEMO202".into()], options())
        .await
        .unwrap();
    let id = report.removed[0].removed_id.clone();
    assert_eq!(
        f.app.forget_removed_course(&id).unwrap_err().kind,
        AppErrorKind::Invalid
    );
    // Seven days later.
    Store::open(&f.app.db_path())
        .unwrap()
        .conn()
        .execute(
            "UPDATE course_tombstones SET purge_after = ?1",
            [(Utc::now() - Duration::hours(1)).to_rfc3339()],
        )
        .unwrap();
    f.app
        .sync_source(&f.folder_source, SyncRequest::default(), |_| {})
        .await
        .unwrap();
    let removed = &f.app.removed_courses().unwrap()[0];
    assert_eq!(removed.state, TombstoneState::Purged);
    // The folder is still there, the course isn't read again.
    assert!(
        f.temp
            .path()
            .join("Courses/DEMO202 Methods/notes.txt")
            .exists()
    );
    assert!(!codes(&f.app).contains(&"DEMO202".to_string()));
    // Restoring a purged folder course reads it again.
    let outcome = f.app.restore_course(&id).await.unwrap();
    assert!(outcome.restored, "{outcome:?}");
    assert!(codes(&f.app).contains(&"DEMO202".to_string()));
    assert!(f.app.removed_courses().unwrap().is_empty());
}
