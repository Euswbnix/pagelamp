//! The course calendar facade (docs/design/v0.3-course-calendar.md §4, §7): the view with its
//! candidates and why AI reading can't run, the reading offers, the scan, accepting and
//! dismissing, the dates form, and reading the syllabus with a model (a local mock server:
//! no real provider is ever called). All data is synthetic; dates follow today, since the
//! facade reads the clock.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, NaiveDate, SubsecRound, TimeDelta, Utc};
use pagelamp_app::ai::{
    BackendRef, BackendState, CostBasis, CostKind, EstimateRequest, GenEvent, GenNoticeCode,
    ModelChoice, RetentionFact,
};
use pagelamp_app::{
    ActivityItem, ActivityKind, App, AppErrorKind, CalendarBatchEvent, ReadCalendarOptions,
};
use pagelamp_core::ai::{
    AiFeature, BlockReason, Effort, MaterialSharing, ModelErrorKind, ProviderRow, UsageRecord,
};
use pagelamp_core::calendar::candidates::{CandidateLeftOut, CandidateReason};
use pagelamp_core::model::*;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::{GenerationStatus, Store};
use pagelamp_core::term::CalendarStatus;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SOURCE: &str = "canvas:lms.example.edu";

fn course_id(external: &str) -> String {
    format!("{SOURCE}/course/{external}")
}

fn day(offset: i64) -> NaiveDate {
    Local::now().date_naive() + TimeDelta::days(offset)
}

/// "September 16, 2026", as a syllabus writes it.
fn spoken(date: NaiveDate) -> String {
    date.format("%B %-d, %Y").to_string()
}

fn first_class_words() -> String {
    format!("Classes begin {}.", spoken(day(-13)))
}

fn last_class_words() -> String {
    format!("The last day of classes is {}.", spoken(day(70)))
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
    for external in ["101", "202", "303", "404", "505"] {
        store
            .upsert_course(&CourseUpsert {
                id: course_id(external),
                source_id: SOURCE.into(),
                external_id: external.into(),
                code: Some(format!("DEMO{external}")),
                name: format!("Demo course {external}"),
                // Teaching now, so every course is current.
                term_start: Some(day(-14)),
                term_end: Some(day(90)),
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
    }
    let syllabus = format!(
        "{} {} Office hours are posted on the course page every week, and the late policy \
         and the grading scheme are in the outline. Bring questions to tutorials: they are \
         the best place to practise before the tests.",
        first_class_words(),
        last_class_words()
    );
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
        "Week 1 Basics",
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
            "Week 1 Basics",
        );
    }
    drop(store);
    app.set_course_hidden("DEMO303", true).unwrap();
    app.set_course_policy("DEMO404", AiPolicy::Prohibited, None)
        .unwrap();
    app.set_course_ai_access("DEMO505", false).unwrap();
    app
}

/// A model on this computer (an Ollama mock), chosen for syllabus reading and disclosed.
async fn with_local_model(app: &App) -> MockServer {
    with_local_model_listing(app, tags(json!([{ "name": "local-model" }]))).await
}

/// The mock daemon emptied, still listing its model (a run asks for the list first).
async fn reset_listing(server: &MockServer) {
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(tags(json!([{ "name": "local-model" }])))
        .mount(server)
        .await;
}

/// Ollama's model list (`/api/tags`), as its daemon gives it.
fn tags(models: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({ "models": models }))
}

/// `with_local_model`, its daemon answering the model list with `list`: a run asks for it to
/// learn where the model runs.
async fn with_local_model_listing(app: &App, list: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(list)
        .mount(&server)
        .await;
    with_ollama_at(app, &server.uri());
    server
}

/// Ollama at `base_url` (on this computer), its `local-model` chosen for syllabus reading and
/// its note accepted.
fn with_ollama_at(app: &App, base_url: &str) {
    Store::open(&app.db_path())
        .unwrap()
        .insert_model_provider(&ProviderRow {
            id: "ollama".into(),
            preset: "ollama".into(),
            label: "Ollama".into(),
            wire: "ollama_native".into(),
            base_url: base_url.into(),
            created_at: Utc::now().trunc_subsecs(0),
            last_probe_json: None,
        })
        .unwrap();
    let backend = BackendRef::Provider {
        provider_id: "ollama".into(),
    };
    app.set_feature_model(
        AiFeature::CourseCalendar,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "local-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let version = app
        .ai_status()
        .unwrap()
        .backends
        .iter()
        .find(|b| b.backend == backend)
        .unwrap()
        .disclosure
        .version;
    app.acknowledge_ai_disclosure(&backend, version).unwrap();
}

/// Ollama's streamed answer carrying `answer` as the JSON text.
fn answer(answer: &serde_json::Value) -> ResponseTemplate {
    let lines = [
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:00Z",
               "message": {"role": "assistant", "content": answer.to_string()}, "done": false}),
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:01Z",
               "message": {"role": "assistant", "content": ""}, "done": true,
               "done_reason": "stop", "prompt_eval_count": 900, "eval_count": 80}),
    ];
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/x-ndjson")
        .set_body_string(body)
}

fn claim(kind: &str, date: NaiveDate, quote: &str, source: &str) -> serde_json::Value {
    json!({"kind": kind, "date": date.to_string(), "end_date": null, "label": "",
           "quote": quote, "source": source})
}

/// What a model reading DEMO101's syllabus (handle c1) answers.
fn good_extraction() -> serde_json::Value {
    json!({
        "stated_term": {"text": null, "quote": null, "source": null},
        "claims": [
            claim("first_class", day(-13), &first_class_words(), "c1"),
            claim("last_class", day(70), &last_class_words(), "c1"),
        ],
        "weeks": [],
        "not_found": ["breaks", "exam_period", "final_exam", "weeks"]
    })
}

fn collect() -> (Arc<Mutex<Vec<GenEvent>>>, impl Fn(GenEvent) + Send + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    (events, move |event| sink.lock().unwrap().push(event))
}

/// Wait until the mock model has been asked (the run is waiting for its answer; a run first
/// asks the server here for its model list).
async fn wait_for_request(server: &MockServer) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() == "/api/tags")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "the run never started"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The generations `activity()` lists, by id.
fn running_generations(app: &App) -> Vec<String> {
    app.activity()
        .items
        .into_iter()
        .map(|item: ActivityItem| {
            assert_eq!(
                (item.kind, item.source_id),
                (ActivityKind::Generation, None)
            );
            item.generation_id.unwrap()
        })
        .collect()
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
    // No model is chosen yet.
    assert_eq!(view.blocked, Some(BlockReason::NoModelChosen));
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
    let wire = serde_json::to_value(&view).unwrap();
    assert_eq!(wire["status"], "none");
    assert_eq!(wire["blocked"], "no_model_chosen");
    assert_eq!(wire["candidates"][2]["left_out"], "no_text");
}

#[tokio::test]
async fn the_outline_a_folder_s_course_toml_names_is_a_candidate() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open_at_with_secrets(temp.path().join("data"), Arc::new(MemorySecrets::new()))
        .unwrap();
    let course = temp.path().join("Courses").join("DEMO707 Field Methods");
    std::fs::create_dir_all(course.join("Admin")).unwrap();
    // A title no rule would pick: only course.toml says it is the outline.
    std::fs::write(
        course.join("Admin").join("info.md"),
        format!(
            "# Field Methods\n\n{}\n",
            "Classes, fieldwork and the final exam. ".repeat(12)
        ),
    )
    .unwrap();
    std::fs::write(course.join("notes.md"), "# Notes\n\nSampling notes.\n").unwrap();
    std::fs::write(course.join("course.toml"), "outline = \"Admin/info.md\"\n").unwrap();
    let source = app
        .add_folder_source(&temp.path().join("Courses"), None, None)
        .unwrap();
    app.sync_source(&source.id, pagelamp_app::SyncRequest::default(), |_| {})
        .await
        .unwrap();
    let candidates: Vec<(String, CandidateReason, bool)> = app
        .calendar_candidates("DEMO707")
        .unwrap()
        .into_iter()
        .map(|c| (c.title, c.reason, c.included))
        .collect();
    assert_eq!(
        candidates,
        [(
            "info.md".to_string(),
            CandidateReason::NamedInCourseToml,
            true
        )]
    );
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

#[test]
fn the_student_chooses_which_materials_are_read() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let slides = format!("{SOURCE}/file/outline");
    let candidates = app
        .set_calendar_sources("DEMO101", vec![], vec![slides.clone()])
        .unwrap();
    let outline = candidates.iter().find(|c| c.material_id == slides).unwrap();
    assert_eq!(outline.left_out, Some(CandidateLeftOut::ExcludedByStudent));
    // A material of another course is refused.
    let other = format!("{SOURCE}/file/slides");
    let err = app
        .set_calendar_sources("DEMO101", vec![other], vec![])
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Invalid);
}

#[tokio::test]
async fn the_scan_proposes_and_the_student_decides() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let proposal = app
        .scan_course_calendar("DEMO101")
        .unwrap()
        .expect("dates in the syllabus");
    assert_eq!(proposal.origin, CalendarOrigin::Scan);
    assert_eq!(proposal.calendar.segments[0].first_class, day(-13));
    assert_eq!(proposal.calendar.segments[0].last_class, Some(day(70)));
    assert!(proposal.ai_label.is_none());
    let view = app.course_calendar("DEMO101").unwrap();
    assert_eq!(view.status, CalendarStatus::Proposed);
    assert_eq!(view.proposals[0].id, proposal.id);

    // Dismissed: the same materials aren't scanned into a proposal again.
    app.dismiss_calendar_proposal(proposal.id).unwrap();
    assert!(app.scan_course_calendar("DEMO101").unwrap().is_none());
    assert_eq!(
        app.dismiss_calendar_proposal(proposal.id).unwrap_err().kind,
        AppErrorKind::NotFound
    );

    // The dates form puts the student's own calendar in force; "Undo" clears it.
    let view = app
        .set_course_dates(
            "DEMO101",
            Some(pagelamp_app::CourseDatesInput {
                first_class: Some(day(-13)),
                last_class: Some(day(70)),
                exams_end: None,
                breaks: vec![],
                second_segment: None,
            }),
        )
        .unwrap();
    assert_eq!(view.status, CalendarStatus::Accepted);
    assert_eq!(view.accepted.unwrap().origin, CalendarOrigin::User);
    let timeline = app.course_timeline("DEMO101").unwrap();
    assert_eq!(timeline.term.anchor_origin, Some(CalendarOrigin::User));
    let view = app.set_course_dates("DEMO101", None).unwrap();
    assert!(view.accepted.is_none());
    // Dates the form can't take are all reported.
    let err = app
        .set_course_dates(
            "DEMO101",
            Some(pagelamp_app::CourseDatesInput {
                first_class: Some(day(10)),
                last_class: Some(day(0)),
                exams_end: None,
                breaks: vec![],
                second_segment: None,
            }),
        )
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Invalid);
    // Downloading chosen files takes the course's own files only.
    for ids in [vec![], vec![format!("{SOURCE}/syllabus/101")]] {
        let err = app
            .download_material_files("DEMO101", ids, |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::Invalid);
    }
}

#[tokio::test]
async fn a_model_reads_the_syllabus_into_a_proposal_the_student_accepts() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    assert_eq!(app.course_calendar("DEMO101").unwrap().blocked, None);
    let estimate = app
        .estimate_generation(&EstimateRequest::CourseCalendar {
            courses: vec!["DEMO101".into()],
        })
        .unwrap();
    assert_eq!(estimate.would_block, None, "{estimate:?}");
    assert!(estimate.input_tokens > 0);

    let (events, on_event) = collect();
    let proposal = app
        .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), on_event)
        .await
        .unwrap();
    assert_eq!(proposal.origin, CalendarOrigin::Ai);
    assert_eq!(proposal.calendar.segments[0].first_class, day(-13));
    assert!(proposal.passing, "{:?}", proposal.conflicts);
    let label = proposal.ai_label.clone().unwrap();
    assert_eq!(
        (label.backend_label.as_str(), label.model.as_str()),
        ("Ollama", "local-model")
    );
    assert!(label.on_device);
    assert!(!proposal.sharing_reminder, "nothing left this computer");
    let first = &proposal.dates[0].evidence[0];
    assert_eq!(first.quote.as_deref(), Some(first_class_words().as_str()));

    // Progress, from building the context to the end.
    let events = events.lock().unwrap().clone();
    assert!(matches!(events.first(), Some(GenEvent::Stage { .. })));
    assert!(events.iter().any(|e| matches!(
        e,
        GenEvent::Started {
            on_device: true,
            ..
        }
    )));
    assert!(events.iter().any(|e| matches!(e, GenEvent::Usage { .. })));
    assert_eq!(events.last(), Some(&GenEvent::Finished { ok: true }));

    // The prompt carried the candidates' text as data, and nothing of another course.
    let chat = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.url.path() == "/api/chat")
        .unwrap();
    let sent = String::from_utf8(chat.body).unwrap();
    assert!(sent.contains("course_material") && sent.contains("Classes begin"));
    assert!(!sent.contains("Stomata"));

    // The run is on record: its usage (free here) and its generation.
    let usage = app.usage_summary(None).unwrap();
    assert_eq!(usage.rows.len(), 1);
    assert_eq!(usage.rows[0].cost_basis, CostBasis::FreeOnDevice);
    let store = Store::open(&app.db_path()).unwrap();
    assert_eq!(
        store.generation("gen-1").unwrap().unwrap().status,
        GenerationStatus::Accepted
    );

    // The proposal waits for the student; accepting puts it in force.
    let view = app.course_calendar("DEMO101").unwrap();
    assert_eq!(
        (view.status, view.proposals.len()),
        (CalendarStatus::Proposed, 1)
    );
    let view = app.accept_calendar_proposal(proposal.id, None).unwrap();
    assert_eq!(view.status, CalendarStatus::Accepted);
    let accepted = view.accepted.unwrap();
    assert_eq!(accepted.origin, CalendarOrigin::Ai);
    assert!(accepted.ai_label.unwrap().on_device);
    let timeline = app.course_timeline("DEMO101").unwrap();
    assert_eq!(timeline.term.anchor_origin, Some(CalendarOrigin::Ai));
    assert!(timeline.term.week_one_monday.is_some());
}

#[tokio::test]
async fn an_answer_without_supported_dates_is_bad_output() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    let invented = json!({
        "stated_term": {"text": null, "quote": null, "source": null},
        "claims": [claim("first_class", day(-6), "Classes begin whenever you like.", "c1")],
        "weeks": [],
        "not_found": []
    });
    Mock::given(method("POST"))
        .respond_with(answer(&invented))
        .mount(&server)
        .await;
    let (events, on_event) = collect();
    let err = app
        .read_course_calendar("DEMO101", "gen-2", ReadCalendarOptions::default(), on_event)
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind, err.model_error),
        (AppErrorKind::Model, Some(ModelErrorKind::BadOutput))
    );
    assert_eq!(
        events.lock().unwrap().last(),
        Some(&GenEvent::Finished { ok: false })
    );
    let store = Store::open(&app.db_path()).unwrap();
    let run = store.generation("gen-2").unwrap().unwrap();
    assert_eq!(
        (run.status, run.error_kind.as_deref()),
        (GenerationStatus::Failed, Some("bad_output"))
    );
    assert!(app.course_calendar("DEMO101").unwrap().proposals.is_empty());
}

#[tokio::test]
async fn activity_lists_a_generation_until_it_ends() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    let read = |generation_id: &'static str| {
        let app = app.clone();
        tokio::spawn(async move {
            app.read_course_calendar(
                "DEMO101",
                generation_id,
                ReadCalendarOptions::default(),
                |_| {},
            )
            .await
        })
    };

    // Completed: listed while the model answers, gone once the proposal is stored.
    Mock::given(method("POST"))
        .respond_with(answer(&good_extraction()).set_delay(Duration::from_millis(1500)))
        .mount(&server)
        .await;
    let task = read("gen-done");
    wait_for_request(&server).await;
    assert_eq!(running_generations(&app), ["gen-done"]);
    task.await.unwrap().unwrap();
    assert!(app.activity().items.is_empty());

    // Failed: the answer is checked (and fails) before the run ends.
    reset_listing(&server).await;
    let invented = json!({
        "stated_term": {"text": null, "quote": null, "source": null},
        "claims": [claim("first_class", day(-6), "Classes begin whenever you like.", "c1")],
        "weeks": [],
        "not_found": []
    });
    Mock::given(method("POST"))
        .respond_with(answer(&invented).set_delay(Duration::from_millis(1500)))
        .mount(&server)
        .await;
    let task = read("gen-failed");
    wait_for_request(&server).await;
    assert_eq!(running_generations(&app), ["gen-failed"]);
    let err = task.await.unwrap().unwrap_err();
    assert_eq!(err.model_error, Some(ModelErrorKind::BadOutput));
    assert!(app.activity().items.is_empty());

    // Blocked by the gate: nothing left behind.
    let err = app
        .read_course_calendar(
            "DEMO505",
            "gen-blocked",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::CourseAiTurnedOff));
    assert!(app.activity().items.is_empty());

    // Cancelled: a second reading with the same id is busy, and the stop ends the run.
    reset_listing(&server).await;
    Mock::given(method("POST"))
        .respond_with(answer(&good_extraction()).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let task = read("gen-stopped");
    wait_for_request(&server).await;
    assert_eq!(running_generations(&app), ["gen-stopped"]);
    let busy = app
        .read_course_calendar(
            "DEMO101",
            "gen-stopped",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(busy.kind, AppErrorKind::Busy);
    assert_eq!(running_generations(&app), ["gen-stopped"]);
    app.cancel_generation("gen-stopped").unwrap();
    let err = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Cancelled);
    assert!(app.activity().items.is_empty());
}

#[tokio::test]
async fn changing_a_course_s_ai_settings_stops_its_reading() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    let read = |generation_id: &'static str| {
        let app = app.clone();
        tokio::spawn(async move {
            app.read_course_calendar(
                "DEMO101",
                generation_id,
                ReadCalendarOptions::default(),
                |_| {},
            )
            .await
        })
    };
    let slow = |delay: Duration| {
        let server = &server;
        async move {
            server.reset().await;
            Mock::given(method("GET"))
                .and(path("/api/tags"))
                .respond_with(tags(json!([{ "name": "local-model" }])))
                .mount(server)
                .await;
            Mock::given(method("POST"))
                .respond_with(answer(&good_extraction()).set_delay(delay))
                .mount(server)
                .await;
        }
    };
    // Another window turns the course's AI off, hides it or marks it prohibited while the
    // model is being asked: the reading stops.
    type Change = fn(&App);
    let changes: [(&'static str, Change, Change); 3] = [
        (
            "gen-off",
            |app| app.set_course_ai_access("DEMO101", false).unwrap(),
            |app| app.set_course_ai_access("DEMO101", true).unwrap(),
        ),
        (
            "gen-hidden",
            |app| app.set_course_hidden("DEMO101", true).unwrap(),
            |app| app.set_course_hidden("DEMO101", false).unwrap(),
        ),
        (
            "gen-prohibited",
            |app| {
                app.set_course_policy("DEMO101", AiPolicy::Prohibited, None)
                    .unwrap()
            },
            |app| {
                app.set_course_policy("DEMO101", AiPolicy::Unknown, None)
                    .unwrap()
            },
        ),
    ];
    for (id, change, undo) in changes {
        slow(Duration::from_secs(30)).await;
        let task = read(id);
        wait_for_request(&server).await;
        change(&app);
        let err = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::Cancelled, "{id}");
        undo(&app);
    }
    // Question (b) turning "not allowed" stops cloud runs only: this model is on this computer.
    slow(Duration::from_millis(1500)).await;
    let task = read("gen-local");
    wait_for_request(&server).await;
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    let proposal = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert!(proposal.is_ok(), "{proposal:?}");
}

#[tokio::test]
async fn a_batch_reads_course_by_course_and_can_be_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    // No model yet: each course says why it can't be read, its own reason first.
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
            Some(BlockReason::NoModelChosen),
            Some(BlockReason::CourseAiTurnedOff)
        ]
    );
    let kinds: Vec<String> = events
        .lock()
        .unwrap()
        .iter()
        .map(|e| {
            serde_json::to_value(e).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .filter(|kind| kind != "gen")
        .collect();
    assert_eq!(
        kinds,
        [
            "course_started",
            "course_finished",
            "course_started",
            "course_finished"
        ]
    );
    assert!(matches!(
        &events.lock().unwrap().last(),
        Some(CalendarBatchEvent::CourseFinished { outcome }) if outcome.course_id == course_id("505")
    ));

    // With a model: a slow answer, stopped by the batch's id.
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&good_extraction()).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let app2 = app.clone();
    let task = tokio::spawn(async move {
        app2.read_course_calendars(
            vec!["DEMO101".into(), "DEMO202".into()],
            "batch-2",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
    });
    wait_for_request(&server).await;
    // The batch is one item of `activity()`, its course's run part of it.
    assert_eq!(running_generations(&app), ["batch-2"]);
    app.cancel_generation("batch-2").unwrap();
    let outcomes = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(app.activity().items.is_empty());
    assert_eq!(outcomes.len(), 1, "the rest is not read");
    assert_eq!(outcomes[0].error, Some(AppErrorKind::Cancelled));
    let store = Store::open(&app.db_path()).unwrap();
    assert_eq!(
        store.generation("batch-2-0").unwrap().unwrap().status,
        GenerationStatus::Cancelled
    );
}

#[tokio::test]
async fn a_sync_scans_changed_outlines_into_proposals_once() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open_at_with_secrets(temp.path().join("data"), Arc::new(MemorySecrets::new()))
        .unwrap();
    let root = temp.path().join("Courses");
    let outline = root.join("DEMO707 Field Methods/Course outline.md");
    std::fs::create_dir_all(outline.parent().unwrap()).unwrap();
    let write = |extra: &str| {
        std::fs::write(
            &outline,
            format!(
                "# DEMO707 Course Outline\n\n{}\n{}\n{extra}\n",
                first_class_words(),
                last_class_words()
            ),
        )
        .unwrap()
    };
    write("");
    let source = app.add_folder_source(&root, None, None).unwrap();
    let sync = || app.sync_source(&source.id, pagelamp_app::SyncRequest::default(), |_| {});
    sync().await.unwrap();
    let view = app.course_calendar("DEMO707").unwrap();
    assert_eq!(view.proposals.len(), 1, "the sync scanned the outline");
    let first = view.proposals[0].id;
    assert_eq!(view.proposals[0].origin, CalendarOrigin::Scan);
    // The same materials again: nothing new, no churn.
    sync().await.unwrap();
    let view = app.course_calendar("DEMO707").unwrap();
    assert_eq!(
        view.proposals.iter().map(|p| p.id).collect::<Vec<_>>(),
        [first]
    );
    // Dismissed: not proposed again from the same text; a changed outline is.
    app.dismiss_calendar_proposal(first).unwrap();
    sync().await.unwrap();
    assert!(app.course_calendar("DEMO707").unwrap().proposals.is_empty());
    write("Office hours move to Thursdays.");
    sync().await.unwrap();
    assert_eq!(app.course_calendar("DEMO707").unwrap().proposals.len(), 1);
}

#[test]
fn what_a_proposal_changes_follows_the_calendar_in_force() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let proposal = app.scan_course_calendar("DEMO101").unwrap().unwrap();
    let codes = |view: &pagelamp_app::CourseCalendarView| -> Vec<String> {
        view.proposals[0]
            .changes
            .iter()
            .map(|c| c.code.as_str().to_string())
            .collect()
    };
    let view = app.course_calendar("DEMO101").unwrap();
    assert_eq!(view.proposals[0].id, proposal.id);
    assert!(codes(&view).contains(&"new_calendar".to_string()));
    // The student's own dates go in force a week later: the proposal now moves the first
    // class back.
    app.set_course_dates(
        "DEMO101",
        Some(pagelamp_app::CourseDatesInput {
            first_class: Some(day(-6)),
            last_class: Some(day(70)),
            exams_end: None,
            breaks: vec![],
            second_segment: None,
        }),
    )
    .unwrap();
    let view = app.course_calendar("DEMO101").unwrap();
    let codes = codes(&view);
    assert!(!codes.contains(&"new_calendar".to_string()), "{codes:?}");
    assert!(
        codes.contains(&"first_class_moved".to_string()),
        "{codes:?}"
    );
}

#[tokio::test]
async fn stored_proposals_are_found_by_id_only() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    assert_eq!(
        app.accept_calendar_proposal(7, None).unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert_eq!(
        app.dismiss_calendar_proposal(7).unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert!(app.accept_passing_proposals(Vec::new()).unwrap().is_empty());
    app.cancel_generation("nothing-running").unwrap();
}

/// Descriptions help the model but aren't part of the contract; `required` lists are sets.
fn contract(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .filter(|(key, _)| key.as_str() != "description")
                .map(|(key, child)| {
                    let child = if key == "required" {
                        let mut names: Vec<serde_json::Value> = child.as_array().unwrap().clone();
                        names.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                        serde_json::Value::Array(names)
                    } else {
                        contract(child)
                    };
                    (key.clone(), child)
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(contract).collect())
        }
        other => other.clone(),
    }
}

#[test]
fn the_model_answers_in_the_design_s_output_schema() {
    let spec = pagelamp_llm::OutputSpec::for_type::<
        pagelamp_core::calendar::extraction::CalendarExtraction,
    >("course_calendar")
    .unwrap();
    let pagelamp_llm::OutputSpec::Json { name, schema } = spec else {
        panic!("a JSON answer");
    };
    assert_eq!(name, "course_calendar");
    // docs/design/v0.3-course-calendar.md §7.4, verbatim.
    let design = json!({
      "type": "object", "additionalProperties": false,
      "required": ["stated_term", "claims", "weeks", "not_found"],
      "properties": {
        "stated_term": { "type": "object", "additionalProperties": false,
          "required": ["text", "quote", "source"],
          "properties": { "text": {"type": ["string","null"]}, "quote": {"type": ["string","null"]},
                          "source": {"type": ["string","null"]} } },
        "claims": { "type": "array", "items": { "type": "object", "additionalProperties": false,
          "required": ["kind", "date", "end_date", "label", "quote", "source"],
          "properties": {
            "kind": { "type": "string", "enum": ["first_class", "last_class", "break", "exam_period",
                                                 "final_exam", "term_start", "term_end"] },
            "date": {"type": "string"}, "end_date": {"type": ["string","null"]},
            "label": {"type": "string"}, "quote": {"type": "string"}, "source": {"type": "string"} } } },
        "weeks": { "type": "array", "items": { "type": "object", "additionalProperties": false,
          "required": ["week", "starts_on", "kind", "topic", "quote", "header_quote", "source"],
          "properties": {
            "week": {"type": "integer"}, "starts_on": {"type": ["string","null"]},
            "kind": {"type": "string", "enum": ["teaching", "break", "exam"]},
            "topic": {"type": ["string","null"]}, "quote": {"type": "string"},
            "header_quote": {"type": ["string","null"]}, "source": {"type": "string"} } } },
        "not_found": { "type": "array", "items": { "type": "string",
          "enum": ["first_class", "last_class", "breaks", "exam_period", "final_exam", "weeks"] } }
      }
    });
    assert_eq!(contract(&schema), contract(&design));
}

/// Dates copied from another course stay "copied" only until the course's own dates are saved
/// again: the dates form (saving or clearing it) and accepting a proposal end the mark and its
/// evidence line, like `set_course_term`.
#[test]
fn saving_or_accepting_a_calendar_ends_the_copied_mark() {
    use std::collections::BTreeMap;

    use pagelamp_core::term::COPIED_DATES_KEY;

    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let store = Store::open(&app.db_path()).unwrap();
    let mark = || {
        store
            .set_setting(
                COPIED_DATES_KEY,
                &BTreeMap::from([(course_id("101"), course_id("202"))]),
            )
            .unwrap();
    };
    let marked = || {
        store
            .setting::<BTreeMap<String, String>>(COPIED_DATES_KEY)
            .unwrap()
            .unwrap_or_default()
            .contains_key(&course_id("101"))
    };
    let says_copied = |app: &App| {
        app.course_timeline("DEMO101")
            .unwrap()
            .evidence_items
            .iter()
            .any(|item| item.code == "dates_copied_from")
    };
    let form = pagelamp_app::CourseDatesInput {
        first_class: Some(day(-13)),
        last_class: Some(day(70)),
        exams_end: None,
        breaks: vec![],
        second_segment: None,
    };

    mark();
    app.set_course_dates("DEMO101", Some(form.clone())).unwrap();
    assert!(!marked());
    assert!(!says_copied(&app));

    mark();
    app.set_course_dates("DEMO101", None).unwrap();
    assert!(!marked());

    // Dates as a copy leaves them (the store's write, confirmed, marked): the line shows.
    store
        .set_course_term(&course_id("101"), Some(day(-13)), Some(day(70)))
        .unwrap();
    store
        .set_setting(
            pagelamp_core::term::CONFIRMED_DATES_KEY,
            &std::collections::BTreeSet::from([course_id("101")]),
        )
        .unwrap();
    mark();
    assert!(says_copied(&app));
    let proposal = app
        .scan_course_calendar("DEMO101")
        .unwrap()
        .expect("dates in the syllabus");
    app.accept_passing_proposals(vec![proposal.id]).unwrap();
    assert!(!marked());
    assert!(!says_copied(&app));
}

/// A model that the server on this computer runs in the cloud (Ollama's `remote_host`) never
/// gets the material text of a course whose sharing answer is not-allowed: the run decides where
/// the model runs from the server's own list, right before it builds the context, and says why
/// it can't read. The same model run on this computer does read it.
#[tokio::test]
async fn a_cloud_model_through_a_local_server_never_gets_a_not_allowed_course() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let cloud = tags(json!([{ "name": "local-model", "remote_host": "https://ollama.com:443" }]));
    let server = with_local_model_listing(&app, cloud).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    let err = app
        .read_course_calendar(
            "DEMO101",
            "gen-cloud",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::MaterialSharingNotAllowed));
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.url.path() != "/api/chat"));

    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(tags(json!([{ "name": "local-model" }])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    app.read_course_calendar(
        "DEMO101",
        "gen-local",
        ReadCalendarOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
}

/// It fails closed: when the server on this computer can't give its model list, or doesn't list
/// the chosen model, the model counts as one in the cloud.
#[tokio::test]
async fn a_model_the_local_server_cant_vouch_for_counts_as_the_cloud() {
    for list in [
        ResponseTemplate::new(404),
        ResponseTemplate::new(200).set_body_string("not a model list"),
        tags(json!([{ "name": "another-model" }])),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let app = app_with_courses(temp.path());
        let server = with_local_model_listing(&app, list).await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(answer(&good_extraction()))
            .mount(&server)
            .await;
        app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
            .unwrap();
        let err = app
            .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.blocked, Some(BlockReason::MaterialSharingNotAllowed));
        let requests = server.received_requests().await.unwrap();
        assert!(requests.iter().all(|r| r.url.path() != "/api/chat"));
    }
}

/// The note for a model that a server here may run in the cloud, accepted.
async fn acknowledge_cloud_note(app: &App, backend: &BackendRef) {
    let (facts, acknowledged) = app.model_disclosure(backend, false).unwrap();
    assert!(!acknowledged && !facts.on_device);
    app.acknowledge_model_disclosure(backend, false, facts.version)
        .unwrap();
    assert!(app.model_disclosure(backend, false).unwrap().1);
}

/// Accepting the note for a server here covers the models it runs here only: a model it runs
/// in the cloud waits for its own note, whose facts aren't "on this computer" or free.
#[tokio::test]
async fn the_local_note_doesnt_cover_a_model_the_server_runs_in_the_cloud() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let list = tags(json!([
        { "name": "local-model" },
        { "name": "big-model", "remote_host": "https://ollama.com:443" },
    ]));
    let server = with_local_model_listing(&app, list).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    app.set_course_material_sharing("DEMO101", MaterialSharing::Allowed)
        .unwrap();
    app.read_course_calendar(
        "DEMO101",
        "gen-local",
        ReadCalendarOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
    let backend = BackendRef::Provider {
        provider_id: "ollama".into(),
    };
    app.set_feature_model(
        AiFeature::CourseCalendar,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "big-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let chats = |requests: &[wiremock::Request]| {
        requests
            .iter()
            .filter(|r| r.url.path() == "/api/chat")
            .count()
    };
    let before = chats(&server.received_requests().await.unwrap());
    let err = app
        .read_course_calendar(
            "DEMO101",
            "gen-cloud",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::DisclosureNotAcknowledged));
    assert_eq!(chats(&server.received_requests().await.unwrap()), before);
    // The status says so too: the model routed to it needs the cloud note (the reading kept
    // the daemon's list, which marks it remote).
    let status = |app: &App| {
        app.ai_status()
            .unwrap()
            .backends
            .into_iter()
            .find(|b| b.backend == backend)
            .unwrap()
            .state
    };
    assert_eq!(status(&app), BackendState::NeedsDisclosure);
    let (facts, _) = app.model_disclosure(&backend, false).unwrap();
    assert_eq!(facts.cost, CostKind::CloudViaLocal);
    assert_eq!(facts.retention, RetentionFact::ProviderTerms);
    let (local, acknowledged) = app.model_disclosure(&backend, true).unwrap();
    assert!(acknowledged && local.on_device);
    acknowledge_cloud_note(&app, &backend).await;
    assert_eq!(status(&app), BackendState::Ready);
    let err = app
        .read_course_calendar(
            "DEMO101",
            "gen-cloud",
            ReadCalendarOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::PriceUnknownNotAcknowledged));
}

/// A server here that isn't running (or doesn't answer in time) gives that error: the run
/// doesn't go on as if the model were in the cloud, and nothing is sent.
#[tokio::test]
async fn a_local_server_that_isnt_running_says_so() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    with_ollama_at(&app, &address);
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    let err = app
        .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.blocked, None);
    assert_eq!(err.model_error, Some(ModelErrorKind::Network));
}

/// Ollama lists a model pulled without a tag as `name:latest`: the same model.
#[tokio::test]
async fn a_model_listed_with_the_latest_tag_runs_here() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server =
        with_local_model_listing(&app, tags(json!([{ "name": "local-model:latest" }]))).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    let proposal = app
        .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap();
    assert!(proposal.ai_label.unwrap().on_device);
}

/// A model list that arrives after its provider was removed isn't kept: `ai remove` takes the
/// list, and an answer still on its way doesn't write it back.
#[tokio::test]
async fn a_list_that_arrives_after_its_provider_went_isn_t_kept() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let slow = tags(json!([{ "name": "local-model" }])).set_delay(Duration::from_millis(1500));
    let server = with_local_model_listing(&app, slow).await;
    let backend = BackendRef::Provider {
        provider_id: "ollama".into(),
    };
    let listing = {
        let (app, backend) = (app.clone(), backend.clone());
        tokio::spawn(async move { app.list_models(&backend).await })
    };
    while server.received_requests().await.unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    app.remove_model_provider("ollama").unwrap();
    listing.await.unwrap().unwrap();
    let kept: Option<serde_json::Value> = Store::open(&app.db_path())
        .unwrap()
        .setting("ai.listed_models")
        .unwrap();
    assert!(
        kept.as_ref()
            .is_none_or(|lists| lists.get("provider:ollama").is_none()),
        "{kept:?}"
    );
}

/// The one reminder for a course whose sharing answer is open follows the first reading that
/// sent its materials to the cloud, also one that failed or whose answer had no checkable
/// dates: a later reading doesn't remind again.
#[tokio::test]
async fn a_cloud_reading_that_fails_after_sending_still_reminds() {
    let unsupported = json!({
        "stated_term": {"text": null, "quote": null, "source": null},
        "claims": [claim("first_class", day(-6), "Classes begin whenever you like.", "c1")],
        "weeks": [],
        "not_found": []
    });
    let rejected = ResponseTemplate::new(401).set_body_json(json!({"error": "unauthorized"}));
    for failure in [rejected, answer(&unsupported)] {
        let temp = tempfile::tempdir().unwrap();
        let app = app_with_courses(temp.path());
        let cloud =
            || tags(json!([{ "name": "local-model", "remote_host": "https://ollama.com:443" }]));
        let server = with_local_model_listing(&app, cloud()).await;
        let backend = BackendRef::Provider {
            provider_id: "ollama".into(),
        };
        acknowledge_cloud_note(&app, &backend).await;
        app.acknowledge_unpriced_model(&backend, "local-model")
            .unwrap();
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(failure)
            .mount(&server)
            .await;
        let (events, on_event) = collect();
        app.read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), on_event)
            .await
            .unwrap_err();
        assert!(events.lock().unwrap().iter().any(|e| matches!(
            e,
            GenEvent::Notice {
                code: GenNoticeCode::MaterialSharingReminder
            }
        )));

        server.reset().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(cloud())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(answer(&good_extraction()))
            .mount(&server)
            .await;
        let proposal = app
            .read_course_calendar("DEMO101", "gen-2", ReadCalendarOptions::default(), |_| {})
            .await
            .unwrap();
        assert!(!proposal.sharing_reminder, "reminded once already");
    }
}

/// Usage lines older than 13 months go when a run is recorded and at each sync, and so do
/// proposals dismissed more than 30 days ago, with the runs that wrote them.
#[tokio::test]
async fn old_ai_records_go_at_a_run_and_at_a_sync() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    // Around 13 calendar months back: older lines go, newer ones stay.
    let thirteen_months = Utc::now()
        .checked_sub_months(chrono::Months::new(13))
        .unwrap();
    let line = |past: bool| UsageRecord {
        at: thirteen_months + TimeDelta::days(if past { -2 } else { 2 }),
        backend: "provider:ollama".into(),
        model: "local-model".into(),
        feature: AiFeature::CourseCalendar,
        input_uncached: 900,
        cache_read: 0,
        cache_write: 0,
        output: 80,
        reasoning: None,
        micro_usd: Some(0),
        cost_basis: "free_on_device".into(),
        estimated: false,
        outcome: "ok".into(),
    };
    let lines = |store: &Store| {
        store
            .ai_usage_between(
                Utc::now() - TimeDelta::days(1000),
                Utc::now() + TimeDelta::days(1),
            )
            .unwrap()
            .len()
    };
    let store = Store::open(&app.db_path()).unwrap();
    store.record_ai_usage(&line(true)).unwrap();
    store.record_ai_usage(&line(false)).unwrap();
    let proposal = app
        .read_course_calendar("DEMO101", "gen-1", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap();
    // The line just inside 13 months and the run's own.
    assert_eq!(lines(&store), 2);

    store
        .dismiss_calendar_proposal(proposal.id, Utc::now() - TimeDelta::days(40))
        .unwrap();
    store.record_ai_usage(&line(true)).unwrap();
    let root = temp.path().join("Courses");
    let course = root.join("DEMO909 Field Methods");
    std::fs::create_dir_all(&course).unwrap();
    std::fs::write(course.join("notes.md"), "# Notes\n\nSampling notes.\n").unwrap();
    let source = app.add_folder_source(&root, None, None).unwrap();
    app.sync_source(&source.id, pagelamp_app::SyncRequest::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(lines(&store), 2);
    assert!(store.calendar_row(proposal.id).unwrap().is_none());
    assert!(store.generation("gen-1").unwrap().is_none());
}

/// "Remove all AI data" stops this process's readings, and a reading still on its way keeps
/// nothing: no usage line, no run, no proposal.
#[tokio::test]
async fn removing_all_ai_data_stops_a_reading_and_keeps_nothing_of_it() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    // An answer that would take long: a stopped reading doesn't wait for it.
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let read = |app: &App, id: &'static str| {
        let app = app.clone();
        tokio::spawn(async move {
            app.read_course_calendar("DEMO101", id, ReadCalendarOptions::default(), |_| {})
                .await
        })
    };
    let kept = |app: &App, id: &str| {
        let store = Store::open(&app.db_path()).unwrap();
        let usage = store
            .ai_usage_between(
                Utc::now() - TimeDelta::days(1),
                Utc::now() + TimeDelta::days(1),
            )
            .unwrap()
            .len();
        let run = store.generation(id).unwrap().is_some();
        let proposals = app.course_calendar("DEMO101").unwrap().proposals.len();
        (usage, run, proposals)
    };

    // In this process: the reading is stopped.
    let reading = read(&app, "gen-here");
    wait_for_request(&server).await;
    let removed_at = std::time::Instant::now();
    app.remove_all_ai_data().unwrap();
    let err = reading.await.unwrap().unwrap_err();
    assert!(
        removed_at.elapsed() < Duration::from_secs(5),
        "the reading wasn't stopped"
    );
    assert_eq!(err.kind, AppErrorKind::Cancelled);
    assert_eq!(kept(&app, "gen-here"), (0, false, 0));

    // From another process (another App on the same data): the reading runs to its end and
    // keeps nothing, as its provider is gone.
    let app = app_with_courses_again(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()).set_delay(Duration::from_millis(1500)))
        .mount(&server)
        .await;
    let reading = read(&app, "gen-there");
    wait_for_request(&server).await;
    let other = App::open_at_with_secrets(temp.path().join("data"), Arc::new(MemorySecrets::new()))
        .unwrap();
    other.remove_all_ai_data().unwrap();
    let err = reading.await.unwrap().unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Cancelled, "{}", err.message);
    assert_eq!(kept(&app, "gen-there"), (0, false, 0));
}

/// The same data folder opened again (its courses are there already).
fn app_with_courses_again(dir: &Path) -> App {
    App::open_at_with_secrets(dir.join("data"), Arc::new(MemorySecrets::new())).unwrap()
}

/// A run of a model that the server on this computer runs in the cloud says so: its start, its
/// saved label and its usage row aren't "on this computer" or free (the price list doesn't know
/// such a model, so its cost is unknown).
#[tokio::test]
async fn a_cloud_model_through_a_local_server_is_labelled_and_billed_as_the_cloud() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let cloud = tags(json!([{ "name": "local-model", "remote_host": "https://ollama.com:443" }]));
    let server = with_local_model_listing(&app, cloud).await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(answer(&good_extraction()))
        .mount(&server)
        .await;
    app.set_course_material_sharing("DEMO101", MaterialSharing::Allowed)
        .unwrap();
    let backend = BackendRef::Provider {
        provider_id: "ollama".into(),
    };
    acknowledge_cloud_note(&app, &backend).await;
    // Its price is unknown, so the run waits for the student's acknowledgement first.
    let err = app
        .read_course_calendar("DEMO101", "gen-ack", ReadCalendarOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::PriceUnknownNotAcknowledged));
    app.acknowledge_unpriced_model(&backend, "local-model")
        .unwrap();
    let (events, on_event) = collect();
    let proposal = app
        .read_course_calendar(
            "DEMO101",
            "gen-cloud",
            ReadCalendarOptions::default(),
            on_event,
        )
        .await
        .unwrap();
    let events = events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        GenEvent::Started {
            on_device: false,
            ..
        }
    )));
    assert!(!proposal.ai_label.unwrap().on_device);
    let usage = app.usage_summary(None).unwrap();
    assert_eq!(usage.rows[0].cost_basis, CostBasis::Unpriced);
}
