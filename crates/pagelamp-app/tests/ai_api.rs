//! The AI facade (v0.3 M1) over temporary data dirs with in-memory secrets and local mock
//! servers. Synthetic data only; no real provider is ever called.

use std::path::Path;
use std::sync::Arc;

use chrono::{SubsecRound, Utc};
use pagelamp_app::ai::{
    BackendProblem, BackendRef, BackendState, CostBasis, EstimateRequest, LocalServerKind,
    ModelChoice, StructuredOutputTier,
};
use pagelamp_app::{App, AppErrorKind};
use pagelamp_core::ai::{
    AiFeature, BlockReason, Effort, MaterialSharing, ModelErrorKind, ProviderRow, UsageRecord,
};
use pagelamp_core::model::*;
use pagelamp_core::secrets::{MemorySecrets, SecretBackend};
use pagelamp_core::store::Store;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-demo-not-a-real-key-canary7731";
const COURSE: &str = "folder:demo/course/DEMO101";

fn app_in(dir: &Path) -> (App, Arc<MemorySecrets>) {
    let secrets = Arc::new(MemorySecrets::new());
    let app = App::open_at_with_secrets(dir.join("data"), secrets.clone()).unwrap();
    (app, secrets)
}

/// DEMO101 with a readable week-3 material.
fn seed_course(app: &App) {
    let store = Store::open(&app.db_path()).unwrap();
    store
        .upsert_source(&SourceRecord {
            id: "folder:demo".into(),
            kind: SourceKind::Folder,
            label: "Demo".into(),
            config: json!({ "path": "/demo/courses" }),
            last_synced_at: None,
            last_error: None,
            last_error_kind: None,
        })
        .unwrap();
    store
        .upsert_course(&CourseUpsert {
            id: COURSE.into(),
            source_id: "folder:demo".into(),
            external_id: "DEMO101".into(),
            code: Some("DEMO101".into()),
            name: "Intro to Demo Studies".into(),
            term_start: None,
            term_end: None,
            url: None,
            syllabus_text: None,
            lms: Default::default(),
        })
        .unwrap();
    let material = format!("{COURSE}/material/slides");
    store
        .upsert_material(&MaterialUpsert {
            id: material.clone(),
            course_id: COURSE.into(),
            module_id: None,
            kind: MaterialKind::File,
            title: "Week 3 slides".into(),
            url: None,
            local_path: None,
            mime: None,
            published_at: None,
            week_hint: Some(3),
        })
        .unwrap();
    store
        .set_text_state(&material, TextStatus::Ok, None, Some("hash"))
        .unwrap();
    store
        .replace_chunks(
            &material,
            &[Chunk {
                material_id: material.clone(),
                ord: 0,
                locator: None,
                text: "Stomata open in light. ".repeat(50),
            }],
        )
        .unwrap();
}

/// A cloud provider row for OpenAI (no network needed for estimates), with its key.
fn add_cloud_openai(app: &App, secrets: &MemorySecrets) -> BackendRef {
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
    secrets.set("llm:openai", KEY).unwrap();
    BackendRef::Provider {
        provider_id: "openai".into(),
    }
}

fn models_body() -> serde_json::Value {
    json!({"object": "list", "data": [{"id": "local-model", "object": "model"}]})
}

async fn mock_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_provider_is_checked_then_stored_without_its_key_in_the_database() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let server = mock_server().await;
    let record = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some(KEY))
        .await
        .unwrap();
    assert!(record.provider_id.starts_with("custom-"));
    assert_eq!(record.key_last4.as_deref(), Some("7731"));
    assert!(record.on_device, "a loopback address runs on this computer");
    assert_eq!(
        secrets
            .get(&format!("llm:{}", record.provider_id))
            .unwrap()
            .as_deref(),
        Some(KEY)
    );
    // The key is in the keychain only: not in the database or its WAL, not in reports.
    for file in ["pagelamp.db", "pagelamp.db-wal"] {
        if let Ok(bytes) = std::fs::read(app.data_dir().join(file)) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains("canary7731"),
                "{file}"
            );
        }
    }
    assert!(!app.diagnostic_report().unwrap().contains("canary7731"));
    // Adding it again is refused; replacing the key works.
    let again = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some(KEY))
        .await
        .unwrap_err();
    assert_eq!(again.kind, AppErrorKind::Invalid);
    let replaced = app
        .update_model_provider_key(&record.provider_id, KEY)
        .await
        .unwrap();
    assert_eq!(replaced.key_last4.as_deref(), Some("7731"));
    // Removing it deletes the key too.
    app.remove_model_provider(&record.provider_id).unwrap();
    assert_eq!(
        secrets.get(&format!("llm:{}", record.provider_id)).unwrap(),
        None
    );
    assert!(app.ai_status().unwrap().providers.is_empty());
}

#[tokio::test]
async fn bad_addresses_coding_plans_and_rejected_keys_are_refused_and_nothing_is_saved() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"error": {"message": "Incorrect API key provided", "code": "invalid_api_key"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    // Before any network call:
    let http = app
        .add_model_provider("custom", Some("http://llm.example.edu/v1"), Some(KEY))
        .await
        .unwrap_err();
    assert_eq!(http.kind, AppErrorKind::Invalid);
    let plan = app
        .add_model_provider(
            "custom",
            Some("https://api.z.ai/api/coding/paas/v4"),
            Some(KEY),
        )
        .await
        .unwrap_err();
    assert_eq!(plan.kind, AppErrorKind::Blocked);
    assert_eq!(plan.blocked, Some(BlockReason::CodingPlanKey));
    assert!(plan.message.contains("Z.ai"), "{}", plan.message);
    let plan_key = app
        .add_model_provider(
            "custom",
            Some(&format!("{}/v1", server.uri())),
            Some("sk-sp-demo-not-a-real-key"),
        )
        .await
        .unwrap_err();
    assert_eq!(plan_key.blocked, Some(BlockReason::CodingPlanKey));
    let no_key = app
        .add_model_provider("custom", Some("https://llm.example.edu/v1"), None)
        .await
        .unwrap_err();
    assert_eq!(no_key.kind, AppErrorKind::Invalid);
    let unknown = app
        .add_model_provider("nope", None, None)
        .await
        .unwrap_err();
    assert_eq!(unknown.kind, AppErrorKind::NotFound);
    // The one network call: the key is rejected, nothing is stored.
    let rejected = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some(KEY))
        .await
        .unwrap_err();
    assert_eq!(rejected.kind, AppErrorKind::Model);
    assert_eq!(rejected.model_error, Some(ModelErrorKind::AuthRejected));
    assert!(app.ai_status().unwrap().providers.is_empty());
    assert!(secrets.get("llm:custom").unwrap().is_none());
    server.verify().await;
    // Unknown providers are not found without any network call.
    let missing = BackendRef::Provider {
        provider_id: "contract-test-provider".into(),
    };
    assert_eq!(
        app.list_models(&missing).await.unwrap_err().kind,
        AppErrorKind::NotFound
    );
    assert_eq!(
        app.test_model(&missing, "m").await.unwrap_err().kind,
        AppErrorKind::NotFound
    );
}

#[tokio::test]
async fn status_asks_for_the_disclosure_then_is_ready() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let backend = add_cloud_openai(&app, &secrets);
    let status = app.ai_status().unwrap();
    let openai = &status.backends[0];
    assert_eq!(openai.state, BackendState::NeedsDisclosure);
    assert_eq!(openai.disclosure_acknowledged, None);
    assert_eq!(status.providers[0].key_last4.as_deref(), Some("7731"));
    assert_eq!(
        status.budget.monthly_micro_usd,
        Some(5_000_000),
        "D18 default"
    );
    assert_eq!(status.features.len(), 4);

    let version = openai.disclosure.version;
    let stale = app
        .acknowledge_ai_disclosure(&backend, version.wrapping_add(1))
        .unwrap_err();
    assert_eq!(stale.kind, AppErrorKind::Invalid);
    app.acknowledge_ai_disclosure(&backend, version).unwrap();
    assert_eq!(
        app.ai_status().unwrap().backends[0].state,
        BackendState::Ready
    );
    // A model routed to it needs the same note: still ready.
    app.set_feature_model(
        AiFeature::WeeklyExplanation,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "gpt-6-luna".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let routed = app.ai_status().unwrap();
    assert_eq!(routed.backends[0].state, BackendState::Ready);
    assert_eq!(routed.backends[0].cloud_disclosure, None);

    // A missing key is a setup problem.
    secrets.delete("llm:openai").unwrap();
    let status = app.ai_status().unwrap();
    assert_eq!(status.backends[0].state, BackendState::NeedsSetup);
    assert!(
        status.backends[0]
            .problems
            .contains(&BackendProblem::KeyMissing)
    );
}

#[tokio::test]
async fn the_estimate_says_what_would_block_a_run() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    seed_course(&app);
    let request = EstimateRequest::WeeklyExplanation {
        course: "DEMO101".into(),
        week: Some(3),
    };
    let block = |app: &App| app.estimate_generation(&request).unwrap().would_block;
    assert_eq!(block(&app), Some(BlockReason::NoModelChosen));

    let backend = add_cloud_openai(&app, &secrets);
    let choose = |model: &str| {
        app.set_feature_model(
            AiFeature::WeeklyExplanation,
            Some(ModelChoice {
                backend: backend.clone(),
                model: model.into(),
                effort: Effort::Lowest,
            }),
        )
        .unwrap()
    };
    choose("gpt-6-luna");
    // Blocks after the gate keep the full estimate (the UI shows the amount).
    let pending = app.estimate_generation(&request).unwrap();
    assert_eq!(
        pending.would_block,
        Some(BlockReason::DisclosureNotAcknowledged)
    );
    assert!(pending.micro_usd_upper.is_some() && pending.input_tokens > 0);
    let version = app.ai_status().unwrap().backends[0].disclosure.version;
    app.acknowledge_ai_disclosure(&backend, version).unwrap();

    let ok = app.estimate_generation(&request).unwrap();
    assert_eq!(ok.would_block, None);
    assert!(ok.price_known && ok.micro_usd_upper.unwrap() > 0, "{ok:?}");
    assert!(ok.input_tokens > 0);

    // Question (b): material text never goes to a cloud backend for a "not allowed" course.
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    // The gate's blocks (question (b) among them) carry no numbers: nothing would be sent.
    let not_allowed = app.estimate_generation(&request).unwrap();
    assert_eq!(
        not_allowed.would_block,
        Some(BlockReason::MaterialSharingNotAllowed)
    );
    assert_eq!(
        (
            not_allowed.micro_usd_upper,
            not_allowed.input_tokens,
            not_allowed.price_known
        ),
        (None, 0, false)
    );
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotSure)
        .unwrap();
    assert_eq!(block(&app), None, "not sure proceeds (D37 option 2)");

    // The gate's own blocks come first.
    Store::open(&app.db_path())
        .unwrap()
        .set_course_ai_access(COURSE, false)
        .unwrap();
    assert_eq!(block(&app), Some(BlockReason::CourseAiTurnedOff));
    Store::open(&app.db_path())
        .unwrap()
        .set_course_ai_access(COURSE, true)
        .unwrap();

    // Budget.
    app.set_monthly_budget(Some(1)).unwrap();
    let over = app.estimate_generation(&request).unwrap();
    assert_eq!(over.would_block, Some(BlockReason::BudgetReached));
    assert_eq!(
        over.micro_usd_upper, ok.micro_usd_upper,
        "the amount that would go over"
    );
    app.set_monthly_budget(None).unwrap();
    assert_eq!(block(&app), None, "no cap");

    // A model without a known price needs its acknowledgement.
    choose("gpt-9-unreleased");
    let unpriced = app.estimate_generation(&request).unwrap();
    assert_eq!(
        unpriced.would_block,
        Some(BlockReason::PriceUnknownNotAcknowledged)
    );
    assert!(!unpriced.price_known && unpriced.micro_usd_upper.is_none());
    assert!(unpriced.input_tokens > 0);
    app.acknowledge_unpriced_model(&backend, "gpt-9-unreleased")
        .unwrap();
    assert_eq!(block(&app), None);

    // Removing the provider forgets its routing.
    app.remove_model_provider("openai").unwrap();
    assert_eq!(block(&app), Some(BlockReason::NoModelChosen));
}

#[tokio::test]
async fn a_model_on_this_computer_still_gets_a_not_allowed_course_and_costs_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    seed_course(&app);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": [
            {"name": "local-model"},
            {"name": "big-model", "remote_host": "https://ollama.com:443"},
        ]})))
        .mount(&server)
        .await;
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
        AiFeature::WeeklyExplanation,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "local-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let version = app.ai_status().unwrap().backends[0].disclosure.version;
    app.acknowledge_ai_disclosure(&backend, version).unwrap();
    app.set_course_material_sharing("DEMO101", MaterialSharing::NotAllowed)
        .unwrap();
    app.set_monthly_budget(Some(0)).unwrap();
    let estimate = || {
        app.estimate_generation(&EstimateRequest::WeeklyExplanation {
            course: "DEMO101".into(),
            week: Some(3),
        })
        .unwrap()
    };
    // Until Ollama has listed its models, nothing says this one runs here: its price is unknown.
    let unlisted = estimate();
    assert_eq!(unlisted.would_block, None, "{unlisted:?}");
    assert_eq!(unlisted.micro_usd_upper, None);
    app.list_models(&backend).await.unwrap();
    let listed = estimate();
    assert_eq!(listed.would_block, None, "{listed:?}");
    assert_eq!(listed.micro_usd_upper, Some(0));
    assert!(listed.input_tokens > 0);
    // A model it listed as one it runs in the cloud, though its name has no cloud tag.
    app.set_feature_model(
        AiFeature::WeeklyExplanation,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "big-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let remote = estimate();
    assert_eq!(
        remote.would_block,
        Some(BlockReason::MaterialSharingNotAllowed)
    );
    assert_eq!(remote.micro_usd_upper, None);
}

/// A `custom` server on this computer may take no key (many local servers don't): nothing goes
/// to the keychain, no key counts as missing, and its models count as in the cloud.
#[tokio::test]
async fn a_custom_server_here_may_take_no_key() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&server)
        .await;
    let record = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some("  "))
        .await
        .unwrap();
    assert_eq!(record.key_last4, None);
    assert_eq!(
        secrets.get(&format!("llm:{}", record.provider_id)).unwrap(),
        None
    );
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.headers.contains_key("authorization"))
    );
    let backend = BackendRef::Provider {
        provider_id: record.provider_id.clone(),
    };
    let status = app.ai_status().unwrap();
    let row = status
        .backends
        .iter()
        .find(|b| b.backend == backend)
        .unwrap();
    assert_eq!(row.state, BackendState::NeedsDisclosure);
    assert!(!row.problems.contains(&BackendProblem::KeyMissing));
    let doctor = app.doctor().unwrap();
    assert_eq!(doctor.ai.providers[0].key_present, None);
    let models = app.list_models(&backend).await.unwrap();
    assert!(models[0].runs_in_cloud && !models[0].price_known);
    // Its models count as in the cloud, so its note is the cloud one: accepted, it is ready
    // (what `ai use … --yes` does).
    app.set_feature_model(
        AiFeature::CourseCalendar,
        Some(ModelChoice {
            backend: backend.clone(),
            model: "local-model".into(),
            effort: Effort::Lowest,
        }),
    )
    .unwrap();
    let state = |app: &App| {
        app.ai_status()
            .unwrap()
            .backends
            .into_iter()
            .find(|b| b.backend == backend)
            .unwrap()
    };
    let row = state(&app);
    assert_eq!(row.state, BackendState::NeedsDisclosure);
    let cloud = row
        .cloud_disclosure
        .expect("a server here has a cloud note");
    assert!(!cloud.on_device);
    let (facts, acknowledged) = app.model_disclosure(&backend, false).unwrap();
    assert_eq!((facts.version, acknowledged), (cloud.version, false));
    app.acknowledge_model_disclosure(&backend, false, facts.version)
        .unwrap();
    let row = state(&app);
    assert_eq!(row.state, BackendState::Ready);
    assert_eq!(row.cloud_disclosure_acknowledged, Some(cloud.version));
    assert_eq!(row.disclosure_acknowledged, None);
    // A key can still be added later.
    let keyed = app
        .update_model_provider_key(&record.provider_id, KEY)
        .await
        .unwrap();
    assert_eq!(keyed.key_last4.as_deref(), Some("7731"));
    assert_eq!(
        app.doctor().unwrap().ai.providers[0].key_present,
        Some(true)
    );
}

#[tokio::test]
async fn models_and_a_test_come_from_the_provider() {
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let server = mock_server().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"ok\\\":true}\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
                ),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            json!({"error": {"message": "model not found", "code": "model_not_found"}}),
        ))
        .mount(&server)
        .await;
    let record = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some(KEY))
        .await
        .unwrap();
    let backend = BackendRef::Provider {
        provider_id: record.provider_id.clone(),
    };
    let models = app.list_models(&backend).await.unwrap();
    assert_eq!(models[0].id, "local-model");
    // A server here other than Ollama or LM Studio can't say where it runs a model.
    assert!(!models[0].on_device && models[0].runs_in_cloud && !models[0].price_known);

    let good = app.test_model(&backend, "local-model").await.unwrap();
    assert!(good.ok, "{good:?}");
    assert_eq!(
        good.structured_output_tier,
        Some(StructuredOutputTier::NativeSchema)
    );
    let bad = app.test_model(&backend, "missing-model").await.unwrap();
    assert!(!bad.ok);
    assert_eq!(bad.error, Some(ModelErrorKind::ModelNotFound));
}

fn usage(backend: &str, feature: AiFeature, micro_usd: Option<u64>, basis: &str) -> UsageRecord {
    UsageRecord {
        at: Utc::now(),
        backend: backend.into(),
        model: "gpt-6-luna".into(),
        feature,
        input_uncached: 1000,
        cache_read: 200,
        cache_write: 0,
        output: 300,
        reasoning: Some(50),
        micro_usd,
        cost_basis: basis.into(),
        estimated: false,
        outcome: "ok".into(),
    }
}

#[tokio::test]
async fn usage_groups_runs_and_counts_only_priced_cost_toward_the_budget() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    add_cloud_openai(&app, &secrets);
    let store = Store::open(&app.db_path()).unwrap();
    for record in [
        usage(
            "provider:openai",
            AiFeature::WeeklyExplanation,
            Some(120),
            "priced",
        ),
        usage(
            "provider:openai",
            AiFeature::WeeklyExplanation,
            Some(80),
            "priced",
        ),
        usage("provider:mystery", AiFeature::StudyPlan, None, "unpriced"),
    ] {
        store.record_ai_usage(&record).unwrap();
    }
    let summary = app.usage_summary(None).unwrap();
    assert_eq!(summary.rows.len(), 2);
    let priced = summary
        .rows
        .iter()
        .find(|r| r.cost_basis == CostBasis::Priced)
        .unwrap();
    assert_eq!(priced.backend_label, "OpenAI");
    assert_eq!((priced.runs, priced.micro_usd), (2, Some(200)));
    assert_eq!(
        priced.input_tokens,
        2 * 1200,
        "totals include cached tokens"
    );
    let unpriced = summary
        .rows
        .iter()
        .find(|r| r.cost_basis == CostBasis::Unpriced)
        .unwrap();
    assert_eq!(unpriced.micro_usd, None);
    assert_eq!(summary.total_micro_usd, 200);
    assert_eq!(summary.budget.spent_micro_usd, 200);
}

#[tokio::test]
async fn removing_all_ai_data_takes_keys_settings_and_the_backup_too() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let backend = add_cloud_openai(&app, &secrets);
    app.set_monthly_budget(Some(42)).unwrap();
    let version = app.ai_status().unwrap().backends[0].disclosure.version;
    app.acknowledge_ai_disclosure(&backend, version).unwrap();
    Store::open(&app.db_path())
        .unwrap()
        .record_ai_usage(&usage(
            "provider:openai",
            AiFeature::StudyPlan,
            Some(5),
            "priced",
        ))
        .unwrap();
    let backup = pagelamp_core::store::backup_path(&app.db_path(), 3);
    std::fs::write(&backup, b"demo copy").unwrap();

    let report = app.remove_all_ai_data().unwrap();
    assert_eq!(
        (report.providers_removed, report.usage_rows_removed),
        (1, 1)
    );
    assert!(report.backup_removed);
    assert!(!backup.exists());
    assert_eq!(secrets.get("llm:openai").unwrap(), None);
    let status = app.ai_status().unwrap();
    assert!(status.providers.is_empty() && status.backends.is_empty());
    assert_eq!(
        status.budget.monthly_micro_usd,
        Some(5_000_000),
        "back to the default"
    );
    assert!(!app.remove_all_ai_data().unwrap().backup_removed);
}

/// Counts every keychain access.
#[derive(Default)]
struct Tripwire(std::sync::atomic::AtomicUsize);

impl SecretBackend for Tripwire {
    fn get(&self, _: &str) -> pagelamp_core::Result<Option<String>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(None)
    }
    fn set(&self, _: &str, _: &str) -> pagelamp_core::Result<()> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    fn delete(&self, _: &str) -> pagelamp_core::Result<()> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn local_work_and_keyless_providers_never_touch_the_keychain() {
    let temp = tempfile::tempdir().unwrap();
    let tripwire = Arc::new(Tripwire::default());
    let app = App::open_at_with_secrets(temp.path().join("data"), tripwire.clone()).unwrap();
    seed_course(&app);
    let store = Store::open(&app.db_path()).unwrap();
    for (id, preset, wire, url) in [
        (
            "openai",
            "openai",
            "openai_responses",
            "https://api.openai.com/v1",
        ),
        (
            "lm_studio",
            "lm_studio",
            "openai_chat",
            "http://127.0.0.1:1234/v1",
        ),
    ] {
        store
            .insert_model_provider(&ProviderRow {
                id: id.into(),
                preset: preset.into(),
                label: id.into(),
                wire: wire.into(),
                base_url: url.into(),
                created_at: Utc::now().trunc_subsecs(0),
                last_probe_json: None,
            })
            .unwrap();
    }
    let request = EstimateRequest::WeeklyExplanation {
        course: "DEMO101".into(),
        week: Some(3),
    };
    // Routing, acknowledgements and estimates are local, even for a provider with a key.
    for id in ["openai", "lm_studio"] {
        let backend = BackendRef::Provider {
            provider_id: id.into(),
        };
        app.set_feature_model(
            AiFeature::WeeklyExplanation,
            Some(ModelChoice {
                backend: backend.clone(),
                model: "gpt-6-luna".into(),
                effort: Effort::Lowest,
            }),
        )
        .unwrap();
        app.acknowledge_unpriced_model(&backend, "gpt-6-luna")
            .unwrap();
        app.estimate_generation(&request).unwrap();
    }
    assert_eq!(tripwire.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    // A provider without a key: its status and removal don't either.
    app.ai_status().unwrap();
    app.remove_model_provider("lm_studio").unwrap();
    assert_eq!(
        tripwire.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "only openai's key was read"
    );
    // Removing everything deletes only the keys that exist.
    app.remove_all_ai_data().unwrap();
    assert_eq!(tripwire.0.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_local_app_on_another_computer_is_disclosed_as_leaving_this_one() {
    use pagelamp_app::ai::{CostKind, RetentionFact, TrainingFact};
    let temp = tempfile::tempdir().unwrap();
    let (app, _) = app_in(temp.path());
    let store = Store::open(&app.db_path()).unwrap();
    for (id, url) in [
        ("here", "http://127.0.0.1:1234/v1"),
        ("there", "https://llm.example.invalid/v1"),
    ] {
        store
            .insert_model_provider(&ProviderRow {
                id: id.into(),
                preset: "lm_studio".into(),
                label: "LM Studio".into(),
                wire: "openai_chat".into(),
                base_url: url.into(),
                created_at: Utc::now().trunc_subsecs(0),
                last_probe_json: None,
            })
            .unwrap();
    }
    let status = app.ai_status().unwrap();
    let facts = |id: &str| {
        status
            .backends
            .iter()
            .find(|b| {
                b.backend
                    == BackendRef::Provider {
                        provider_id: id.into(),
                    }
            })
            .unwrap()
            .disclosure
            .clone()
    };
    let here = facts("here");
    assert!(here.on_device);
    assert_eq!(
        (here.retention, here.training, here.cost),
        (
            RetentionFact::OnDevice,
            TrainingFact::NoTraining,
            CostKind::FreeOnDevice
        )
    );
    // The gate treats it as cloud (question (b)), and so does the disclosure.
    let there = facts("there");
    assert!(!there.on_device);
    assert_eq!(
        (there.retention, there.training, there.cost),
        (
            RetentionFact::ProviderTerms,
            TrainingFact::Unknown,
            CostKind::SelfHosted
        )
    );
}

#[tokio::test]
async fn doctor_says_which_keys_are_there_and_which_local_servers_answer_but_never_more() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    // A server on this computer that accepts connections, and a port where nothing listens.
    let listening = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let open_port = listening.local_addr().unwrap().port();
    let closed_port = {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap().port()
    };
    let store = Store::open(&app.db_path()).unwrap();
    for (id, preset, wire, url) in [
        (
            "openai",
            "openai",
            "openai_responses",
            "https://api.openai.com/v1".to_string(),
        ),
        (
            "anthropic",
            "anthropic",
            "anthropic_messages",
            "https://api.anthropic.com".to_string(),
        ),
        (
            "lm_studio-1",
            "lm_studio",
            "openai_chat",
            format!("http://127.0.0.1:{open_port}/v1"),
        ),
        (
            "ollama-1",
            "ollama",
            "ollama_native",
            format!("http://127.0.0.1:{closed_port}"),
        ),
    ] {
        store
            .insert_model_provider(&ProviderRow {
                id: id.into(),
                preset: preset.into(),
                label: id.into(),
                wire: wire.into(),
                base_url: url,
                created_at: Utc::now().trunc_subsecs(0),
                last_probe_json: None,
            })
            .unwrap();
    }
    secrets.set("llm:openai", KEY).unwrap();

    let doctor = app.doctor().unwrap();
    let check = |preset: &str| {
        doctor
            .ai
            .providers
            .iter()
            .find(|p| p.preset == preset)
            .unwrap()
            .clone()
    };
    let openai = check("openai");
    assert_eq!(
        (openai.key_present, openai.on_device, openai.reachable),
        (Some(true), false, None)
    );
    assert_eq!(check("anthropic").key_present, Some(false));
    let lm_studio = check("lm_studio");
    assert_eq!(
        (
            lm_studio.key_present,
            lm_studio.on_device,
            lm_studio.reachable
        ),
        (None, true, Some(true))
    );
    assert_eq!(check("ollama").reachable, Some(false));
    // Whether the usual Ollama and LM Studio ports answer depends on this computer.
    let kinds: Vec<_> = doctor.ai.local_servers.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [LocalServerKind::Ollama, LocalServerKind::LmStudio]);

    let report = app.diagnostic_report().unwrap();
    assert!(report.contains("- AI providers: "), "{report}");
    assert!(report.contains("openai (key present)"), "{report}");
    assert!(report.contains("anthropic (key MISSING)"), "{report}");
    assert!(
        report.contains("lm_studio (on this computer, running)"),
        "{report}"
    );
    for secret in ["canary7731", &open_port.to_string(), "lm_studio-1"] {
        assert!(!report.contains(secret), "{secret} in the report");
    }
    let json = serde_json::to_string(&doctor).unwrap();
    assert!(!json.contains("canary7731") && !json.contains(&format!(":{open_port}")));
    drop(listening);
}

/// A keychain that can't be reached (no Secret Service, no default keychain).
struct NoKeychain;

impl SecretBackend for NoKeychain {
    fn get(&self, _: &str) -> pagelamp_core::Result<Option<String>> {
        Err(pagelamp_core::Error::Secret("no keychain".into()))
    }
    fn set(&self, _: &str, _: &str) -> pagelamp_core::Result<()> {
        Err(pagelamp_core::Error::Secret("no keychain".into()))
    }
    fn delete(&self, _: &str) -> pagelamp_core::Result<()> {
        Err(pagelamp_core::Error::Secret("no keychain".into()))
    }
}

/// Without a keychain, a custom server here works without a key and can be removed; a
/// provider that needs a key still stops at the keychain.
#[tokio::test]
async fn a_keyless_server_here_needs_no_keychain() {
    let temp = tempfile::tempdir().unwrap();
    let app = App::open_at_with_secrets(temp.path().join("data"), Arc::new(NoKeychain)).unwrap();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&server)
        .await;
    let record = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), None)
        .await
        .unwrap();
    let backend = BackendRef::Provider {
        provider_id: record.provider_id.clone(),
    };
    assert_eq!(app.ai_status().unwrap().backends.len(), 1);
    assert_eq!(
        app.list_models(&backend).await.unwrap()[0].id,
        "local-model"
    );
    app.remove_model_provider(&record.provider_id).unwrap();
    assert!(app.ai_status().unwrap().providers.is_empty());

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
    assert!(app.ai_status().is_err());
    // Its key can't be deleted: nothing is removed, so it can be tried again.
    let err = app.remove_model_provider("openai").unwrap_err();
    assert!(err.message.contains("llm:openai"), "{}", err.message);
    assert!(
        Store::open(&app.db_path())
            .unwrap()
            .model_provider("openai")
            .unwrap()
            .is_some()
    );
}

/// A keychain that keeps keys but won't delete them (locked, or access refused).
#[derive(Default)]
struct StuckKeychain(MemorySecrets);

impl SecretBackend for StuckKeychain {
    fn get(&self, id: &str) -> pagelamp_core::Result<Option<String>> {
        self.0.get(id)
    }
    fn set(&self, id: &str, secret: &str) -> pagelamp_core::Result<()> {
        self.0.set(id, secret)
    }
    fn delete(&self, _: &str) -> pagelamp_core::Result<()> {
        Err(pagelamp_core::Error::Secret("access refused".into()))
    }
}

/// A key the keychain won't delete is never left behind: removing its provider changes
/// nothing and says the key is still there; removing all AI data keeps that provider and its
/// key, names it, and takes the rest.
#[tokio::test]
async fn a_key_the_keychain_wont_delete_keeps_its_provider() {
    let temp = tempfile::tempdir().unwrap();
    let keychain = Arc::new(StuckKeychain::default());
    let app = App::open_at_with_secrets(temp.path().join("data"), keychain.clone()).unwrap();
    let server = mock_server().await;
    // A custom server here, added with a key: that key must go with it too.
    let record = app
        .add_model_provider("custom", Some(&format!("{}/v1", server.uri())), Some(KEY))
        .await
        .unwrap();
    let account = format!("llm:{}", record.provider_id);
    let err = app.remove_model_provider(&record.provider_id).unwrap_err();
    assert!(err.message.contains(&account), "{}", err.message);
    assert_eq!(app.ai_status().unwrap().providers.len(), 1);
    assert_eq!(keychain.get(&account).unwrap().as_deref(), Some(KEY));

    // A second provider, keyless: removing all takes it and leaves the stuck one, named.
    let keyless = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&keyless)
        .await;
    app.add_model_provider("custom", Some(&format!("{}/v1", keyless.uri())), None)
        .await
        .unwrap();
    let report = app.remove_all_ai_data().unwrap();
    assert_eq!(report.keys_left, vec![record.provider_id.clone()]);
    assert_eq!(report.providers_removed, 1);
    let left: Vec<String> = app
        .ai_status()
        .unwrap()
        .providers
        .into_iter()
        .map(|p| p.provider_id)
        .collect();
    assert_eq!(left, vec![record.provider_id.clone()]);
    assert_eq!(keychain.get(&account).unwrap().as_deref(), Some(KEY));
}

/// A provider added without a key never uses a key that an earlier provider at the same
/// address left in the keychain, and adding it clears that entry.
#[tokio::test]
async fn an_old_key_at_the_same_address_doesn_t_come_back() {
    let temp = tempfile::tempdir().unwrap();
    let (app, secrets) = app_in(temp.path());
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&server)
        .await;
    let url = format!("{}/v1", server.uri());
    let first = app
        .add_model_provider("custom", Some(&url), Some(KEY))
        .await
        .unwrap();
    // The row goes, the key stays (as a removal that failed halfway once could leave it).
    Store::open(&app.db_path())
        .unwrap()
        .delete_model_provider(&first.provider_id)
        .unwrap();
    let account = format!("llm:{}", first.provider_id);
    assert!(secrets.get(&account).unwrap().is_some());
    let again = app
        .add_model_provider("custom", Some(&url), None)
        .await
        .unwrap();
    assert_eq!(again.provider_id, first.provider_id);
    assert_eq!(again.key_last4, None);
    assert_eq!(secrets.get(&account).unwrap(), None);
    // Even if the entry were back, it isn't read: the list goes without a key.
    secrets.set(&account, KEY).unwrap();
    let backend = BackendRef::Provider {
        provider_id: again.provider_id.clone(),
    };
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_body()))
        .mount(&server)
        .await;
    app.list_models(&backend).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|r| !r.headers.contains_key("authorization"))
    );
    assert_eq!(app.ai_status().unwrap().providers[0].key_last4, None);
    // A key set later is used, and removing the provider deletes it.
    app.update_model_provider_key(&again.provider_id, KEY)
        .await
        .unwrap();
    assert_eq!(
        app.ai_status().unwrap().providers[0].key_last4.as_deref(),
        Some("7731")
    );
    app.remove_model_provider(&again.provider_id).unwrap();
    assert_eq!(secrets.get(&account).unwrap(), None);
}
