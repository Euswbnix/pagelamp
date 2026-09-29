//! Course weeks, phases and lifecycle: the tests of docs/design/v0.3-course-calendar.md §9.6
//! (CAL-n, by their design names). All dates and courses are synthetic.

use chrono::{DateTime, NaiveDate, Utc};
use pagelamp_core::model::*;
use pagelamp_core::term::{TermInput, resolve_term};
use pagelamp_core::timeline::{self, course_timeline};

const UOFT: &str = "canvas:q.utoronto.ca";
const TORONTO: &str = "America/Toronto";

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text).unwrap().to_utc()
}

/// A course record as the store returns it.
fn course(source: &str, code: &str, name: &str) -> Course {
    Course {
        id: format!("{source}/course/1"),
        source_id: source.into(),
        external_id: "1".into(),
        code: Some(code.into()),
        name: name.into(),
        term_start: None,
        term_end: None,
        term_source: TermSource::None,
        url: None,
        ai_policy: AiPolicy::Unknown,
        ai_policy_note: None,
        ai_access: true,
        hidden: false,
        enrollment_active: true,
        updated_at: at("2026-09-01T12:00:00Z"),
    }
}

/// The UofT-style enrollment-window term of the owner's report (synthetic course).
fn uoft_window_term() -> CourseTermData {
    CourseTermData {
        lms: LmsCourseInfo {
            term_name: Some("Fall 2026".into()),
            term_start: Some(date("2026-05-04")),
            term_end: Some(date("2027-01-31")),
            time_zone: Some(TORONTO.into()),
            access_restricted: Some(false),
            ..LmsCourseInfo::default()
        },
        synced_term_start: Some(date("2026-05-04")),
        synced_term_end: Some(date("2027-01-31")),
        ..CourseTermData::default()
    }
}

fn material(id: &str, title: &str, published: &str) -> Material {
    Material {
        id: id.into(),
        course_id: format!("{UOFT}/course/1"),
        module_id: None,
        kind: MaterialKind::File,
        title: title.into(),
        url: None,
        local_path: None,
        mime: None,
        published_at: Some(at(published)),
        week_hint: timeline::parse_week_hint(title),
        content_hash: None,
        text_status: TextStatus::Pending,
        text_error: None,
        download_blocked: None,
        updated_at: at(published),
    }
}

struct Case {
    course: Course,
    data: CourseTermData,
    materials: Vec<Material>,
    modules: Vec<Module>,
    events: Vec<Event>,
    confirmed: bool,
}

impl Case {
    fn new(course: Course, data: CourseTermData) -> Self {
        Case {
            course,
            data,
            materials: Vec::new(),
            modules: Vec::new(),
            events: Vec::new(),
            confirmed: false,
        }
    }

    fn input(&self, today: &str) -> TermInput<'_> {
        TermInput {
            course: &self.course,
            data: &self.data,
            dates_confirmed: self.confirmed,
            modules: &self.modules,
            materials: &self.materials,
            events: &self.events,
            today: date(today),
        }
    }

    fn timeline(&self, today: &str) -> CourseTimeline {
        course_timeline(&self.input(today))
    }
}

fn codes(timeline: &CourseTimeline) -> Vec<&str> {
    timeline
        .evidence_items
        .iter()
        .map(|item| item.code.as_str())
        .collect()
}

fn dem332() -> Course {
    course(
        UOFT,
        "DEM332H5 F LEC0101 20269",
        "DEM332H5 F LEC0101 20269 Demo Methods",
    )
}

/// Weekly slides "Week 1/2/3" posted on the Tuesdays 09-08, 09-15, 09-22 (08:00 Toronto).
fn weekly_slides() -> Vec<Material> {
    vec![
        material("w1", "Week 1 slides", "2026-09-08T12:00:00Z"),
        material("w2", "Week 2 slides", "2026-09-15T12:00:00Z"),
        material("w3", "Week 3 slides", "2026-09-22T12:00:00Z"),
    ]
}

// ----- alpha.1 (M0) --------------------------------------------------------------------------

/// CAL-1: a UofT-style enrollment window never yields week 22.
#[test]
fn uoft_like_enrollment_window_is_not_used_to_count_weeks() {
    // The regression baseline: v0.1 counted from the term start.
    assert_eq!(
        timeline::week_of(date("2026-05-04"), date("2026-09-28")),
        Some(22)
    );

    let mut case = Case::new(dem332(), uoft_window_term());
    case.materials = weekly_slides();
    let t = case.timeline("2026-09-28");
    assert_eq!(t.current_week, Some(4));
    assert_eq!(t.confidence, Confidence::Medium);
    assert_eq!(t.phase, CoursePhase::Teaching);
    assert_eq!(t.default_week, Some(4));
    assert_eq!(t.term.anchor, TermAnchorSource::PublishedWeekLabels);
    assert_eq!(t.term.week_one_monday, Some(date("2026-09-07")));
    assert!(codes(&t).contains(&"term_looks_like_enrollment_window"));
    assert!(
        t.evidence.iter().any(|line| line
            == "LMS term 'Fall 2026' runs 2026-05-04 → 2027-01-31 (39 weeks): longer than a \
                teaching term, so it is not used to count weeks"),
        "{:#?}",
        t.evidence
    );
    assert_eq!(
        t.term.not_used,
        [RejectedDates {
            source: TermAnchorSource::LmsTerm,
            start: Some(date("2026-05-04")),
            end: Some(date("2027-01-31")),
            reason: RejectReason::LongerThanTeachingTerm,
            end_only: false,
        }]
    );
    // The window still bounds the course: LMS term ∪ session window.
    assert_eq!(
        t.term.outer_frame,
        Some(DateSpan {
            start: date("2026-05-04"),
            end: date("2027-01-31")
        })
    );
    assert!(codes(&t).contains(&"session_window"));
    for today in ["2026-09-10", "2026-10-15", "2026-11-20"] {
        assert_ne!(case.timeline(today).current_week, Some(22), "{today}");
    }
}

/// CAL-2: the same term without week numbers: week unknown, and nothing to prefill the form.
#[test]
fn uoft_like_window_without_labels_is_unknown() {
    let mut case = Case::new(dem332(), uoft_window_term());
    case.materials = vec![
        material("a", "Syllabus", "2026-09-02T12:00:00Z"),
        material("b", "Lecture slides", "2026-09-22T12:00:00Z"),
    ];
    let t = case.timeline("2026-09-28");
    assert_eq!(t.current_week, None);
    assert_eq!(t.phase, CoursePhase::Unknown);
    assert!(!t.outside_term);
    // The dates form prefills from `term.teaching`, never from the Canvas window.
    assert!(t.term.teaching.is_empty());
    assert_eq!(t.term.week_one_monday, None);
    assert_eq!(t.term.anchor, TermAnchorSource::NoAnchor);
    assert_eq!(t.term.not_used.len(), 1);
    assert!(codes(&t).contains(&"no_course_dates"));
    assert!(codes(&t).contains(&"no_week_signal"));
}

/// CAL-3: a bulk day and a page whose `updated_at` drifted are left out of the fit.
#[test]
fn week_fit_ignores_bulk_days_and_edited_pages() {
    let mut case = Case::new(dem332(), uoft_window_term());
    case.materials = weekly_slides();
    // Last year's slides for every week uploaded on one day…
    for week in 1..=12 {
        case.materials.push(material(
            &format!("old{week}"),
            &format!("Week {week} old slides"),
            "2026-09-03T15:00:00Z",
        ));
    }
    // …and a "Week 2 reading" page edited on Wednesday 09-23 (its date is `updated_at`).
    case.materials
        .push(material("page", "Week 2 reading", "2026-09-23T15:00:00Z"));
    case.materials
        .push(material("w4", "Week 4 slides", "2026-09-29T12:00:00Z"));
    let t = case.timeline("2026-09-30");
    assert_eq!(t.term.week_one_monday, Some(date("2026-09-07")));
    assert_eq!(t.current_week, Some(4));
    assert_eq!(t.term.anchor, TermAnchorSource::PublishedWeekLabels);
}

/// CAL-4: "Week 3" posted Sunday 21:00 in Toronto counts for the next week, with and without
/// the course's time zone.
#[test]
fn week_fit_sunday_evening_post_counts_for_next_week() {
    for tz in [Some(TORONTO), None] {
        let mut data = uoft_window_term();
        data.lms.time_zone = tz.map(Into::into);
        let mut case = Case::new(dem332(), data);
        // Sundays 21:00 EDT = Mondays 01:00 UTC.
        case.materials = vec![
            material("w1", "Week 1 slides", "2026-09-07T01:00:00Z"),
            material("w2", "Week 2 slides", "2026-09-14T01:00:00Z"),
            material("w3", "Week 3 slides", "2026-09-21T01:00:00Z"),
        ];
        let t = case.timeline("2026-09-28");
        assert_eq!(t.current_week, Some(4), "{tz:?}");
        assert_eq!(t.term.week_one_monday, Some(date("2026-09-07")), "{tz:?}");
    }
}

/// CAL-5: weeks 1–4 copied into a folder on one day → week 4, never week 3.
#[test]
fn week_fit_same_day_copy_of_several_weeks() {
    let mut case = Case::new(
        course("folder:demo", "DEMO101", "Intro to Demo Studies"),
        CourseTermData::default(),
    );
    case.materials = (1..=4)
        .map(|w| {
            material(
                &format!("w{w}"),
                &format!("Week {w} notes.pdf"),
                "2026-09-28T14:00:00Z",
            )
        })
        .collect();
    let t = case.timeline("2026-09-28");
    assert_eq!(t.current_week, Some(4));
    assert_eq!(t.notes_week, Some(4));
}

/// CAL-6: a solutions file for every week, posted a week later, doesn't pull the week back.
#[test]
fn week_fit_ignores_late_solutions() {
    let mut case = Case::new(dem332(), uoft_window_term());
    case.materials = weekly_slides();
    case.materials
        .push(material("s1", "Week 1 solutions", "2026-09-16T15:00:00Z"));
    case.materials
        .push(material("s2", "Week 2 solutions", "2026-09-23T15:00:00Z"));
    case.materials
        .push(material("s3", "Week 3 solutions", "2026-09-30T15:00:00Z"));
    case.materials
        .push(material("w4", "Week 4 slides", "2026-09-29T12:00:00Z"));
    let t = case.timeline("2026-10-01");
    assert_eq!(t.term.week_one_monday, Some(date("2026-09-07")));
    assert_eq!(t.current_week, Some(4));
    assert_eq!(t.term.anchor_confidence, Confidence::Medium);
}

/// CAL-7 (the resolver half; the migration itself is tested in store_files.rs): after v3
/// cleared the untouched 2027-01-31 prefill, the student's start stays theirs and 12-15 is
/// never "Week 15 · high".
#[test]
fn v3_migration_clears_prefilled_term_dates() {
    let mut data = uoft_window_term();
    data.user_term_start = Some(date("2026-09-08"));
    data.user_term_end = None; // cleared by the v3 migration
    // The LMS course's own dates (plausible) give the last class.
    data.lms.course_start = Some(date("2026-09-08"));
    data.lms.course_end = Some(date("2026-12-08"));
    let case = Case::new(dem332(), data);
    let t = case.timeline("2026-12-15");
    assert_eq!(t.term.anchor, TermAnchorSource::StudentConfirmed);
    assert_eq!(t.term.anchor_origin, Some(CalendarOrigin::Legacy));
    assert_eq!(t.term.student_start, Some(date("2026-09-08")));
    assert_eq!(t.phase, CoursePhase::ExamPeriod);
    assert_eq!(t.current_week, None);
    assert_eq!(t.last_teaching_week, Some(14));
    assert!(codes(&t).contains(&"legacy_dates"));

    // Once confirmed in 0.3 the dates are the student's own.
    let mut confirmed = case;
    confirmed.confirmed = true;
    let t = confirmed.timeline("2026-10-01");
    assert_eq!(t.term.anchor_origin, Some(CalendarOrigin::User));
    assert_eq!((t.current_week, t.confidence), (Some(4), Confidence::High));
}

/// CAL-9: a plausible end and no exam dates: the exam period, then Ended.
#[test]
fn end_only_anchor_gives_exam_period() {
    let mut data = CourseTermData::default();
    data.lms.course_start = Some(date("2026-09-08"));
    data.lms.course_end = Some(date("2026-12-08"));
    data.lms.time_zone = Some(TORONTO.into());
    let case = Case::new(
        course("canvas:lms.example.edu", "DEMO101", "Intro to Demo Studies"),
        data,
    );
    let t = case.timeline("2026-12-16");
    assert_eq!(t.phase, CoursePhase::ExamPeriod);
    assert_eq!(t.phase_confidence, Confidence::Low);
    assert_eq!(t.current_week, None);
    assert_eq!(t.default_week, None);
    assert_eq!(t.last_teaching_week, Some(14));
    assert!(!t.outside_term);
    assert!(codes(&t).contains(&"exam_period_estimated"));
    let t = case.timeline("2026-12-30");
    assert_eq!(t.phase, CoursePhase::Ended);
    assert!(t.outside_term);
    let t = case.timeline("2026-10-01");
    assert_eq!(
        (t.phase, t.current_week, t.term.anchor),
        (
            CoursePhase::Teaching,
            Some(4),
            TermAnchorSource::LmsCourseDates
        )
    );
}

/// CAL-10: a material posted Sunday 23:30 on the day Canada leaves DST lands on the same day
/// in every signal.
#[test]
fn course_dates_use_course_time_zone() {
    let mut data = CourseTermData::default();
    data.lms.course_start = Some(date("2026-09-08"));
    data.lms.course_end = Some(date("2026-12-08"));
    data.lms.time_zone = Some(TORONTO.into());
    let mut case = Case::new(
        course("canvas:lms.example.edu", "DEMO101", "Intro to Demo Studies"),
        data,
    );
    // 2026-11-01 23:30 EST = 2026-11-02 04:30 UTC.
    case.materials = vec![material("n", "Week 8 notes", "2026-11-02T04:30:00Z")];
    let input = case.input("2026-11-03");
    let resolved = resolve_term(&input);
    assert_eq!(resolved.last_activity, Some(date("2026-11-01")));
    let t = course_timeline(&input);
    assert!(
        t.evidence
            .iter()
            .any(|line| line.contains("'Week 8 notes' published 2026-11-01")),
        "{:#?}",
        t.evidence
    );
    // Without the course time zone the same instant is the next (UTC) day everywhere.
    case.data.lms.time_zone = None;
    let t = case.timeline("2026-11-03");
    assert!(
        t.evidence
            .iter()
            .any(|line| line.contains("'Week 8 notes' published 2026-11-02")),
        "{:#?}",
        t.evidence
    );
}

/// A Canvas term from `start` to `end` (Toronto time zone).
fn canvas_term(name: &str, start: &str, end: &str) -> CourseTermData {
    CourseTermData {
        lms: LmsCourseInfo {
            term_name: Some(name.into()),
            term_start: Some(date(start)),
            term_end: Some(date(end)),
            time_zone: Some(TORONTO.into()),
            ..LmsCourseInfo::default()
        },
        synced_term_start: Some(date(start)),
        synced_term_end: Some(date(end)),
        ..CourseTermData::default()
    }
}

/// CAL-8 (the timeline half): in a May–August Canvas term, a summer F course keeps the start
/// but not the end (clipped to June 30); an S course doesn't use the term at all.
#[test]
fn summer_f_end_is_clipped_to_session_window() {
    let term = canvas_term("Summer 2026", "2026-05-04", "2026-08-28");
    let f = Case::new(
        course(
            UOFT,
            "DEM101H5 F LEC0101 20265",
            "DEM101H5 F LEC0101 20265 Demo I",
        ),
        term.clone(),
    );
    let t = f.timeline("2026-07-15");
    assert_eq!(t.term.anchor, TermAnchorSource::LmsTerm);
    assert_eq!(t.term.teaching[0].last_class, Some(date("2026-06-30")));
    assert_eq!(
        t.term.not_used,
        [RejectedDates {
            source: TermAnchorSource::LmsTerm,
            start: None,
            end: Some(date("2026-08-28")),
            reason: RejectReason::EndOutsideSessionWindow,
            end_only: true,
        }]
    );
    assert_eq!(t.phase, CoursePhase::ExamPeriod);
    assert_eq!(t.phase_confidence, Confidence::Low);
    let item = t
        .evidence_items
        .iter()
        .find(|item| item.code == "end_not_used")
        .unwrap();
    assert_eq!(item.param("until"), Some("2026-06-30"));
    // In May the F course is teaching, at Low phase confidence (its end was replaced).
    let may = f.timeline("2026-05-20");
    assert_eq!(
        (may.phase, may.phase_confidence, may.current_week),
        (CoursePhase::Teaching, Confidence::Low, Some(3))
    );

    let mut s = Case::new(
        course(
            UOFT,
            "DEM102H5 S LEC0101 20265",
            "DEM102H5 S LEC0101 20265 Demo II",
        ),
        term,
    );
    // The window-start clause alone (no activity yet).
    let t = s.timeline("2026-06-15");
    assert_eq!(t.term.anchor, TermAnchorSource::NoAnchor);
    assert_eq!(
        t.term.not_used[0].reason,
        RejectReason::StartsBeforeSessionWindow
    );
    // With July materials, week 1 comes from them.
    s.materials = vec![
        material("w1", "Week 1 slides", "2026-07-07T12:00:00Z"),
        material("w2", "Week 2 slides", "2026-07-14T12:00:00Z"),
    ];
    let t = s.timeline("2026-07-15");
    assert_eq!(t.term.not_used.len(), 1);
    assert!(!t.term.not_used[0].end_only);
    assert_eq!(t.term.anchor, TermAnchorSource::PublishedWeekLabels);
    assert_eq!((t.phase, t.current_week), (CoursePhase::Teaching, Some(2)));
}

/// CAL-15 (the timeline half): a full-year Y course inside a January-ending Canvas term keeps
/// teaching until the session window's end.
#[test]
fn teaching_phase_blocks_weak_end_signals() {
    let case = Case::new(
        course(
            UOFT,
            "DEM137Y5 Y LEC0101 20269",
            "DEM137Y5 Y LEC0101 20269 Demo Year",
        ),
        canvas_term("Fall-Winter 2026", "2026-09-01", "2027-01-31"),
    );
    let t = case.timeline("2027-02-24");
    assert_eq!(t.term.teaching[0].last_class, Some(date("2027-04-30")));
    assert_eq!(
        t.term.not_used[0].reason,
        RejectReason::EndOutsideSessionWindow
    );
    assert!(t.term.not_used[0].end_only);
    assert_eq!(t.phase, CoursePhase::Teaching);
    assert_eq!(t.phase_confidence, Confidence::Low);
    assert_eq!(t.current_week, Some(26));
    assert!(!t.outside_term);
}
