//! Weekly explanations (model-access design §5.2) through the facade: a grounded explanation
//! from a local mock model (citations resolved, unknown handles and uncited paragraphs
//! dropped), the left-out list and "include", the output language, saved explanations and
//! staleness, the gate matrix with question (b), and bad output. Synthetic data only; no run
//! ever leaves this computer (a cloud backend is only used where the gate blocks first).

use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDate, SubsecRound, TimeDelta, Utc};
use pagelamp_app::ai::{
    BackendRef, ExplainOptions, GenEvent, GenStage, ModelChoice, OutputLanguage,
};
use pagelamp_app::{App, AppErrorKind};
use pagelamp_core::ai::{
    AiFeature, BlockReason, Effort, MaterialSharing, ModelErrorKind, ProviderRow,
};
use pagelamp_core::ai_gate::LeftOutReason;
use pagelamp_core::model::*;
use pagelamp_core::secrets::{MemorySecrets, SecretBackend};
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

fn material(store: &Store, external: &str, name: &str, title: &str, text: &str) -> String {
    let id = format!("{SOURCE}/file/{external}-{name}");
    store
        .upsert_material(&MaterialUpsert {
            id: id.clone(),
            course_id: course_id(external),
            module_id: None,
            kind: MaterialKind::File,
            title: title.into(),
            url: Some(format!("https://lms.example.edu/files/{name}")),
            local_path: None,
            mime: None,
            published_at: None,
            week_hint: Some(3),
        })
        .unwrap();
    if !text.is_empty() {
        store
            .set_text_state(&id, TextStatus::Ok, None, Some("hash-1"))
            .unwrap();
        store
            .replace_chunks(
                &id,
                &[Chunk {
                    material_id: id.clone(),
                    ord: 0,
                    locator: Some("slide 2".into()),
                    text: text.into(),
                }],
            )
            .unwrap();
    }
    id
}

/// Every course teaches now; week 3 is the one explained. DEMO101 readable (slides and an
/// assignment sheet), DEMO202 prohibited, DEMO303 turned off, DEMO404 hidden, DEMO505 readable
/// and "not allowed" (question (b)), DEMO606 readable without text.
fn app_with_courses(dir: &std::path::Path) -> (App, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::new());
    let app = App::open_at_with_secrets(dir.join("data"), secrets.clone()).unwrap();
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
    for external in ["101", "202", "303", "404", "505", "606"] {
        store
            .upsert_course(&CourseUpsert {
                id: course_id(external),
                source_id: SOURCE.into(),
                external_id: external.into(),
                code: Some(format!("DEMO{external}")),
                name: format!("Demo course {external}"),
                term_start: Some(day(-14)),
                term_end: Some(day(90)),
                url: None,
                syllabus_text: None,
                lms: Default::default(),
            })
            .unwrap();
    }
    material(
        &store,
        "101",
        "slides",
        "Week 3 slides",
        "Stomata open in light.",
    );
    material(
        &store,
        "101",
        "a2",
        "Assignment 2",
        "Question 1: explain guard cells.",
    );
    for external in ["202", "303", "404", "505"] {
        material(
            &store,
            external,
            "slides",
            "Week 3 slides",
            &format!("Canary {external} text."),
        );
    }
    material(&store, "606", "slides", "Week 3 slides", "");
    drop(store);
    app.set_course_policy("DEMO202", AiPolicy::Prohibited, None)
        .unwrap();
    app.set_course_ai_access("DEMO303", false).unwrap();
    app.set_course_hidden("DEMO404", true).unwrap();
    app.set_course_material_sharing("DEMO505", MaterialSharing::NotAllowed)
        .unwrap();
    (app, secrets)
}

/// A model on this computer (an Ollama mock), chosen for explanations and disclosed.
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
    choose(app, backend.clone());
    acknowledge(app, &backend);
    server
}

fn choose(app: &App, backend: BackendRef) {
    app.set_feature_model(
        AiFeature::WeeklyExplanation,
        Some(ModelChoice {
            backend,
            model: "local-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
}

fn acknowledge(app: &App, backend: &BackendRef) {
    let version = app
        .ai_status()
        .unwrap()
        .backends
        .iter()
        .find(|b| &b.backend == backend)
        .unwrap()
        .disclosure
        .version;
    app.acknowledge_ai_disclosure(backend, version).unwrap();
}

fn answer(answer: &serde_json::Value) -> ResponseTemplate {
    let lines = [
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:00Z",
               "message": {"role": "assistant", "content": answer.to_string()}, "done": false}),
        json!({"model": "local-model", "created_at": "2026-09-29T10:00:01Z",
               "message": {"role": "assistant", "content": ""}, "done": true,
               "done_reason": "stop", "prompt_eval_count": 500, "eval_count": 120}),
    ];
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/x-ndjson")
        .set_body_string(body)
}

fn explanation() -> serde_json::Value {
    json!({
        "sections": [
            {"heading": "Stomata", "paragraphs": [
                {"text": "Stomata open in light.", "citations": ["c1", "c7"]},
                {"text": "An invented fact.", "citations": ["c9"]},
                {"text": "Guard cells swell.", "citations": ["c1", "c1"]}
            ]},
            {"heading": "Uncited", "paragraphs": [
                {"text": "Nothing to back this.", "citations": []}
            ]}
        ],
        "check_questions": ["Why do stomata open?", "What do guard cells do?", " ", "Q3?", "Q4?"]
    })
}

fn collect() -> (Arc<Mutex<Vec<GenEvent>>>, impl Fn(GenEvent) + Send + Sync) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    (events, move |event| sink.lock().unwrap().push(event))
}

async fn sent(server: &MockServer, n: usize) -> String {
    String::from_utf8(server.received_requests().await.unwrap()[n].body.clone()).unwrap()
}

#[tokio::test]
async fn an_explanation_is_grounded_in_the_week_s_materials() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&explanation()))
        .mount(&server)
        .await;
    let (events, on_event) = collect();
    let result = app
        .explain_week(
            "DEMO101",
            Some(3),
            "explain-1",
            ExplainOptions {
                ui_language: Some("zh-CN".into()),
                ..ExplainOptions::default()
            },
            on_event,
        )
        .await
        .unwrap();

    // Every paragraph cites a material of the week; made-up handles and uncited paragraphs go.
    assert_eq!(result.week, Some(3));
    let paragraphs: Vec<(&str, Vec<&str>)> = result
        .sections
        .iter()
        .flat_map(|s| &s.paragraphs)
        .map(|p| {
            (
                p.text.as_str(),
                p.citations.iter().map(|c| c.handle.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        paragraphs,
        [
            ("Stomata open in light.", vec!["c1"]),
            ("Guard cells swell.", vec!["c1"])
        ]
    );
    assert_eq!(
        result.sections.len(),
        1,
        "a section left without paragraphs goes"
    );
    let citation = &result.sections[0].paragraphs[0].citations[0];
    assert_eq!(
        (
            citation.material_id.as_str(),
            citation.title.as_str(),
            citation.locator.as_deref(),
            citation.url.as_deref()
        ),
        (
            format!("{SOURCE}/file/101-slides").as_str(),
            "Week 3 slides",
            Some("slide 2"),
            Some("https://lms.example.edu/files/slides")
        )
    );
    assert_eq!(result.dropped_citations, 2);
    assert_eq!(
        result.check_questions.len(),
        3,
        "2–3 questions, empty ones dropped"
    );
    assert_eq!(result.left_out.len(), 1);
    assert_eq!(
        (result.left_out[0].title.as_str(), result.left_out[0].reason),
        ("Assignment 2", LeftOutReason::LooksLikeAssessment)
    );
    assert!(!result.stale && !result.sharing_reminder && !result.cite_ai_use);
    assert_eq!(result.meta.backend_label, "Ollama");

    // The stages the Explain tab shows.
    let events = events.lock().unwrap().clone();
    let context = events.iter().find_map(|e| match e {
        GenEvent::Context {
            summary,
            input_tokens,
        } => Some((summary.materials_included, *input_tokens)),
        _ => None,
    });
    assert!(
        matches!(context, Some((1, Some(tokens))) if tokens > 0),
        "{context:?}"
    );
    assert!(events.contains(&GenEvent::Stage {
        stage: GenStage::Validating
    }));
    assert_eq!(events.last(), Some(&GenEvent::Finished { ok: true }));

    // Policy golden: only the readable course's week text, the assignment sheet left out,
    // the answer's language as fixed wording.
    let body = sent(&server, 0).await;
    assert!(body.contains("Stomata open in light."));
    assert!(
        !body.contains("guard cells"),
        "the left-out assignment sheet"
    );
    for external in ["202", "303", "404", "505"] {
        assert!(!body.contains(&format!("Canary {external}")), "{external}");
    }
    assert!(body.contains("Simplified Chinese"));

    // "Include" sends the assignment sheet next time; the course language setting.
    app.set_ai_output_language(OutputLanguage::Course).unwrap();
    assert_eq!(app.ai_output_language().unwrap(), OutputLanguage::Course);
    let second = app
        .explain_week(
            "DEMO101",
            None,
            "explain-2",
            ExplainOptions {
                include: vec![format!("{SOURCE}/file/101-a2")],
                ..ExplainOptions::default()
            },
            |_| {},
        )
        .await
        .unwrap();
    assert!(second.left_out.is_empty());
    let body = sent(&server, 1).await;
    assert!(body.contains("guard cells"));
    assert!(body.contains("the language the course materials are written in"));
}

#[tokio::test]
async fn saved_explanations_turn_stale_when_the_week_changes() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&explanation()))
        .mount(&server)
        .await;
    app.explain_week(
        "DEMO101",
        Some(3),
        "explain-1",
        ExplainOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
    app.explain_week(
        "DEMO101",
        Some(3),
        "explain-2",
        ExplainOptions::default(),
        |_| {},
    )
    .await
    .unwrap();
    let saved = app.saved_explanations("DEMO101", Some(3)).unwrap();
    assert_eq!(
        saved
            .iter()
            .map(|e| (e.meta.generation_id.as_str(), e.stale))
            .collect::<Vec<_>>(),
        [("explain-2", false), ("explain-1", false)]
    );
    assert_eq!(app.saved_explanations("DEMO101", None).unwrap().len(), 2);
    assert!(
        app.saved_explanations("DEMO101", Some(4))
            .unwrap()
            .is_empty()
    );

    // The slides changed.
    let store = Store::open(&app.db_path()).unwrap();
    store
        .set_text_state(
            &format!("{SOURCE}/file/101-slides"),
            TextStatus::Ok,
            None,
            Some("hash-2"),
        )
        .unwrap();
    assert!(app.saved_explanations("DEMO101", Some(3)).unwrap()[0].stale);
    store
        .set_text_state(
            &format!("{SOURCE}/file/101-slides"),
            TextStatus::Ok,
            None,
            Some("hash-1"),
        )
        .unwrap();
    assert!(!app.saved_explanations("DEMO101", Some(3)).unwrap()[0].stale);
    // A new material that week.
    material(&store, "101", "notes", "Week 3 notes", "More notes.");
    assert!(app.saved_explanations("DEMO101", Some(3)).unwrap()[0].stale);
}

/// The gate matrix (design §4.1), question (b) included (D37 option 2).
#[tokio::test]
async fn explanations_are_blocked_for_the_course_s_reason() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_with_courses(temp.path());
    let err = app
        .explain_week("DEMO101", Some(3), "x", ExplainOptions::default(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::NoModelChosen));
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&explanation()))
        .mount(&server)
        .await;
    for (course, reason) in [
        ("DEMO202", BlockReason::CoursePolicyProhibited),
        ("DEMO303", BlockReason::CourseAiTurnedOff),
        ("DEMO404", BlockReason::CourseHidden),
        ("DEMO606", BlockReason::NoReadableMaterials),
    ] {
        let err = app
            .explain_week(course, Some(3), "x", ExplainOptions::default(), |_| {})
            .await
            .unwrap_err();
        assert_eq!(err.blocked, Some(reason), "{course}");
    }
    // "Not allowed" stays on this computer: a local model may read it.
    let local = app
        .explain_week(
            "DEMO505",
            Some(3),
            "local-505",
            ExplainOptions::default(),
            |_| {},
        )
        .await
        .unwrap();
    assert!(!local.sharing_reminder);
    // A cloud backend is refused before anything is sent.
    Store::open(&app.db_path())
        .unwrap()
        .insert_model_provider(&ProviderRow {
            id: "openai".into(),
            preset: "openai".into(),
            label: "OpenAI".into(),
            wire: "openai_responses".into(),
            base_url: "https://api.openai.com/v1".into(),
            created_at: Utc::now().trunc_subsecs(0),
            last_probe_json: None,
        })
        .unwrap();
    secrets.set("llm:openai", "sk-demo-not-a-real-key").unwrap();
    choose(
        &app,
        BackendRef::Provider {
            provider_id: "openai".into(),
        },
    );
    let (events, on_event) = collect();
    let err = app
        .explain_week(
            "DEMO505",
            Some(3),
            "cloud-505",
            ExplainOptions::default(),
            on_event,
        )
        .await
        .unwrap_err();
    assert_eq!(err.blocked, Some(BlockReason::MaterialSharingNotAllowed));
    assert!(
        !events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, GenEvent::Started { .. })),
        "no run started"
    );
}

#[tokio::test]
async fn an_answer_citing_nothing_real_is_bad_output() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_with_courses(temp.path());
    let server = with_local_model(&app).await;
    Mock::given(method("POST"))
        .respond_with(answer(&json!({
            "sections": [{"heading": "Made up", "paragraphs": [
                {"text": "Invented.", "citations": ["c8", "c9"]}
            ]}],
            "check_questions": []
        })))
        .mount(&server)
        .await;
    let err = app
        .explain_week(
            "DEMO101",
            Some(3),
            "explain-bad",
            ExplainOptions::default(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind, err.model_error),
        (AppErrorKind::Model, Some(ModelErrorKind::BadOutput))
    );
    let row = Store::open(&app.db_path())
        .unwrap()
        .generation("explain-bad")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, GenerationStatus::Failed);
    assert!(
        app.saved_explanations("DEMO101", Some(3))
            .unwrap()
            .is_empty()
    );
}
