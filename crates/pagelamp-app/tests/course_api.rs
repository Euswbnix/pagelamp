//! The course-lane facade: timeline, lifecycle, "I'm still taking this", removal snoozes and
//! confirmed dates (docs/design/v0.3-course-calendar.md §4). All data is synthetic.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use chrono::{Local, NaiveDate, TimeDelta};
use pagelamp_app::{App, AppErrorKind, KEEP_CURRENT_DAYS, NOT_NOW_DAYS, keep_forever};
use pagelamp_core::model::*;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::Store;
use pagelamp_core::term::CONFIRMED_DATES_KEY;
use serde_json::json;

const SOURCE: &str = "canvas:lms.example.edu";

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn course_id(external: &str) -> String {
    format!("{SOURCE}/course/{external}")
}

/// DEMO101 (visible) and DEMO303 (hidden) on a Canvas-like source, seeded directly.
fn app_with_courses(dir: &Path) -> App {
    let app = App::open_at_with_secrets(dir.join("data"), Arc::new(MemorySecrets::new())).unwrap();
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_source(&SourceRecord {
            id: SOURCE.into(),
            kind: SourceKind::Canvas,
            label: "lms.example.edu".into(),
            config: json!({ "base_url": "https://lms.example.edu" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    for (external, code) in [("101", "DEMO101"), ("303", "DEMO303")] {
        store
            .upsert_course(&CourseUpsert {
                id: course_id(external),
                source_id: SOURCE.into(),
                external_id: external.into(),
                code: Some(code.into()),
                name: format!("Demo course {external}"),
                term_start: None,
                term_end: None,
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
    }
    store.set_course_hidden(&course_id("303"), true).unwrap();
    app
}

fn term_data(app: &App, id: &str) -> CourseTermData {
    Store::open(&app.db_path())
        .unwrap()
        .course_term_data(id)
        .unwrap()
        .unwrap()
}

fn confirmed(app: &App) -> BTreeSet<String> {
    Store::open(&app.db_path())
        .unwrap()
        .setting::<BTreeSet<String>>(CONFIRMED_DATES_KEY)
        .unwrap()
        .unwrap_or_default()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[test]
fn keep_course_current_has_a_facade_default_and_can_be_undone() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let id = course_id("101");

    // No dates at all: today + 120 days.
    let course = app.keep_course_current("demo101", None).unwrap();
    assert_eq!(course.id, id);
    assert_eq!(
        term_data(&app, &id).keep_current_until,
        Some(today() + TimeDelta::days(KEEP_CURRENT_DAYS))
    );
    // An explicit date wins; hidden courses are addressable by code.
    app.keep_course_current("DEMO303", Some(date("2027-01-15")))
        .unwrap();
    assert_eq!(
        term_data(&app, &course_id("303")).keep_current_until,
        Some(date("2027-01-15"))
    );
    app.clear_keep_course_current("DEMO101").unwrap();
    assert_eq!(term_data(&app, &id).keep_current_until, None);

    let err = app.keep_course_current("NOPE999", None).unwrap_err();
    assert_eq!(err.kind, AppErrorKind::NotFound);
}

#[test]
fn removal_snoozes_are_computed_by_the_facade() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    app.snooze_removal_suggestions(vec!["DEMO101".into(), "DEMO303".into()], SnoozeKind::NotNow)
        .unwrap();
    let not_now = Some(today() + TimeDelta::days(NOT_NOW_DAYS));
    assert_eq!(
        term_data(&app, &course_id("101")).removal_snoozed_until,
        not_now
    );
    assert_eq!(
        term_data(&app, &course_id("303")).removal_snoozed_until,
        not_now
    );

    app.snooze_removal_suggestions(vec!["DEMO101".into()], SnoozeKind::Keep)
        .unwrap();
    assert_eq!(keep_forever(), date("9999-12-31"));
    assert_eq!(
        term_data(&app, &course_id("101")).removal_snoozed_until,
        Some(keep_forever())
    );

    app.clear_removal_snooze(vec!["DEMO101".into()]).unwrap();
    assert_eq!(
        term_data(&app, &course_id("101")).removal_snoozed_until,
        None
    );

    // One unknown course: nothing changes (one transaction).
    let err = app
        .snooze_removal_suggestions(vec!["DEMO303".into(), "NOPE".into()], SnoozeKind::Keep)
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::NotFound);
    assert_eq!(
        term_data(&app, &course_id("303")).removal_snoozed_until,
        not_now
    );
}

#[test]
fn saving_or_confirming_student_dates_marks_them_confirmed() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let id = course_id("101");
    assert!(confirmed(&app).is_empty());

    app.set_course_term("DEMO101", Some(date("2026-09-08")), None)
        .unwrap();
    assert!(confirmed(&app).contains(&id));
    // Clearing the dates takes the course off the list again.
    app.set_course_term("DEMO101", None, None).unwrap();
    assert!(!confirmed(&app).contains(&id));

    let timeline = app.confirm_course_dates("DEMO303").unwrap();
    assert!(timeline.term.student_start.is_none());
    assert_eq!(confirmed(&app), BTreeSet::from([course_id("303")]));
    // Confirming twice changes nothing.
    app.confirm_course_dates("DEMO303").unwrap();
    assert_eq!(confirmed(&app).len(), 1);
}

#[test]
fn course_timeline_and_lifecycle_summary_cover_hidden_courses() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let timeline = app.course_timeline("DEMO303").unwrap();
    assert_eq!(timeline.as_of, today());

    // No dates and no activity at all: both courses are Inactive (design §8.1 rule 7) and
    // suggested for removal, the hidden one too.
    let summary = app.lifecycle_summary().unwrap();
    let ids: Vec<&str> = summary
        .courses
        .iter()
        .map(|entry| entry.course_id.as_str())
        .collect();
    assert_eq!(ids, [course_id("101"), course_id("303")]);
    assert!(summary.courses[1].hidden);
    assert!(
        summary
            .courses
            .iter()
            .all(|entry| entry.lifecycle.state == LifecycleState::Inactive)
    );
    assert_eq!(summary.suggested, [course_id("101"), course_id("303")]);
    assert!(summary.show_banner);
    assert_eq!(summary.banner_snoozed_until, None);

    // "Not now" on the banner hides it for this set of courses…
    app.snooze_lifecycle_banner().unwrap();
    let summary = app.lifecycle_summary().unwrap();
    assert!(!summary.show_banner);
    assert_eq!(
        summary.banner_snoozed_until,
        Some(today() + TimeDelta::days(NOT_NOW_DAYS))
    );
    // …"Keep" takes a course off the suggestions…
    app.snooze_removal_suggestions(vec!["DEMO101".into()], SnoozeKind::Keep)
        .unwrap();
    let summary = app.lifecycle_summary().unwrap();
    assert_eq!(summary.suggested, [course_id("303")]);
    assert!(!summary.show_banner);
    // …and a newly suggested course shows the banner again.
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: course_id("404"),
            source_id: SOURCE.into(),
            external_id: "404".into(),
            code: Some("DEMO404".into()),
            name: "Demo course 404".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    let summary = app.lifecycle_summary().unwrap();
    assert!(summary.show_banner);
}
