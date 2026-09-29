//! From a course's materials to an unsaved proposal (docs/design/v0.3-course-calendar.md
//! §7.2–§7.6): the shared reading inputs, the deterministic scan, a model-style extraction
//! keyed by context handles, and the proposal the facade will store. All data is synthetic.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use pagelamp_core::calendar::assemble::DateKind;
use pagelamp_core::calendar::candidates::CandidateSignals;
use pagelamp_core::calendar::extraction::{
    CalendarExtraction, ClaimKind, ExtractedClaim, StatedTerm,
};
use pagelamp_core::calendar::proposal::NewProposal;
use pagelamp_core::calendar::reading::reading_inputs;
use pagelamp_core::calendar::validate::DropReason;
use pagelamp_core::model::*;
use pagelamp_core::store::Store;
use pagelamp_core::views::AsOf;
use serde_json::json;

const SOURCE: &str = "canvas:lms.example.edu";
const COURSE: &str = "canvas:lms.example.edu/course/101";
const SYLLABUS: &str = "canvas:lms.example.edu/syllabus/101";
const OUTLINE: &str = "canvas:lms.example.edu/file/outline";

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&format!("{text}T12:00:00Z"))
        .unwrap()
        .with_timezone(&Utc)
}

/// Monday of teaching week 3.
fn as_of() -> AsOf {
    AsOf {
        now: at("2026-09-21"),
        today: date("2026-09-21"),
        tz: None,
    }
}

const SYLLABUS_TEXT: &str = "DEMO101 Intro to Demo Studies, Fall 2026.\n\
Classes begin Tuesday, September 8, 2026.\n\
Reading week: October 26-30, 2026 (no classes).\n\
Last day of classes: Tuesday, December 8, 2026.\n\
Final exam period: December 10-21, 2026.\n\
Office hours are posted on the course page. Late work loses 10% per day.";

fn add_material(store: &Store, id: &str, kind: MaterialKind, title: &str, text: &str, hash: &str) {
    store
        .upsert_material(&MaterialUpsert {
            id: id.into(),
            course_id: COURSE.into(),
            module_id: None,
            kind,
            title: title.into(),
            url: Some(format!("https://lms.example.edu/{title}")),
            local_path: None,
            mime: None,
            published_at: Some(at("2026-09-01")),
            week_hint: None,
        })
        .unwrap();
    store
        .set_text_state(id, TextStatus::Ok, None, Some(hash))
        .unwrap();
    store
        .replace_chunks(
            id,
            &[Chunk {
                material_id: id.into(),
                ord: 0,
                locator: Some("p. 1".into()),
                text: text.into(),
            }],
        )
        .unwrap();
}

fn demo_store() -> (Store, Course) {
    let store = Store::open_in_memory().unwrap();
    store
        .upsert_source(&SourceRecord {
            id: SOURCE.into(),
            kind: SourceKind::Canvas,
            label: "Demo LMS".into(),
            config: json!({}),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: COURSE.into(),
            source_id: SOURCE.into(),
            external_id: "101".into(),
            code: Some("DEMO101".into()),
            name: "Intro to Demo Studies".into(),
            term_start: Some(date("2026-09-07")),
            term_end: Some(date("2026-12-22")),
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    add_material(
        &store,
        SYLLABUS,
        MaterialKind::Syllabus,
        "Syllabus",
        SYLLABUS_TEXT,
        "h1",
    );
    add_material(
        &store,
        OUTLINE,
        MaterialKind::File,
        "Course outline.pdf",
        "Week 1 (Sept 8) Basics of demo studies",
        "h2",
    );
    let course = store.get_course(COURSE).unwrap().unwrap();
    (store, course)
}

#[test]
fn the_scan_reads_the_candidates_into_a_passing_proposal() {
    let (store, course) = demo_store();
    let inputs = reading_inputs(&store, &course, as_of(), &CandidateSignals::default()).unwrap();
    let mut read: Vec<&str> = inputs.sources.keys().map(String::as_str).collect();
    read.sort();
    assert_eq!(read, [OUTLINE, SYLLABUS]);
    assert_eq!(inputs.manifest.len(), 2);
    assert_eq!(inputs.validation.today, Some(date("2026-09-21")));

    let assembled = inputs.scan(None).expect("the syllabus states its dates");
    let segment = &assembled.calendar.segments[0];
    assert_eq!(segment.first_class, date("2026-09-08"));
    assert_eq!(segment.last_class, Some(date("2026-12-08")));
    let exam = assembled.calendar.exam_period.expect("an exam period");
    assert_eq!(
        (exam.start, exam.end),
        (date("2026-12-10"), date("2026-12-21"))
    );
    let reading_week = &assembled.calendar.breaks[0];
    assert_eq!(
        (reading_week.span.start, reading_week.span.end),
        (date("2026-10-26"), date("2026-10-30"))
    );
    assert!(assembled.passing, "{:?}", assembled.conflicts);
    assert_eq!(assembled.resulting_week_today, Some(3));
    // Every date carries the syllabus's own words.
    let first = assembled
        .dates
        .iter()
        .find(|d| d.kind == DateKind::FirstClass)
        .unwrap();
    assert_eq!(first.evidence[0].material_id, SYLLABUS);
    assert!(
        first.evidence[0]
            .quote
            .as_deref()
            .unwrap()
            .contains("Classes begin Tuesday, September 8")
    );

    // The unsaved proposal becomes the facade's once stored.
    let fingerprint = inputs.fingerprint();
    let proposal = NewProposal {
        course_id: inputs.course_id.clone(),
        origin: CalendarOrigin::Scan,
        assembled: assembled.clone(),
        ai_label: None,
        sharing_reminder: false,
        manifest: inputs.manifest.clone(),
        fingerprint: fingerprint.clone(),
    }
    .into_proposal(7, at("2026-09-21"));
    assert_eq!((proposal.id, proposal.course_id.as_str()), (7, COURSE));
    assert_eq!(proposal.origin, CalendarOrigin::Scan);
    assert_eq!(proposal.calendar, assembled.calendar);
    assert_eq!(proposal.passing, assembled.passing);
}

#[test]
fn the_fingerprint_follows_the_candidates_text() {
    let (store, course) = demo_store();
    let before = reading_inputs(&store, &course, as_of(), &CandidateSignals::default())
        .unwrap()
        .fingerprint();
    let again = reading_inputs(&store, &course, as_of(), &CandidateSignals::default())
        .unwrap()
        .fingerprint();
    assert_eq!(before, again);
    // The outline changes: a new fingerprint (and so a new proposal is allowed).
    store
        .set_text_state(OUTLINE, TextStatus::Ok, None, Some("h3"))
        .unwrap();
    let after = reading_inputs(&store, &course, as_of(), &CandidateSignals::default())
        .unwrap()
        .fingerprint();
    assert_ne!(before, after);
}

fn claim(
    kind: ClaimKind,
    date: &str,
    end: Option<&str>,
    quote: &str,
    source: &str,
) -> ExtractedClaim {
    ExtractedClaim {
        kind,
        date: date.into(),
        end_date: end.map(Into::into),
        label: String::new(),
        quote: quote.into(),
        source: source.into(),
    }
}

#[test]
fn a_model_extraction_is_checked_against_the_text_behind_its_handles() {
    let (store, course) = demo_store();
    let inputs = reading_inputs(&store, &course, as_of(), &CandidateSignals::default()).unwrap();
    // The context builder gives the model handles ("c1") instead of material ids.
    let by_handle: HashMap<String, _> =
        [("c1".to_string(), inputs.sources[SYLLABUS].clone())].into();
    let extraction = CalendarExtraction {
        stated_term: StatedTerm {
            text: Some("Fall 2026".into()),
            quote: Some("DEMO101 Intro to Demo Studies, Fall 2026.".into()),
            source: Some("c1".into()),
        },
        claims: vec![
            claim(
                ClaimKind::FirstClass,
                "2026-09-08",
                None,
                "Classes begin Tuesday, September 8, 2026.",
                "c1",
            ),
            claim(
                ClaimKind::LastClass,
                "2026-12-08",
                None,
                "Last day of classes: Tuesday, December 8, 2026.",
                "c1",
            ),
            claim(
                ClaimKind::ExamPeriod,
                "2026-12-10",
                Some("2026-12-21"),
                "Final exam period: December 10-21, 2026.",
                "c1",
            ),
            // Invented: no such words in the syllabus.
            claim(
                ClaimKind::Break,
                "2026-11-09",
                Some("2026-11-13"),
                "Fall break: November 9-13, 2026.",
                "c1",
            ),
            // A handle the context never gave.
            claim(
                ClaimKind::FinalExam,
                "2026-12-15",
                None,
                "Final exam: December 15, 2026.",
                "c9",
            ),
        ],
        weeks: vec![],
        not_found: vec![],
    };
    let assembled = inputs.propose(&extraction, &by_handle, None).unwrap();
    assert_eq!(
        assembled.calendar.segments[0].first_class,
        date("2026-09-08")
    );
    assert!(
        assembled.calendar.breaks.is_empty(),
        "the invented break is dropped"
    );
    let dropped: HashMap<DropReason, u32> = assembled
        .dropped
        .iter()
        .map(|d| (d.reason, d.count))
        .collect();
    assert_eq!(dropped.get(&DropReason::UnsupportedQuote), Some(&1));
    assert_eq!(dropped.get(&DropReason::UnknownSource), Some(&1));

    // Nothing that gives a first class: the run as a whole is bad output (V10).
    let empty = CalendarExtraction {
        stated_term: StatedTerm::default(),
        claims: vec![claim(
            ClaimKind::LastClass,
            "2026-12-08",
            None,
            "Last day of classes: Tuesday, December 8, 2026.",
            "c1",
        )],
        weeks: vec![],
        not_found: vec![],
    };
    assert!(inputs.propose(&empty, &by_handle, None).is_err());
}
