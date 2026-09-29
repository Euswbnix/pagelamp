//! Study plans PageLamp writes itself (model-access design §5.1) through the facade: a draft
//! from a local mock model, the scheduler's dates, the graded-work filter, accepting and
//! ticking items, and the policy golden (structure only; hidden and inactive courses left out).
//! Synthetic data only.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{Local, NaiveDate, SubsecRound, TimeDelta, Utc};
use pagelamp_app::ai::{BackendRef, GenEvent, ModelChoice, PlanWarningCode, StudyPlanRequest};
use pagelamp_app::{ActivityKind, App, AppErrorKind, DayOfWeek};
use pagelamp_core::ai::{AiFeature, BlockReason, Effort, ModelErrorKind, ProviderRow};
use pagelamp_core::model::*;
use pagelamp_core::secrets::MemorySecrets;
use pagelamp_core::store::{GenerationStatus, Store};
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const SOURCE: &str = "canvas:lms.example.edu";

fn course_id(external: &str) -> String {
    format!("{SOURCE}/course/{external}")
}

fn day(offset: i64) -> NaiveDate {
    Local::now().date_naive() + TimeDelta::days(offset)
}

/// DEMO101 readable, DEMO202 prohibited, DEMO303 turned off, DEMO404 hidden (all teaching
/// now), DEMO505 ended long ago. Each has a week-3 material whose text holds a secret word.
fn app_with_courses(dir: &std::path::Path) -> App {
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
    for (external, start, end) in [
        ("101", -14, 90),
        ("202", -14, 90),
        ("303", -14, 90),
        ("404", -14, 90),
        ("505", -400, -300),
    ] {
        let id = course_id(external);
        store
            .upsert_course(&CourseUpsert {
                id: id.clone(),
                source_id: SOURCE.into(),
                external_id: external.into(),
                code: Some(format!("DEMO{external}")),
                name: format!("Demo course {external}"),
                term_start: Some(day(start)),
                term_end: Some(day(end)),
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
        let material = format!("{SOURCE}/file/{external}");
        store
            .upsert_material(&MaterialUpsert {
                id: material.clone(),
                course_id: id.clone(),
                module_id: None,
                kind: MaterialKind::File,
                title: format!("Week 3 slides {external}"),
                url: None,
                local_path: None,
                mime: None,
                published_at: None,
                week_hint: Some(3),
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
                    text: format!("Stomata secret {external}"),
                }],
            )
            .unwrap();
    }
    store
        .replace_events(
            SOURCE,
            &[Event {
                id: format!("{SOURCE}/assignment/1"),
                source_id: SOURCE.into(),
                course_id: Some(course_id("202")),
                kind: EventKind::AssignmentDue,
                title: "A2".into(),
                starts_at: None,
                ends_at: None,
                due_at: Some(Utc::now() + TimeDelta::days(5)),
                url: None,
                updated_at: Utc::now(),
                course_hint: None,
            }],
        )
        .unwrap();
    drop(store);
    app.set_course_policy("DEMO202", AiPolicy::Prohibited, None)
        .unwrap();
    app.set_course_ai_access("DEMO303", false).unwrap();
    app.set_course_hidden("DEMO404", true).unwrap();
    app
}

/// A model on this computer (an Ollama mock), chosen for study plans and disclosed.
async fn with_local_model(app: &App) -> MockServer {
    let server = MockServer::start().await;
    Store::open(&app.db_path())
        .unwrap()
        .insert_model_provider(&ProviderRow {
            id: "ollama".into(),
            preset: "ollama".into(),
            label: "Ollama".into(),
            wire: "ollama_native".into(),
            base_url: server.uri(),
            created_at: Utc::now().trunc_subsecs(0),
            last_probe_json: None,
        })
        .unwrap();
    let backend = BackendRef::Provider {
        provider_id: "ollama".into(),
    };
    app.set_feature_model(
        AiFeature::StudyPlan,
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
    server
}

/// Ollama's streamed answer carrying `answer` as the JSON text.
fn answer(answer: &serde_json::Value) -> ResponseTemplate {
    let lines = [
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:00Z",
               "message": {"role": "assistant", "content": answer.to_string()}, "done": false}),
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:01Z",
               "message": {"role": "assistant", "content": ""}, "done": true,
               "done_reason": "stop", "prompt_eval_count": 700, "eval_count": 90}),
    ];
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/x-ndjson")
        .set_body_string(body)
}

fn task(
    course: Option<&str>,
    kind: &str,
    title: &str,
    materials: &[&str],
    latest: i64,
) -> serde_json::Value {
    json!({
        "course_id": course.map(course_id),
        "kind": kind,
        "title": title,
        "description": null,
        "material_ids": materials,
        "minutes": 60,
        "priority": "normal",
        "earliest": null,
        "latest": day(latest).to_string(),
    })
}

fn tasks() -> serde_json::Value {
    json!({"tasks": [
        task(Some("101"), "read", "Re-read the week 3 slides", &[&format!("{SOURCE}/file/101"), "made-up"], 5),
        task(Some("202"), "prepare_deadline", "Write A2's answers", &[], 4),
        task(Some("202"), "review", "Review week 3 before A2", &[], 4),
        task(None, "review", "Review this week's notes", &[], 6),
    ]})
}

fn collect() -> (Arc<Mutex<Vec<GenEvent>>>, impl Fn(GenEvent) + Send + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    (events, move |event| sink.lock().unwrap().push(event))
}

#[tokio::test]
async fn a_draft_is_scheduled_filtered_accepted_and_ticked() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&tasks()))
        .mount(&server)
        .await;
    let (events, on_event) = collect();
    let draft = app
        .generate_study_plan(
            StudyPlanRequest {
                horizon_days: Some(7),
                hours_per_week: Some(14),
                days_off: vec![DayOfWeek::Sunday],
                note: Some("Focus on the A2 topics".into()),
                ..StudyPlanRequest::default()
            },
            "plan-1",
            on_event,
        )
        .await
        .unwrap();

    // The scheduler dated every task inside the horizon; the graded one is gone.
    let titles: Vec<&str> = draft.plan.items.iter().map(|i| i.title.as_str()).collect();
    assert!(titles.contains(&"Re-read the week 3 slides"), "{titles:?}");
    assert!(!titles.iter().any(|t| t.contains("answers")), "{titles:?}");
    let today = Local::now().date_naive();
    assert_eq!(
        (draft.plan.horizon_start, draft.plan.horizon_end),
        (today, today + TimeDelta::days(6))
    );
    for item in &draft.plan.items {
        assert!(today <= item.date && item.date <= draft.plan.horizon_end);
        assert_ne!(chrono::Datelike::weekday(&item.date), chrono::Weekday::Sun);
    }
    let reread = draft
        .plan
        .items
        .iter()
        .find(|i| i.title == "Re-read the week 3 slides")
        .unwrap();
    assert_eq!(reread.material_ids, [format!("{SOURCE}/file/101")]);
    let warnings: Vec<(PlanWarningCode, u32)> =
        draft.warnings.iter().map(|w| (w.code, w.count)).collect();
    assert_eq!(
        warnings,
        [
            (PlanWarningCode::GradedWorkLeftOut, 1),
            (PlanWarningCode::UnknownMaterialsDropped, 1)
        ]
    );
    assert_eq!(draft.meta.backend_label, "Ollama");
    assert_eq!(draft.meta.usage.output_tokens, 90);
    assert_eq!(
        draft.meta.est_cost_micro_usd,
        Some(0),
        "free on this computer"
    );
    let mut planned: Vec<&str> = draft
        .meta
        .context
        .courses
        .iter()
        .map(|c| c.course_id.as_str())
        .collect();
    planned.sort_unstable();
    assert_eq!(
        planned,
        [course_id("101"), course_id("202"), course_id("303")]
    );
    let events = events.lock().unwrap().clone();
    assert!(events.contains(&GenEvent::Stage {
        stage: pagelamp_app::ai::GenStage::Scheduling
    }));
    assert_eq!(events.last(), Some(&GenEvent::Finished { ok: true }));

    // Policy golden: what was sent is structure only, hidden and ended courses left out, and
    // the student's note travels as data.
    let sent =
        String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert!(!sent.contains("Stomata"), "no material text in a plan run");
    for code in ["DEMO101", "DEMO202", "DEMO303"] {
        assert!(sent.contains(code), "{code} is planned (structure only)");
    }
    for code in ["DEMO404", "DEMO505"] {
        assert!(!sent.contains(code), "{code} is left out");
    }
    assert!(sent.contains("Focus on the A2 topics"));

    // A draft, until the student keeps it.
    let store = Store::open(&app.db_path()).unwrap();
    assert_eq!(
        store.generation("plan-1").unwrap().unwrap().status,
        GenerationStatus::Draft
    );
    assert!(app.latest_study_plan().unwrap().is_none());
    let stored = app.accept_study_plan("plan-1").unwrap();
    assert_eq!(stored.origin, PlanOrigin::PageLamp);
    assert_eq!(stored.generation_id.as_deref(), Some("plan-1"));
    assert_eq!(stored.plan.items.len(), draft.plan.items.len());
    assert_eq!(
        store.generation("plan-1").unwrap().unwrap().status,
        GenerationStatus::Accepted
    );
    let again = app.accept_study_plan("plan-1").unwrap_err();
    assert_eq!(again.kind, AppErrorKind::Invalid);
    assert_eq!(
        app.accept_study_plan("plan-none").unwrap_err().kind,
        AppErrorKind::NotFound
    );

    // Ticking items (what the weekly progress reminder counts).
    let ticked = app.set_study_plan_item_done(stored.id, 0, true).unwrap();
    assert!(ticked.plan.items[0].done);
    assert!(app.latest_study_plan().unwrap().unwrap().plan.items[0].done);
    let err = app
        .set_study_plan_item_done(stored.id, 99, true)
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::NotFound);
}

#[tokio::test]
async fn ticking_counts_items_as_readers_see_them() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let today = Local::now().date_naive();
    let item = |course: &str, title: &str| StudyPlanItem {
        date: today,
        course_id: Some(course_id(course)),
        title: title.into(),
        description: None,
        material_ids: Vec::new(),
        minutes: Some(30),
        done: false,
    };
    let stored = Store::open(&app.db_path())
        .unwrap()
        .save_study_plan(&StudyPlan {
            horizon_start: today,
            horizon_end: today + TimeDelta::days(3),
            items: vec![item("303", "Removed course's item"), item("101", "Read")],
            notes: None,
        })
        .unwrap();
    assert_eq!(stored.origin, PlanOrigin::AiApp);
    let rt_remove = app
        .remove_courses(
            vec!["DEMO303".into()],
            pagelamp_app::RemoveOptions {
                reason: None,
                keep_downloaded_files: false,
                purge_now: false,
                delete_pre_update_backup: false,
            },
        )
        .await;
    rt_remove.unwrap();
    // Item 0 is "Read": the removed course's item isn't shown, so it isn't counted.
    let ticked = app.set_study_plan_item_done(stored.id, 0, true).unwrap();
    assert_eq!(ticked.plan.items.len(), 1);
    assert_eq!(ticked.plan.items[0].title, "Read");
    assert!(ticked.plan.items[0].done);
}

#[tokio::test]
async fn requests_are_checked_and_a_run_needs_a_model() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    for request in [
        StudyPlanRequest {
            horizon_days: Some(0),
            ..StudyPlanRequest::default()
        },
        StudyPlanRequest {
            horizon_days: Some(57),
            ..StudyPlanRequest::default()
        },
        StudyPlanRequest {
            hours_per_week: Some(0),
            ..StudyPlanRequest::default()
        },
        StudyPlanRequest {
            days_off: vec![
                DayOfWeek::Monday,
                DayOfWeek::Tuesday,
                DayOfWeek::Wednesday,
                DayOfWeek::Thursday,
                DayOfWeek::Friday,
                DayOfWeek::Saturday,
                DayOfWeek::Sunday,
            ],
            ..StudyPlanRequest::default()
        },
    ] {
        let err = app
            .generate_study_plan(request.clone(), "plan-x", |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::Invalid, "{request:?}");
    }
    let (events, on_event) = collect();
    let err = app
        .generate_study_plan(StudyPlanRequest::default(), "plan-x", on_event)
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::NoModelChosen));
    assert_eq!(
        events.lock().unwrap().last(),
        Some(&GenEvent::Finished { ok: false })
    );
}

#[tokio::test]
async fn an_answer_that_isn_t_tasks_is_bad_output() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&json!({"plan": "read everything"})))
        .mount(&server)
        .await;
    let err = app
        .generate_study_plan(StudyPlanRequest::default(), "plan-bad", |_| {})
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind, err.model_error),
        (AppErrorKind::Model, Some(ModelErrorKind::BadOutput))
    );
    let row = Store::open(&app.db_path())
        .unwrap()
        .generation("plan-bad")
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.status, row.error_kind.as_deref()),
        (GenerationStatus::Failed, Some("bad_output"))
    );
}

#[tokio::test]
async fn a_running_plan_is_listed_and_can_be_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let app = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&tasks()).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let task = {
        let app = app.clone();
        tokio::spawn(async move {
            app.generate_study_plan(StudyPlanRequest::default(), "plan-stop", |_| {})
                .await
        })
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while server.received_requests().await.unwrap().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the run never started"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let items = app.activity().items;
    assert_eq!(items.len(), 1);
    assert_eq!(
        (items[0].kind, items[0].generation_id.as_deref()),
        (ActivityKind::Generation, Some("plan-stop"))
    );
    app.cancel_generation("plan-stop").unwrap();
    let err = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(err.kind, AppErrorKind::Cancelled);
    assert!(app.activity().items.is_empty());
    assert_eq!(
        Store::open(&app.db_path())
            .unwrap()
            .generation("plan-stop")
            .unwrap()
            .unwrap()
            .status,
        GenerationStatus::Cancelled
    );
}
