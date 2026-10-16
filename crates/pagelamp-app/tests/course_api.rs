//! The course-lane facade: timeline, lifecycle, "I'm still taking this", removal snoozes and
//! confirmed dates (docs/design/v0.3-course-calendar.md §4). All data is synthetic.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use chrono::{Local, NaiveDate, TimeDelta};
use pagelamp_app::{
    App, AppErrorKind, ApplySkipReason, KEEP_CURRENT_DAYS, NOT_NOW_DAYS, ReplacedDates,
    SkippedCourse, keep_forever,
};
use pagelamp_core::model::*;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::Store;
use pagelamp_core::term::{CONFIRMED_DATES_KEY, COPIED_DATES_KEY};
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
    // A day in the past is refused (review 4).
    let err = app
        .keep_course_current("DEMO101", Some(today() - TimeDelta::days(1)))
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Invalid);
    assert_eq!(term_data(&app, &id).keep_current_until, None);
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
fn a_failed_read_never_drops_the_confirmed_dates() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    app.confirm_course_dates("DEMO303").unwrap();
    let raw = rusqlite::Connection::open(app.db_path()).unwrap();

    // The list can't be read while writes still work (here it is stored as a blob): saving
    // or confirming DEMO101's dates fails and writes nothing. It never replaces the list with
    // DEMO101 alone.
    raw.execute(
        "UPDATE settings SET value = CAST(value AS BLOB) WHERE key = ?1",
        [CONFIRMED_DATES_KEY],
    )
    .unwrap();
    assert!(app.confirm_course_dates("DEMO101").is_err());
    assert!(
        app.set_course_term("DEMO101", Some(date("2026-09-08")), None)
            .is_err()
    );
    assert!(app.course_timeline("DEMO303").is_err());
    raw.execute(
        "UPDATE settings SET value = CAST(value AS TEXT) WHERE key = ?1",
        [CONFIRMED_DATES_KEY],
    )
    .unwrap();
    assert_eq!(confirmed(&app), BTreeSet::from([course_id("303")]));
    // The dates weren't saved either (one transaction).
    assert_eq!(term_data(&app, &course_id("101")).user_term_start, None);

    // The settings table can't be read at all.
    raw.execute_batch("ALTER TABLE settings RENAME TO settings_away")
        .unwrap();
    assert!(app.confirm_course_dates("DEMO101").is_err());
    assert!(app.course_timeline("DEMO303").is_err());
    assert!(app.list_courses().is_err());
    raw.execute_batch("ALTER TABLE settings_away RENAME TO settings")
        .unwrap();
    assert_eq!(confirmed(&app), BTreeSet::from([course_id("303")]));

    // Another version's shape counts as an empty list: confirming starts a new one.
    raw.execute(
        "UPDATE settings SET value = '{\"ids\": 3}' WHERE key = ?1",
        [CONFIRMED_DATES_KEY],
    )
    .unwrap();
    app.course_timeline("DEMO303").unwrap();
    app.confirm_course_dates("DEMO101").unwrap();
    assert_eq!(confirmed(&app), BTreeSet::from([course_id("101")]));
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

// ----- the courses of one session ------------------------------------------------------------

/// The host whose course codes carry a session code (the session hint is host-gated).
const SESSION_HOST: &str = "canvas:q.utoronto.ca";

fn session_id(external: &str) -> String {
    format!("{SESSION_HOST}/course/{external}")
}

/// Three courses of a Fall 2028 session on one campus (the third with dates from PageLamp 0.1
/// the student never confirmed) and one on another campus.
fn app_with_session(dir: &Path) -> App {
    let app = App::open_at_with_secrets(dir.join("data"), Arc::new(MemorySecrets::new())).unwrap();
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_source(&SourceRecord {
            id: SESSION_HOST.into(),
            kind: SourceKind::Canvas,
            label: "Session LMS".into(),
            config: json!({}),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    for (external, code) in [
        ("1", "DEM240H5 F LEC0101 20289"),
        ("2", "DEM332H5 F LEC0102 20289"),
        ("3", "DEM205H5 LEC0101 20289"),
        ("4", "DEM101H1 F LEC0101 20289"),
    ] {
        store
            .upsert_course(&CourseUpsert {
                id: session_id(external),
                source_id: SESSION_HOST.into(),
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
    store
        .set_course_term(&session_id("3"), Some(date("2028-09-12")), None)
        .unwrap();
    app
}

#[test]
fn the_dates_of_one_course_apply_to_the_others_of_its_session_and_undo_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_session(temp.path());
    app.set_course_term(
        "DEM240H5 F LEC0101 20289",
        Some(date("2028-09-05")),
        Some(date("2028-12-05")),
    )
    .unwrap();

    // The other courses of the session: one borrows its week 1 already, one has its own dates.
    let listed = app.session_courses(&session_id("1")).unwrap();
    assert_eq!(listed.session.as_deref(), Some("20289"));
    let rows: Vec<(&str, bool, Option<NaiveDate>, TermAnchorSource)> = listed
        .courses
        .iter()
        .map(|c| (c.course_id.as_str(), c.own_dates, c.student_start, c.anchor))
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                session_id("3").as_str(),
                true,
                Some(date("2028-09-12")),
                TermAnchorSource::StudentConfirmed
            ),
            (
                session_id("2").as_str(),
                false,
                None,
                TermAnchorSource::SameSession
            ),
        ]
    );
    assert_eq!(listed.courses[1].week_one_monday, Some(date("2028-09-04")));
    // A course on another campus has no other course in its session.
    let alone = app.session_courses(&session_id("4")).unwrap();
    assert_eq!(alone.session.as_deref(), Some("20289"));
    assert!(alone.courses.is_empty());

    let report = app
        .apply_course_dates_to(
            &session_id("1"),
            vec![
                session_id("2"),
                "DEM205H5 LEC0101 20289".into(),
                session_id("4"),
                "NOPE999".into(),
            ],
        )
        .unwrap();
    assert_eq!(report.applied, vec![session_id("2"), session_id("3")]);
    assert_eq!(
        report.skipped,
        vec![
            SkippedCourse {
                course_id: session_id("4"),
                reason: ApplySkipReason::NotSameSession
            },
            SkippedCourse {
                course_id: "NOPE999".into(),
                reason: ApplySkipReason::UnknownCourse
            },
        ]
    );
    assert_eq!(
        report.replaced,
        vec![
            ReplacedDates {
                course_id: session_id("2"),
                start: None,
                end: None,
                legacy: false
            },
            ReplacedDates {
                course_id: session_id("3"),
                start: Some(date("2028-09-12")),
                end: None,
                legacy: true
            },
        ]
    );
    // Both now have the dates as their own, confirmed, and say where they came from.
    for id in [session_id("2"), session_id("3")] {
        let data = term_data(&app, &id);
        assert_eq!(
            (data.user_term_start, data.user_term_end),
            (Some(date("2028-09-05")), Some(date("2028-12-05")))
        );
        assert!(confirmed(&app).contains(&id));
        let timeline = app.course_timeline(&id).unwrap();
        assert_eq!(timeline.term.anchor, TermAnchorSource::StudentConfirmed);
        assert!(
            timeline
                .evidence_items
                .iter()
                .any(|item| item.code == "dates_copied_from"
                    && item.param("course") == Some("DEM240H5 F LEC0101 20289")),
            "{id}"
        );
    }

    // Undo puts back exactly what was there, 0.1 dates unconfirmed again.
    app.restore_course_dates(report.replaced).unwrap();
    let two = term_data(&app, &session_id("2"));
    assert_eq!((two.user_term_start, two.user_term_end), (None, None));
    let three = term_data(&app, &session_id("3"));
    assert_eq!(three.user_term_start, Some(date("2028-09-12")));
    assert_eq!(
        confirmed(&app),
        BTreeSet::from([session_id("1")]),
        "only the course the student saved"
    );
    let copied: BTreeMap<String, String> = Store::open(&app.db_path())
        .unwrap()
        .setting(COPIED_DATES_KEY)
        .unwrap()
        .unwrap_or_default();
    assert!(copied.is_empty());

    // A course without dates of the student's has nothing to copy.
    let report = app
        .apply_course_dates_to(&session_id("2"), vec![session_id("3")])
        .unwrap();
    assert!(report.applied.is_empty());
    assert_eq!(report.skipped[0].reason, ApplySkipReason::NoDates);
}

#[test]
fn saving_a_course_s_own_dates_ends_their_copied_from_line() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_session(temp.path());
    app.set_course_term(&session_id("1"), Some(date("2028-09-05")), None)
        .unwrap();
    app.apply_course_dates_to(&session_id("1"), vec![session_id("2")])
        .unwrap();
    let copied_line = |app: &App| {
        app.course_timeline(&session_id("2"))
            .unwrap()
            .evidence_items
            .iter()
            .any(|item| item.code == "dates_copied_from")
    };
    assert!(copied_line(&app));
    app.set_course_term(&session_id("2"), Some(date("2028-09-06")), None)
        .unwrap();
    assert!(!copied_line(&app));
}
