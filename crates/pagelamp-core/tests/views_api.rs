//! Tests of `pagelamp_core::views` over an in-memory store with a fixed `AsOf`.
//! All data is synthetic ("DEMO101 Intro to Demo Studies" and friends).

use chrono::{DateTime, NaiveDate, Utc};
use pagelamp_core::Error;
use pagelamp_core::model::*;
use pagelamp_core::store::Store;
use pagelamp_core::views::{self, AsOf, WeekNoteKind};
use serde_json::json;

// ----- fixtures -----------------------------------------------------------------------------

const SOURCE: &str = "canvas:lms.example.edu";

fn ts(text: &str) -> Timestamp {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

/// Thursday of teaching week 3 (term starts Monday 2026-09-07).
fn at() -> AsOf {
    AsOf {
        now: ts("2026-09-24T12:00:00Z"),
        today: date("2026-09-24"),
        tz: None,
    }
}

fn cid(external: &str) -> String {
    format!("{SOURCE}/course/{external}")
}

fn add_course(store: &Store, external: &str, code: Option<&str>, name: &str, term: bool) {
    store
        .upsert_course(&CourseUpsert {
            id: cid(external),
            source_id: SOURCE.into(),
            external_id: external.into(),
            code: code.map(Into::into),
            name: name.into(),
            term_start: term.then(|| date("2026-09-07")),
            term_end: term.then(|| date("2026-12-18")),
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
}

fn module(course: &str, id: &str, name: &str, week: Option<u32>) -> Module {
    Module {
        id: format!("{SOURCE}/module/{id}"),
        course_id: cid(course),
        name: name.into(),
        position: None,
        unlock_at: None,
        week_hint: week,
    }
}

/// A material with the given chunk texts (text_status ok when it has chunks).
#[allow(clippy::too_many_arguments)]
fn add_material(
    store: &Store,
    course: &str,
    id: &str,
    kind: MaterialKind,
    module: Option<&str>,
    week: Option<u32>,
    published: &str,
    texts: &[&str],
) -> String {
    let material_id = format!("{SOURCE}/file/{id}");
    store
        .upsert_material(&MaterialUpsert {
            id: material_id.clone(),
            course_id: cid(course),
            module_id: module.map(|m| format!("{SOURCE}/module/{m}")),
            kind,
            title: id.into(),
            url: Some(format!("https://lms.example.edu/files/{id}")),
            local_path: None,
            mime: None,
            published_at: Some(ts(published)),
            week_hint: week,
        })
        .unwrap();
    let chunks: Vec<Chunk> = texts
        .iter()
        .enumerate()
        .map(|(ord, text)| Chunk {
            material_id: material_id.clone(),
            ord: ord as u32,
            locator: Some(format!("p. {}", ord + 1)),
            text: (*text).into(),
        })
        .collect();
    store.replace_chunks(&material_id, &chunks).unwrap();
    if !texts.is_empty() {
        store
            .set_text_state(&material_id, TextStatus::Ok, None, Some("hash"))
            .unwrap();
    }
    material_id
}

fn event(id: &str, course: Option<&str>, title: &str, due: &str) -> Event {
    Event {
        id: format!("{SOURCE}/assignment/{id}"),
        source_id: SOURCE.into(),
        course_id: course.map(cid),
        kind: EventKind::AssignmentDue,
        title: title.into(),
        starts_at: None,
        ends_at: None,
        due_at: Some(ts(due)),
        url: None,
        updated_at: ts("2026-09-01T00:00:00Z"),
        course_hint: None,
    }
}

/// DEMO101 (visible, term known), DEMO202 (visible, no term), DEMO303 (hidden).
fn demo_store() -> Store {
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
        .record_sync(SOURCE, ts("2026-09-24T08:00:00Z"), None)
        .unwrap();
    add_course(
        &store,
        "101",
        Some("DEMO101"),
        "Intro to Demo Studies",
        true,
    );
    add_course(
        &store,
        "202",
        Some("DEMO202"),
        "Advanced Demo Studies",
        false,
    );
    add_course(&store, "303", Some("DEMO303"), "Hidden Demo Seminar", true);
    store.set_course_hidden(&cid("303"), true).unwrap();

    store
        .replace_modules(
            &cid("101"),
            &[
                module("101", "w1", "Week 1: Basics", Some(1)),
                module("101", "w2", "Week 2: Methods", Some(2)),
                module("101", "w3", "Week 3: Practice", Some(3)),
                module("101", "misc", "Resources", None),
            ],
        )
        .unwrap();
    // Week 1 by own hint; week 2 via its module; week 3 via publish date only.
    add_material(
        &store,
        "101",
        "intro",
        MaterialKind::File,
        Some("w1"),
        Some(1),
        "2026-09-07T09:00:00Z",
        &["alpha basics"],
    );
    add_material(
        &store,
        "101",
        "methods",
        MaterialKind::File,
        Some("w2"),
        None,
        "2026-09-14T09:00:00Z",
        &["beta methods"],
    );
    add_material(
        &store,
        "101",
        "practice",
        MaterialKind::Page,
        None,
        None,
        "2026-09-22T09:00:00Z",
        &["gamma practice", "delta more"],
    );
    add_material(
        &store,
        "101",
        "video",
        MaterialKind::ExternalLink,
        Some("misc"),
        None,
        "2026-09-23T09:00:00Z",
        &[],
    );
    add_material(
        &store,
        "101",
        "notice",
        MaterialKind::Announcement,
        None,
        None,
        "2026-09-23T10:00:00Z",
        &["Room change for the lab"],
    );
    add_material(
        &store,
        "101",
        "old-notice",
        MaterialKind::Announcement,
        None,
        None,
        "2026-08-01T10:00:00Z",
        &["Welcome"],
    );
    add_material(
        &store,
        "303",
        "secret",
        MaterialKind::File,
        None,
        Some(1),
        "2026-09-20T09:00:00Z",
        &["hidden epsilon"],
    );

    store
        .replace_events(
            SOURCE,
            &[
                event("a1", Some("101"), "Problem Set 1", "2026-09-26T23:59:00Z"),
                event("a2", Some("101"), "Problem Set 2", "2026-10-30T23:59:00Z"),
                event("a0", Some("101"), "Problem Set 0", "2026-09-20T23:59:00Z"),
                event("h1", Some("303"), "Hidden essay", "2026-09-27T23:59:00Z"),
                event("x1", None, "Campus event", "2026-09-28T18:00:00Z"),
            ],
        )
        .unwrap();
    store
}

fn titles(views: &[views::MaterialView]) -> Vec<&str> {
    views.iter().map(|v| v.title.as_str()).collect()
}

fn deadline_titles(deadlines: &[views::Deadline]) -> Vec<&str> {
    deadlines.iter().map(|d| d.event.title.as_str()).collect()
}

// ----- list_courses ---------------------------------------------------------------------------

#[test]
fn list_courses_summarises_each_course() {
    let store = demo_store();
    let courses = views::list_courses(&store, false, at()).unwrap();
    let codes: Vec<_> = courses
        .iter()
        .map(|c| c.course.code.clone().unwrap())
        .collect();
    assert_eq!(codes, ["DEMO101", "DEMO202"]);

    let demo = &courses[0];
    assert_eq!(demo.ai_materials, AiMaterialsState::Readable);
    assert_eq!(demo.timeline.current_week, Some(3));
    assert_eq!(demo.counts.modules, 4);
    assert_eq!(demo.counts.materials, 6);
    // intro, methods, practice, notice, old-notice have text; the link has none.
    assert_eq!(demo.counts.indexed_materials, 5);
    // PS1 (Sep 26) and PS2 (Oct 30 is > 21 days away → excluded); PS0 is past.
    assert_eq!(demo.counts.upcoming_deadlines, 1);
    let next = demo.next_deadline.as_ref().unwrap();
    assert_eq!(next.event.title, "Problem Set 1");
    assert_eq!(next.course_code.as_deref(), Some("DEMO101"));
    assert_eq!(demo.source_label, "Demo LMS");
    assert_eq!(demo.last_synced_at, Some(ts("2026-09-24T08:00:00Z")));

    let all = views::list_courses(&store, true, at()).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all.iter().any(|c| c.course.hidden));
}

#[test]
fn readable_count_is_zero_unless_course_is_readable() {
    let store = demo_store();
    store.set_course_ai_access(&cid("101"), false).unwrap();
    let demo = &views::list_courses(&store, false, at()).unwrap()[0];
    assert_eq!(demo.ai_materials, AiMaterialsState::TurnedOff);
    assert_eq!(demo.counts.indexed_materials, 0);
    assert_eq!(demo.counts.materials, 6, "structure stays visible");

    store.set_course_ai_access(&cid("101"), true).unwrap();
    store
        .set_course_policy(&cid("101"), AiPolicy::Prohibited, None)
        .unwrap();
    let demo = &views::list_courses(&store, false, at()).unwrap()[0];
    assert_eq!(demo.ai_materials, AiMaterialsState::WithheldByPolicy);
    assert_eq!(demo.counts.indexed_materials, 0);
}

// ----- course_overview ------------------------------------------------------------------------

#[test]
fn course_overview_collects_recent_and_upcoming() {
    let store = demo_store();
    let overview = views::course_overview(&store, "demo101", false, at()).unwrap();
    assert_eq!(overview.course.id, cid("101"));
    assert_eq!(overview.timeline.current_week, Some(3));
    let current: Vec<_> = overview
        .current_modules
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(current, ["Week 3: Practice"]);
    // Published within 14 days, newest first, announcements separate.
    assert_eq!(
        titles(&overview.recent_materials),
        ["video", "practice", "methods"]
    );
    assert_eq!(titles(&overview.recent_announcements), ["notice"]);
    assert_eq!(
        deadline_titles(&overview.upcoming_deadlines),
        ["Problem Set 1"]
    );
    assert_eq!(overview.ai_materials, AiMaterialsState::Readable);
    assert_eq!(overview.source_label, "Demo LMS");
    // Module names are filled in.
    let methods = &overview.recent_materials[2];
    assert_eq!(methods.module_name.as_deref(), Some("Week 2: Methods"));
    assert_eq!(methods.chunk_count, 1);
    assert_eq!(overview.downloadable_files, 0);
}

#[test]
fn downloadable_files_counts_what_a_download_would_fetch() {
    let store = demo_store();
    let file = |id: &str, kind: MaterialKind| {
        add_material(
            &store,
            "101",
            id,
            kind,
            None,
            Some(9),
            "2026-01-05T10:00:00Z",
            &[],
        )
    };
    for (id, kind, blocked) in [
        ("old-slides", MaterialKind::File, None),
        ("old-notes", MaterialKind::File, None),
        ("locked", MaterialKind::File, Some(DownloadBlock::Locked)),
        ("huge", MaterialKind::File, Some(DownloadBlock::TooLarge)),
        ("a-page", MaterialKind::Page, None),
    ] {
        let id = file(id, kind);
        store
            .set_text_state(&id, TextStatus::NotDownloaded, None, None)
            .unwrap();
        store.set_download_blocked(&id, blocked).unwrap();
    }
    let overview = views::course_overview(&store, "DEMO101", false, at()).unwrap();
    assert_eq!(
        overview.downloadable_files, 2,
        "any week, files only, not blocked"
    );
}

#[test]
fn hidden_courses_are_reachable_only_with_include_hidden() {
    let store = demo_store();
    assert!(matches!(
        views::course_overview(&store, "DEMO303", false, at()),
        Err(Error::NotFound(_))
    ));
    let overview = views::course_overview(&store, "DEMO303", true, at()).unwrap();
    assert!(overview.course.hidden);
    assert!(views::week_materials(&store, "DEMO303", Some(1), true, at()).is_ok());
    assert!(views::week_materials(&store, "DEMO303", Some(1), false, at()).is_err());
    let hidden = views::deadlines(&store, Some("DEMO303"), 21, 0, true, at()).unwrap();
    assert_eq!(deadline_titles(&hidden), ["Hidden essay"]);
    assert!(views::deadlines(&store, Some("DEMO303"), 21, 0, false, at()).is_err());
}

// ----- week_materials -------------------------------------------------------------------------

#[test]
fn week_materials_uses_hint_then_module_then_publish_date() {
    let store = demo_store();
    let week1 = views::week_materials(&store, "DEMO101", Some(1), false, at()).unwrap();
    assert_eq!(week1.week, Some(1));
    assert_eq!(week1.requested_week, Some(1));
    assert_eq!(titles(&week1.materials), ["intro"]);
    assert_eq!(week1.modules.len(), 1);
    assert_eq!(week1.note_kind, None);

    let week2 = views::week_materials(&store, "DEMO101", Some(2), false, at()).unwrap();
    assert_eq!(titles(&week2.materials), ["methods"]);

    // Default = current week (3): "practice" belongs via its publish date (Sep 22).
    // "video" has no hint, a module without a week, and was published Sep 23 → week 3 too.
    let current = views::week_materials(&store, "DEMO101", None, false, at()).unwrap();
    assert_eq!(current.week, Some(3));
    assert_eq!(current.requested_week, None);
    let mut shown = titles(&current.materials);
    shown.sort();
    assert_eq!(shown, ["practice", "video"]);
    assert_eq!(current.available_weeks, [1, 2, 3]);
    assert_eq!(current.ai_materials, AiMaterialsState::Readable);
}

#[test]
fn week_materials_notes() {
    let store = demo_store();
    let empty = views::week_materials(&store, "DEMO101", Some(9), false, at()).unwrap();
    assert_eq!(empty.note_kind, Some(WeekNoteKind::NoMaterialsThisWeek));
    assert!(empty.note.as_deref().unwrap().contains("week 9"));

    // DEMO202: no term, no modules, no hints → current week unknown → recent materials.
    add_material(
        &store,
        "202",
        "fresh",
        MaterialKind::File,
        None,
        None,
        "2026-09-21T09:00:00Z",
        &["zeta"],
    );
    add_material(
        &store,
        "202",
        "stale",
        MaterialKind::File,
        None,
        None,
        "2026-06-01T09:00:00Z",
        &["eta"],
    );
    let unknown = views::week_materials(&store, "DEMO202", None, false, at()).unwrap();
    assert_eq!(unknown.week, None);
    assert_eq!(unknown.note_kind, Some(WeekNoteKind::CurrentWeekUnknown));
    assert_eq!(titles(&unknown.materials), ["fresh"]);

    // Changed in v0.3 (calendar design §6.6): after the last class (Dec 18) there is no
    // teaching week. Until Dec 18 + 21 days it is the exam period: no week by default, the
    // materials of the last 14 days instead…
    add_material(
        &store,
        "101",
        "exam-review",
        MaterialKind::File,
        None,
        Some(18),
        "2027-01-04T09:00:00Z",
        &["theta"],
    );
    let exams = AsOf {
        now: ts("2027-01-05T12:00:00Z"),
        today: date("2027-01-05"),
        tz: None,
    };
    let exam_period = views::week_materials(&store, "DEMO101", None, false, exams).unwrap();
    assert!(!exam_period.timeline.outside_term);
    assert_eq!(exam_period.timeline.phase, CoursePhase::ExamPeriod);
    assert_eq!(exam_period.week, None);
    assert_eq!(exam_period.note_kind, Some(WeekNoteKind::ExamPeriod));
    assert_eq!(titles(&exam_period.materials), ["exam-review"]);
    // …then Ended, flagged as outside the term.
    let later = AsOf {
        now: ts("2027-01-15T12:00:00Z"),
        today: date("2027-01-15"),
        tz: None,
    };
    let outside = views::week_materials(&store, "DEMO101", None, false, later).unwrap();
    assert!(outside.timeline.outside_term);
    assert_eq!(outside.week, None);
    assert_eq!(outside.note_kind, Some(WeekNoteKind::OutsideTerm));
    assert_eq!(titles(&outside.materials), ["exam-review"]);
    // An explicitly requested week is shown and not flagged.
    let explicit = views::week_materials(&store, "DEMO101", Some(18), false, later).unwrap();
    assert_eq!(titles(&explicit.materials), ["exam-review"]);
    assert_eq!(explicit.note_kind, None);
}

// ----- deadlines ------------------------------------------------------------------------------

#[test]
fn deadlines_window_and_hidden_filtering() {
    let store = demo_store();
    let upcoming = views::deadlines(&store, None, 21, 0, false, at()).unwrap();
    // Hidden course's essay excluded; course-less campus event kept; sorted by date.
    assert_eq!(
        deadline_titles(&upcoming),
        ["Problem Set 1", "Campus event"]
    );
    assert_eq!(
        upcoming[0].course_name.as_deref(),
        Some("Intro to Demo Studies")
    );
    assert_eq!(upcoming[1].course_code, None);

    let with_past = views::deadlines(&store, Some("DEMO101"), 60, 7, false, at()).unwrap();
    assert_eq!(
        deadline_titles(&with_past),
        ["Problem Set 0", "Problem Set 1", "Problem Set 2"]
    );

    // Absurd windows must not panic.
    let everything = views::deadlines(&store, None, u32::MAX, u32::MAX, false, at()).unwrap();
    assert_eq!(everything.len(), 4);
}

// ----- announcements --------------------------------------------------------------------------

#[test]
fn announcements_are_recent_capped_and_withheld_when_not_readable() {
    let store = demo_store();
    let items = views::announcements(&store, "DEMO101", 14, 1000, at()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].text, "Room change for the lab");
    assert!(!items[0].truncated);

    let capped = views::announcements(&store, "DEMO101", 14, 4, at()).unwrap();
    assert_eq!(capped[0].text, "Room");
    assert!(capped[0].truncated);

    let old = views::announcements(&store, "DEMO101", 90, 1000, at()).unwrap();
    assert_eq!(old.len(), 2);

    store
        .set_course_policy(&cid("101"), AiPolicy::Prohibited, None)
        .unwrap();
    let withheld = views::announcements(&store, "DEMO101", 14, 1000, at()).unwrap();
    assert_eq!(withheld.len(), 1, "titles stay visible");
    assert_eq!(withheld[0].text, "");
    assert_eq!(withheld[0].ai_materials, AiMaterialsState::WithheldByPolicy);
}

// ----- read_material --------------------------------------------------------------------------

#[test]
fn read_material_paginates_by_character_budget() {
    let store = demo_store();
    let long: Vec<String> = (0..10)
        .map(|i| format!("chunk {i} {}", "é".repeat(90)))
        .collect();
    let texts: Vec<&str> = long.iter().map(String::as_str).collect();
    let id = add_material(
        &store,
        "101",
        "long",
        MaterialKind::File,
        None,
        Some(2),
        "2026-09-15T09:00:00Z",
        &texts,
    );

    // Each chunk is 98 chars; budget 250 → 2 chunks per page.
    let first = views::read_material(&store, &id, 0, 250).unwrap();
    assert_eq!(first.chunks.len(), 2);
    assert_eq!(first.next_chunk, Some(2));
    assert_eq!(first.total_chunks, 10);
    assert!(!first.truncated);
    assert_eq!(first.course_code.as_deref(), Some("DEMO101"));

    let last = views::read_material(&store, &id, 8, 250).unwrap();
    assert_eq!(last.chunks.len(), 2);
    assert_eq!(last.next_chunk, None);

    let past_end = views::read_material(&store, &id, 50, 250).unwrap();
    assert!(past_end.chunks.is_empty());
    assert_eq!(past_end.next_chunk, None);
}

#[test]
fn read_material_truncates_an_oversized_single_chunk_on_a_char_boundary() {
    let store = demo_store();
    let huge = "漢".repeat(1000);
    let id = add_material(
        &store,
        "101",
        "huge",
        MaterialKind::File,
        None,
        None,
        "2026-09-15T09:00:00Z",
        &[&huge, "tail"],
    );
    let page = views::read_material(&store, &id, 0, 300).unwrap();
    assert_eq!(page.chunks.len(), 1);
    assert!(page.truncated);
    assert_eq!(page.chunks[0].text.chars().count(), 300);
    assert_eq!(page.next_chunk, Some(1));
    // max_chars below the minimum is clamped up, never to zero.
    let tiny = views::read_material(&store, &id, 1, 1).unwrap();
    assert_eq!(tiny.chunks[0].text, "tail");
}

#[test]
fn read_material_respects_hidden_courses_and_ai_access() {
    let store = demo_store();
    let secret = format!("{SOURCE}/file/secret");
    assert!(matches!(
        views::read_material(&store, &secret, 0, 1000),
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        views::read_material(&store, "nope", 0, 1000),
        Err(Error::NotFound(_))
    ));

    let intro = format!("{SOURCE}/file/intro");
    store.set_course_ai_access(&cid("101"), false).unwrap();
    let off = views::read_material(&store, &intro, 0, 1000).unwrap();
    assert_eq!(off.ai_materials, AiMaterialsState::TurnedOff);
    assert!(off.chunks.is_empty());
    assert_eq!(off.next_chunk, None);
    assert_eq!(off.total_chunks, 1, "structure (counts) stays visible");
    assert_eq!(off.material.title, "intro");

    store.set_course_ai_access(&cid("101"), true).unwrap();
    let on = views::read_material(&store, &intro, 0, 1000).unwrap();
    assert_eq!(on.chunks[0].text, "alpha basics");
}

// ----- search ---------------------------------------------------------------------------------

#[test]
fn student_search_and_ai_search_differ_only_by_ai_access() {
    let store = demo_store();
    add_material(
        &store,
        "202",
        "shared",
        MaterialKind::File,
        None,
        None,
        "2026-09-21T09:00:00Z",
        &["alpha advanced"],
    );
    store.set_course_ai_access(&cid("202"), false).unwrap();

    let student = views::search(&store, "alpha", None, 10).unwrap();
    assert_eq!(student.len(), 2);
    // Hidden course never appears.
    assert!(
        views::search(&store, "epsilon", None, 10)
            .unwrap()
            .is_empty()
    );

    let ai = views::search_for_ai(&store, "alpha", None, 10).unwrap();
    assert_eq!(ai.hits.len(), 1);
    assert_eq!(ai.hits[0].course_code.as_deref(), Some("DEMO101"));
    assert_eq!(ai.excluded_courses, ["DEMO202"]);
    assert_eq!(ai.course_ai_materials, None);

    let filtered = views::search_for_ai(&store, "alpha", Some("DEMO202"), 10).unwrap();
    assert!(filtered.hits.is_empty());
    assert_eq!(
        filtered.course_ai_materials,
        Some(AiMaterialsState::TurnedOff)
    );

    let readable = views::search_for_ai(&store, "alpha", Some("DEMO101"), 10).unwrap();
    assert_eq!(readable.hits.len(), 1);
    assert_eq!(
        readable.course_ai_materials,
        Some(AiMaterialsState::Readable)
    );

    assert!(matches!(
        views::search_for_ai(&store, "alpha", Some("DEMO303"), 10),
        Err(Error::NotFound(_))
    ));
}

// ----- sync_status ----------------------------------------------------------------------------

#[test]
fn sync_status_flags_stale_and_failing_sources() {
    let store = demo_store();
    let fresh = views::sync_status(&store, at()).unwrap();
    assert!(!fresh.stale);
    assert_eq!(fresh.last_synced_at, Some(ts("2026-09-24T08:00:00Z")));
    assert_eq!(fresh.counts.courses, 2);
    assert_eq!(fresh.counts.hidden_courses, 1);

    let two_days_later = AsOf {
        now: ts("2026-09-26T12:00:00Z"),
        today: date("2026-09-26"),
        tz: None,
    };
    assert!(views::sync_status(&store, two_days_later).unwrap().stale);

    store
        .record_sync(
            SOURCE,
            ts("2026-09-24T11:00:00Z"),
            Some((SourceErrorKind::AuthExpiredOrRevoked, "token expired")),
        )
        .unwrap();
    let failing = views::sync_status(&store, at()).unwrap();
    assert!(failing.stale);
    assert!(failing.sources[0].stale);

    let empty = Store::open_in_memory().unwrap();
    assert!(views::sync_status(&empty, at()).unwrap().stale);
}
