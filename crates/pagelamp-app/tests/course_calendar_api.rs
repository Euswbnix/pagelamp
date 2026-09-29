//! The course calendar facade before schema v4 (docs/design/v0.3-course-calendar.md §4, §7):
//! the view with its candidates, why AI reading can't run, the reading offers, and the AI
//! and storage methods refused as this build does. All data is synthetic.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{Local, TimeDelta};
use pagelamp_app::{App, AppErrorKind, CalendarBatchEvent, ReadCalendarOptions};
use pagelamp_core::ai::BlockReason;
use pagelamp_core::calendar::candidates::{CandidateLeftOut, CandidateReason};
use pagelamp_core::model::*;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::Store;
use pagelamp_core::term::CalendarStatus;
use serde_json::json;

const SOURCE: &str = "canvas:lms.example.edu";

fn course_id(external: &str) -> String {
    format!("{SOURCE}/course/{external}")
}

fn add_material(
    store: &Store,
    course: &str,
    id: &str,
    kind: MaterialKind,
    title: &str,
    text: &str,
) {
    let material = format!("{SOURCE}/{id}");
    store
        .upsert_material(&MaterialUpsert {
            id: material.clone(),
            course_id: course_id(course),
            module_id: None,
            kind,
            title: title.into(),
            url: None,
            local_path: None,
            mime: None,
            published_at: None,
            week_hint: None,
        })
        .unwrap();
    if text.is_empty() {
        return;
    }
    store
        .set_text_state(&material, TextStatus::Ok, None, Some("hash"))
        .unwrap();
    store
        .replace_chunks(
            &material,
            &[Chunk {
                material_id: material.clone(),
                ord: 0,
                locator: Some("p. 1".into()),
                text: text.into(),
            }],
        )
        .unwrap();
}

/// DEMO101 (a syllabus and an outline), DEMO202 (slides only), DEMO303 (hidden), DEMO404
/// (prohibited) and DEMO505 (AI access off); the last three have an outline too.
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
    let today = Local::now().date_naive();
    for external in ["101", "202", "303", "404", "505"] {
        store
            .upsert_course(&CourseUpsert {
                id: course_id(external),
                source_id: SOURCE.into(),
                external_id: external.into(),
                code: Some(format!("DEMO{external}")),
                name: format!("Demo course {external}"),
                // Teaching now, so every course is current.
                term_start: Some(today - TimeDelta::days(14)),
                term_end: Some(today + TimeDelta::days(90)),
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
    }
    let syllabus = "Classes begin Tuesday, September 8. Reading week: October 26-30. ".repeat(5);
    add_material(
        &store,
        "101",
        "syllabus/101",
        MaterialKind::Syllabus,
        "Syllabus",
        &syllabus,
    );
    add_material(
        &store,
        "101",
        "file/outline",
        MaterialKind::File,
        "DEMO101 Course Outline.pdf",
        "Week 1 (Sept 8) Basics",
    );
    add_material(
        &store,
        "101",
        "file/schedule",
        MaterialKind::File,
        "Lecture schedule",
        "",
    );
    add_material(
        &store,
        "202",
        "file/slides",
        MaterialKind::File,
        "Week 3 slides",
        "Stomata open in light.",
    );
    for external in ["303", "404", "505"] {
        add_material(
            &store,
            external,
            &format!("file/outline{external}"),
            MaterialKind::File,
            "Course outline",
            "Week 1 (Sept 8)",
        );
    }
    drop(store);
    app.set_course_hidden("DEMO303", true).unwrap();
    app.set_course_policy("DEMO404", AiPolicy::Prohibited, None)
        .unwrap();
    app.set_course_ai_access("DEMO505", false).unwrap();
    app
}

#[test]
fn the_view_lists_candidates_and_why_ai_reading_cannot_run() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let view = app.course_calendar("DEMO101").unwrap();
    assert_eq!(view.course_id, course_id("101"));
    assert_eq!(view.status, CalendarStatus::NoCalendar);
    assert!(view.accepted.is_none() && view.proposals.is_empty());
    let candidates: Vec<(&str, CandidateReason, bool, Option<CandidateLeftOut>)> = view
        .candidates
        .iter()
        .map(|c| (c.title.as_str(), c.reason, c.included, c.left_out))
        .collect();
    assert_eq!(
        candidates,
        [
            ("Syllabus", CandidateReason::Syllabus, true, None),
            (
                "DEMO101 Course Outline.pdf",
                CandidateReason::TitleOutline,
                true,
                None
            ),
            (
                "Lecture schedule",
                CandidateReason::TitleSchedule,
                false,
                Some(CandidateLeftOut::NoText)
            ),
        ]
    );
    assert_eq!(app.calendar_candidates("DEMO101").unwrap(), view.candidates);
    // A readable course: this build has no AI reading.
    assert_eq!(view.blocked, Some(BlockReason::BackendDisabledInThisBuild));
    // The course's own reason comes first; hidden courses are addressable.
    for (course, reason) in [
        ("DEMO303", BlockReason::CourseHidden),
        ("DEMO404", BlockReason::CoursePolicyProhibited),
        ("DEMO505", BlockReason::CourseAiTurnedOff),
    ] {
        assert_eq!(
            app.course_calendar(course).unwrap().blocked,
            Some(reason),
            "{course}"
        );
    }
    // The wire names the frontend built against.
    let wire = serde_json::to_value(&view).unwrap();
    assert_eq!(wire["status"], "none");
    assert_eq!(wire["blocked"], "backend_disabled_in_this_build");
    assert_eq!(wire["candidates"][2]["left_out"], "no_text");
    assert_eq!(wire["candidates"][0]["reason"], "syllabus");
}

#[test]
fn reading_offers_skip_hidden_withheld_and_empty_courses() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let offers = app.syllabus_reading_offers().unwrap();
    assert_eq!(offers.len(), 1, "{offers:?}");
    let offer = &offers[0];
    assert_eq!(offer.course_id, course_id("101"));
    assert_eq!(offer.reason_code, "no_calendar");
    assert_eq!((offer.candidates, offer.has_text), (3, true));
}

#[tokio::test]
async fn ai_reading_is_refused_per_course_with_its_reason() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let err = app
        .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind, err.blocked),
        (
            AppErrorKind::Blocked,
            Some(BlockReason::BackendDisabledInThisBuild)
        )
    );
    let err = app
        .read_course_calendar("DEMO404", "gen-2", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::CoursePolicyProhibited));
    let err = app
        .read_course_calendar("DEMO999", "gen-3", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::NotFound);

    // A batch reports each course, one after another.
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let outcomes = app
        .read_course_calendars(
            vec!["DEMO101".into(), "DEMO505".into()],
            "batch-1",
            ReadCalendarOptions::default(),
            move |event| sink.lock().unwrap().push(event),
        )
        .await
        .unwrap();
    let blocked: Vec<Option<BlockReason>> = outcomes.iter().map(|o| o.blocked).collect();
    assert_eq!(
        blocked,
        [
            Some(BlockReason::BackendDisabledInThisBuild),
            Some(BlockReason::CourseAiTurnedOff)
        ]
    );
    assert!(
        outcomes
            .iter()
            .all(|o| o.proposal_id.is_none() && o.error.is_none())
    );
    let events = events.lock().unwrap();
    let wire: Vec<serde_json::Value> = events
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let kinds: Vec<&str> = wire.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "course_started",
            "course_finished",
            "course_started",
            "course_finished"
        ]
    );
    assert_eq!(
        (wire[2]["index"].as_u64(), wire[2]["total"].as_u64()),
        (Some(1), Some(2))
    );
    assert!(matches!(
        &events[3],
        CalendarBatchEvent::CourseFinished { outcome } if outcome.course_id == course_id("505")
    ));
    // Nothing runs, so there is nothing to stop.
    app.cancel_generation("batch-1").unwrap();
}

#[tokio::test]
async fn stored_calendars_arrive_with_schema_v4() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    // No proposal exists in this build.
    assert_eq!(
        app.accept_calendar_proposal(7, None).unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert_eq!(
        app.dismiss_calendar_proposal(7).unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert!(app.accept_passing_proposals(Vec::new()).unwrap().is_empty());
    // The rest says it isn't available yet.
    let not_yet = [
        app.scan_course_calendar("DEMO101").map(|_| ()).unwrap_err(),
        app.set_calendar_sources("DEMO101", vec![], vec![])
            .map(|_| ())
            .unwrap_err(),
        app.set_course_dates("DEMO101", None)
            .map(|_| ())
            .unwrap_err(),
        app.download_material_files("DEMO101", vec![], |_| {})
            .await
            .map(|_| ())
            .unwrap_err(),
    ];
    for err in not_yet {
        assert_eq!(err.kind, AppErrorKind::Internal, "{}", err.message);
        assert!(
            err.message.contains("not available in this build yet"),
            "{}",
            err.message
        );
    }
}
