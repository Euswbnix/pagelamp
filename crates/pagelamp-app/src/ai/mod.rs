//! PageLamp's own model calls, as the facade offers them (v0.3 M1; design §3.8). The logic lives
//! here, so the Tauri app, the Swift shell and the CLI can't diverge (rule 12): disclosure facts,
//! policy questions, budgets and estimates are computed in Rust, and the UIs only render them.
//!
//! Model code is in `pagelamp-llm` (drivers) and `pagelamp_core::ai_gate` (the only producer of
//! prompt text).

pub(crate) mod codex;
mod estimate;
pub mod prompts;
mod providers;
pub(crate) mod run;
mod settings;
mod types;
mod usage;

pub use codex::{CHATGPT_PLAN_OFFERED, DEFAULT_WEEKLY_CAP};
pub(crate) use estimate::{calendar_budget, feature_choice, request_shape};
pub(crate) use settings::backend_key;
pub use types::*;

use std::time::Duration;

use chrono::NaiveDate;
use pagelamp_core::ai::{AiFeature, Destination, MaterialSharing};
use pagelamp_core::ingest::sha256_hex;
use pagelamp_llm::profile::{self, ProviderProfile, Retention, Training, Wire};
use pagelamp_llm::{Backend, CancellationToken, HttpDriver};

use crate::{App, AppError, AppErrorKind, Result};

/// Bumped whenever the fixed disclosure paragraphs (limitations and risks, ownership) change in
/// the UIs: part of every `DisclosureFacts::version`, so the student is asked again.
const DISCLOSURE_WORDING_VERSION: u32 = 1;

/// How long `detect_local_servers` waits for a local server.
const DETECT_TIMEOUT: Duration = Duration::from_secs(2);

/// The warning threshold of the monthly budget (owner decision D18).
pub const BUDGET_WARN_AT_PERCENT: u8 = 80;

/// The default monthly budget for API keys: US$5 (owner decision D18), in micro-USD.
pub const DEFAULT_MONTHLY_BUDGET_MICRO_USD: u64 = 5_000_000;

impl App {
    /// The providers PageLamp can set up, with their data-policy line (no network call).
    pub fn model_provider_presets(&self) -> Vec<ProviderPreset> {
        profile::presets().iter().map(provider_preset).collect()
    }

    /// Ollama and LM Studio on this computer: which are running (a quick loopback call each).
    pub async fn detect_local_servers(&self) -> Result<Vec<LocalServer>> {
        let mut servers = Vec::new();
        for (preset_id, kind) in [
            ("ollama", LocalServerKind::Ollama),
            ("lm_studio", LocalServerKind::LmStudio),
        ] {
            let Some(preset) = profile::preset(preset_id) else {
                continue;
            };
            let base_url = preset
                .base_url
                .as_ref()
                .map(|url| url.as_str().trim_end_matches('/').to_string())
                .unwrap_or_default();
            let running = match HttpDriver::new(preset.clone(), None) {
                Ok(driver) => {
                    let backend = Backend::Http(driver);
                    let cancel = CancellationToken::new();
                    matches!(
                        tokio::time::timeout(DETECT_TIMEOUT, backend.list_models(&cancel)).await,
                        Ok(Ok(_))
                    )
                }
                Err(_) => false,
            };
            servers.push(LocalServer {
                kind,
                base_url,
                running,
            });
        }
        Ok(servers)
    }

    // ----- status and providers -----------------------------------------------------------------

    /// Providers, feature routing, backends with their disclosure facts, and the budget. No
    /// network call (local servers: `detect_local_servers`).
    pub fn ai_status(&self) -> Result<AiStatus> {
        let store = self.read_store()?;
        let acks = settings::disclosures(&store)?;
        let mut providers = Vec::new();
        let mut backends = Vec::new();
        let routing = settings::routing(&store)?;
        // The ChatGPT plan first (design §2.1's order), once it is set up or chosen.
        let codex_chosen = routing
            .0
            .values()
            .any(|choice| choice.backend == BackendRef::Codex);
        // Not offered in this build: no Codex backend at all (the UIs hide the card).
        if self.chatgpt_plan_offered() && (self.codex_installed() || codex_chosen) {
            let disclosure = self.disclosure(&BackendRef::Codex)?;
            let acknowledged = acks
                .get(&settings::backend_key(&BackendRef::Codex))
                .map(|a| a.version);
            let mut problems = Vec::new();
            if !self.codex_installed() {
                problems.push(BackendProblem::RuntimeMissing);
            }
            if self.codex_login_cached() != Some(CodexLoginState::Chatgpt) {
                problems.push(BackendProblem::NotSignedIn);
            }
            if acknowledged.is_some_and(|version| version != disclosure.version) {
                problems.push(BackendProblem::DisclosureChanged);
            }
            let state = if problems.contains(&BackendProblem::RuntimeMissing)
                || problems.contains(&BackendProblem::NotSignedIn)
            {
                BackendState::NeedsSetup
            } else if acknowledged != Some(disclosure.version) {
                BackendState::NeedsDisclosure
            } else {
                BackendState::Ready
            };
            backends.push(AiBackendStatus {
                backend: BackendRef::Codex,
                label: codex::CODEX_LABEL.to_string(),
                kind: BackendKind::Codex,
                state,
                problems,
                disclosure,
                disclosure_acknowledged: acknowledged,
                cloud_disclosure: None,
                cloud_disclosure_acknowledged: None,
            });
        }
        for (record, provider) in self.provider_records(&store)? {
            let backend = BackendRef::Provider {
                provider_id: record.provider_id.clone(),
            };
            let profile = &provider.profile;
            let disclosure = disclosure_for(profile);
            let acknowledged = acks
                .get(&settings::backend_key(&backend))
                .map(|a| a.version);
            // A server here may run a model in the cloud (any but LM Studio): such a model has
            // a note of its own. Only Ollama and LM Studio say a model runs here.
            let here_note =
                !profile.on_device() || matches!(profile.id.as_str(), "ollama" | "lm_studio");
            let cloud_disclosure = (profile.on_device() && profile.id != "lm_studio")
                .then(|| cloud_via_local_disclosure(profile));
            let cloud_acknowledged = cloud_disclosure
                .as_ref()
                .and_then(|_| acks.get(&settings::cloud_key(&backend)))
                .map(|a| a.version);
            let here_ok = here_note && acknowledged == Some(disclosure.version);
            let cloud_ok = cloud_disclosure
                .as_ref()
                .is_some_and(|facts| cloud_acknowledged == Some(facts.version));
            // Each routed model needs the note a run checks for it (`disclosure_and_key`, as
            // `other_blocks` does), placed as an estimate places it (a run asks the server
            // again); with none routed, either note the backend has.
            let mut routed = false;
            let mut notes_ok = true;
            for choice in routing
                .0
                .values()
                .filter(|choice| choice.backend == backend)
            {
                routed = true;
                let runs_here = self.estimate_profile(choice)?.1 == Destination::OnDevice;
                let (facts, key) = self.disclosure_and_key(&backend, runs_here)?;
                notes_ok &= acks
                    .get(&key)
                    .is_some_and(|ack| ack.version == facts.version);
            }
            if !routed {
                notes_ok = here_ok || cloud_ok;
            }
            let mut problems = Vec::new();
            if provider.needs_key() && provider.key.is_none() {
                problems.push(BackendProblem::KeyMissing);
            }
            if acknowledged.is_some_and(|version| version != disclosure.version)
                || cloud_disclosure.as_ref().is_some_and(|facts| {
                    cloud_acknowledged.is_some_and(|version| version != facts.version)
                })
            {
                problems.push(BackendProblem::DisclosureChanged);
            }
            let state = if problems.contains(&BackendProblem::KeyMissing) {
                BackendState::NeedsSetup
            } else if !notes_ok {
                BackendState::NeedsDisclosure
            } else {
                BackendState::Ready
            };
            backends.push(AiBackendStatus {
                backend,
                label: record.label.clone(),
                kind: if record.on_device {
                    BackendKind::Local
                } else {
                    BackendKind::ApiKey
                },
                state,
                problems,
                disclosure,
                disclosure_acknowledged: acknowledged,
                cloud_disclosure,
                cloud_disclosure_acknowledged: cloud_acknowledged,
            });
            providers.push(record);
        }
        let features = AiFeature::ALL
            .iter()
            .map(|feature| FeatureRouting {
                feature: *feature,
                choice: routing.0.get(feature).cloned(),
            })
            .collect();
        Ok(AiStatus {
            chatgpt_plan_offered: self.chatgpt_plan_offered(),
            backends,
            providers,
            features,
            budget: usage::budget_status(&store)?,
        })
    }

    /// Add an API-key or local provider: checks the address (HTTPS unless this computer), refuses
    /// coding-plan endpoints and keys, validates the key with a free model-list call, and keeps
    /// the key in the keychain (`llm:<provider_id>`). Nothing is saved when a check fails.
    pub async fn add_model_provider(
        &self,
        preset: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<ModelProviderRecord> {
        self.add_provider(preset, base_url, api_key).await
    }

    /// Replace a provider's key (validated first; the old key stays if the new one fails).
    pub async fn update_model_provider_key(
        &self,
        provider_id: &str,
        api_key: &str,
    ) -> Result<ModelProviderRecord> {
        self.replace_provider_key(provider_id, api_key).await
    }

    /// Remove a provider: its row, its keychain entry, and every feature routed to it.
    pub fn remove_model_provider(&self, provider_id: &str) -> Result<()> {
        self.delete_provider(provider_id)
    }

    /// The models a backend offers: its live list with PageLamp's price and capability data.
    pub async fn list_models(&self, backend: &BackendRef) -> Result<Vec<ModelInfo>> {
        self.list_models_with(backend, &CancellationToken::new())
            .await
    }

    /// `list_models`, stopped by `cancel`. A server on this computer says which models it runs
    /// here only if it is LM Studio or Ollama (Ollama marks its cloud models); any other server
    /// here can't say, so its models count as in the cloud, without a known price. Ollama's
    /// list is kept for estimates (`settings::LISTED_MODELS`).
    pub(crate) async fn list_models_with(
        &self,
        backend: &BackendRef,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModelInfo>> {
        if *backend == BackendRef::Codex {
            self.require_chatgpt_plan()?;
            return Ok(self.codex_models());
        }
        let provider = self.provider_for(backend)?;
        let provider_id = provider.row.id.clone();
        let profile = provider.profile.clone();
        let local = profile.on_device();
        let says_where = matches!(profile.id.as_str(), "ollama" | "lm_studio");
        let models = provider.backend()?.list_models(cancel).await?;
        let models: Vec<ModelInfo> = models
            .into_iter()
            .map(|model| {
                let quirks = profile.model_quirks(&model.id);
                let here = model.on_device && (!local || says_where);
                ModelInfo {
                    price_known: here
                        || pagelamp_llm::catalog::lookup(&profile.id, &model.id).is_some(),
                    runs_in_cloud: local && !here,
                    on_device: here,
                    context_window: model.context_window,
                    reasoning_always_on: quirks.thinking_always_on,
                    label: model.display_name,
                    suggested_for: Vec::new(),
                    id: model.id,
                }
            })
            .collect();
        if local && profile.id == "ollama" {
            // For estimates only: a run asks the daemon itself, so a failed write changes
            // nothing but the next estimate.
            if let Err(err) = self.keep_listed_models(&provider_id, backend, &models) {
                tracing::warn!(target: "pagelamp::ai", "keeping the model list failed: {:?}", err.kind);
            }
        }
        Ok(models)
    }

    /// Keep Ollama's list for estimates (`ai.listed_models`), in one transaction and only while
    /// the provider is still there: a list that arrives after `ai remove` isn't written back.
    fn keep_listed_models(
        &self,
        provider_id: &str,
        backend: &BackendRef,
        models: &[ModelInfo],
    ) -> Result<()> {
        let store = self.write_store()?;
        store.in_transaction(|store| {
            if store.model_provider(provider_id)?.is_none() {
                return Ok(());
            }
            let mut listed: settings::ListedModels =
                store.setting(settings::LISTED_MODELS)?.unwrap_or_default();
            listed.insert(
                settings::backend_key(backend),
                models.iter().map(|m| (m.id.clone(), m.on_device)).collect(),
            );
            store.set_setting(settings::LISTED_MODELS, &listed)
        })?;
        Ok(())
    }

    /// "Test": a tiny request with no course data. A model that answers badly or refuses the
    /// key is a report with `ok: false`, not an error; the last test is kept with the provider.
    pub async fn test_model(&self, backend: &BackendRef, model: &str) -> Result<ProbeReport> {
        if *backend == BackendRef::Codex {
            return self.codex_test(model).await;
        }
        let provider = self.provider_for(backend)?;
        let provider_id = provider.row.id.clone();
        let thinking = provider.profile.model_quirks(model).thinking_always_on;
        let started = std::time::Instant::now();
        let report = match provider
            .backend()?
            .probe(model, CancellationToken::new())
            .await
        {
            Ok(probe) => ProbeReport {
                ok: true,
                latency_ms: u32::try_from(probe.latency.as_millis()).unwrap_or(u32::MAX),
                structured_output_tier: Some(tier(probe.json_tier)),
                thinking_always_on: probe.thinking_always_on,
                error: None,
            },
            Err(pagelamp_llm::LlmError::Model(error)) => ProbeReport {
                ok: false,
                latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
                structured_output_tier: None,
                thinking_always_on: thinking,
                error: Some(error.kind),
            },
            Err(other) => return Err(other.into()),
        };
        let json = serde_json::to_string(&report).ok();
        self.write_store()?
            .set_model_provider_probe(&provider_id, json.as_deref())?;
        Ok(report)
    }

    // ----- choices and acknowledgements ------------------------------------------------------------

    /// Which model a feature uses (`None`: none). The backend must exist.
    pub fn set_feature_model(&self, feature: AiFeature, choice: Option<ModelChoice>) -> Result<()> {
        if let Some(choice) = &choice {
            if choice.backend == BackendRef::Codex {
                self.require_chatgpt_plan()?;
                codex::check_codex_model(&choice.model)?;
            } else {
                self.provider_profile(&choice.backend)?;
            }
            if choice.model.trim().is_empty() {
                return Err(AppError::new(AppErrorKind::Invalid, "Choose a model."));
            }
        }
        let store = self.write_store()?;
        let mut routing = settings::routing(&store)?;
        match choice {
            Some(choice) => routing.0.insert(feature, choice),
            None => routing.0.remove(&feature),
        };
        store.set_setting(settings::ROUTING, &routing)?;
        Ok(())
    }

    /// The student read a backend's disclosure. `version` must be the one currently shown:
    /// facts that changed since are asked again.
    pub fn acknowledge_ai_disclosure(&self, backend: &BackendRef, version: u32) -> Result<()> {
        if *backend == BackendRef::Codex {
            self.require_chatgpt_plan()?;
        }
        let current = self.disclosure(backend)?.version;
        if version != current {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "What this service does with your data changed; please read it again.",
            ));
        }
        let store = self.write_store()?;
        let mut acks = settings::disclosures(&store)?;
        acks.insert(
            settings::backend_key(backend),
            settings::Acknowledgement {
                version,
                at: chrono::Utc::now(),
            },
        );
        store.set_setting(settings::DISCLOSURES, &acks)?;
        Ok(())
    }

    /// "The budget is not enforced for this model": needed before the first run of a model
    /// without a known price.
    pub fn acknowledge_unpriced_model(&self, backend: &BackendRef, model: &str) -> Result<()> {
        if *backend == BackendRef::Codex {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "Runs with the ChatGPT plan have no price to acknowledge.",
            ));
        }
        self.provider_profile(backend)?;
        let store = self.write_store()?;
        let mut acks = settings::unpriced(&store)?;
        let models = acks.entry(settings::backend_key(backend)).or_default();
        if !models.iter().any(|m| m == model) {
            models.push(model.to_string());
        }
        store.set_setting(settings::UNPRICED, &acks)?;
        Ok(())
    }

    /// The monthly soft cap for API keys, in micro-USD (`None`: no cap).
    pub fn set_monthly_budget(&self, micro_usd: Option<u64>) -> Result<()> {
        self.write_store()?.set_setting(
            settings::BUDGET,
            &settings::BudgetSetting {
                monthly_micro_usd: micro_usd,
            },
        )?;
        Ok(())
    }

    // ----- estimate and usage ---------------------------------------------------------------------

    /// "≈ $x" before Generate (an upper bound), and what would block the run. Local and cheap:
    /// no network call.
    pub fn estimate_generation(&self, request: &EstimateRequest) -> Result<CostEstimate> {
        self.estimate(request)
    }

    /// Token counts and estimated cost of a month (default: this month).
    pub fn usage_summary(&self, month: Option<NaiveDate>) -> Result<UsageSummary> {
        self.usage(month)
    }

    /// The student's answer to "may this course's materials be shared with an AI service?".
    /// "Not allowed" stops this app's runs that send the course's materials to a cloud model
    /// (`stop_course_runs`); a model on this computer may go on.
    pub fn set_course_material_sharing(&self, course: &str, answer: MaterialSharing) -> Result<()> {
        let store = self.write_store()?;
        let course = store.resolve_course_with(course, true)?;
        store.set_course_material_sharing(&course.id, answer)?;
        if answer == MaterialSharing::NotAllowed {
            self.stop_course_runs(&course.id, true);
        }
        Ok(())
    }

    /// Keys, providers, generations, the usage ledger, AI settings and the pre-update backup
    /// (from schema 4 on it holds AI data too; the result says it was removed).
    pub fn remove_all_ai_data(&self) -> Result<RemoveAiDataReport> {
        // Codex first: its sign-in ends before its folder goes (Busy if Codex is running).
        self.forget_codex()?;
        // This process's runs stop; a run elsewhere keeps nothing once its provider is gone.
        self.state.runs.cancel_all();
        let store = self.write_store()?;
        // Keys first: a provider whose key can't be deleted stays, and is named.
        let keyless = settings::keyless(&store)?;
        let mut keys_left = Vec::new();
        for row in store.model_providers()? {
            if !providers::uses_key(&row) || keyless.contains(&row.id) {
                continue;
            }
            if let Err(err) = providers::forget_key(&*self.secrets, &row) {
                tracing::warn!(target: "pagelamp::ai", "{}", err.message);
                keys_left.push(row.id);
            }
        }
        let removed = store.remove_all_ai_data(&keys_left)?;
        for key in settings::ALL_KEYS {
            store.remove_setting(key)?;
        }
        drop(store);
        let backup_removed = pagelamp_core::store::database_backup(&self.db_path()).is_some();
        if backup_removed {
            pagelamp_core::store::delete_database_backups(&self.db_path()).map_err(|err| {
                AppError::new(
                    AppErrorKind::Internal,
                    format!("could not delete the pre-update backup: {err}"),
                )
            })?;
        }
        Ok(RemoveAiDataReport {
            providers_removed: removed.providers,
            generations_removed: removed.generations,
            usage_rows_removed: removed.usage_rows,
            backup_removed,
            keys_left,
        })
    }

    /// At a sync's start: usage lines older than the ledger's 13 months, and calendar
    /// proposals dismissed more than 30 days ago with the runs that wrote them, for a student
    /// who no longer uses the AI features. Problems are logged; they never stop the sync.
    pub(crate) fn prune_ai_records_at_sync_start(&self) {
        let now = chrono::Utc::now();
        let result = self.write_store().and_then(|store| -> Result<()> {
            store.prune_ai_usage(pagelamp_core::store::usage_cutoff(now))?;
            store.prune_dismissed_calendars(now)?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::warn!(target: "pagelamp::ai", "pruning AI records failed: {:?}", err.kind);
        }
    }
}

fn tier(tier: pagelamp_llm::request::JsonTier) -> StructuredOutputTier {
    match tier {
        pagelamp_llm::request::JsonTier::NativeSchema => StructuredOutputTier::NativeSchema,
        pagelamp_llm::request::JsonTier::JsonObject => StructuredOutputTier::JsonObject,
        pagelamp_llm::request::JsonTier::PromptOnly => StructuredOutputTier::PromptOnly,
    }
}

fn provider_preset(preset: &ProviderProfile) -> ProviderPreset {
    ProviderPreset {
        id: preset.id.clone(),
        label: preset.display_name.clone(),
        wire: provider_wire(preset.wire),
        default_base_url: preset
            .base_url
            .as_ref()
            .map(|url| url.as_str().trim_end_matches('/').to_string()),
        needs_key: preset.auth != profile::Auth::None,
        base_url_editable: preset.base_url_editable,
        local: preset.local,
        data_policy: disclosure_for(preset),
    }
}

pub(crate) fn provider_wire(wire: Wire) -> ProviderWire {
    match wire {
        Wire::OpenAiResponses => ProviderWire::OpenaiResponses,
        Wire::OpenAiChat => ProviderWire::OpenaiChat,
        Wire::AnthropicMessages => ProviderWire::AnthropicMessages,
        Wire::OllamaNative => ProviderWire::OllamaNative,
    }
}

/// `doctor`'s AI facts (`AiDoctor`). Local only: the keychain is asked whether each key is
/// there, and servers on this computer whether they accept connections; nothing is sent.
pub(crate) fn doctor_checks(
    store: Option<&pagelamp_core::store::Store>,
    secrets: &dyn pagelamp_core::secrets::SecretBackend,
) -> AiDoctor {
    let rows = store
        .and_then(|store| store.model_providers().ok())
        .unwrap_or_default();
    let providers = rows
        .iter()
        .map(|row| {
            let profile = providers::profile_from_row(row).ok();
            let on_device = profile.as_ref().is_some_and(ProviderProfile::on_device);
            // A server here that may go without a key has none missing.
            let key_optional = profile.as_ref().is_some_and(providers::key_optional);
            AiProviderCheck {
                preset: row.preset.clone(),
                on_device,
                key_present: providers::uses_key(row)
                    .then(|| matches!(secrets.get(&providers::key_account(&row.id)), Ok(Some(_))))
                    .filter(|present| *present || !key_optional),
                reachable: profile
                    .and_then(|profile| profile.base_url)
                    .filter(|_| on_device)
                    .map(|url| accepts_connections(&url)),
            }
        })
        .collect();
    let local_servers = [
        ("ollama", LocalServerKind::Ollama),
        ("lm_studio", LocalServerKind::LmStudio),
    ]
    .into_iter()
    .filter_map(|(preset_id, kind)| {
        let url = profile::preset(preset_id)?.base_url.clone()?;
        Some(LocalServer {
            kind,
            base_url: url.as_str().trim_end_matches('/').to_string(),
            running: accepts_connections(&url),
        })
    })
    .collect();
    AiDoctor {
        providers,
        local_servers,
    }
}

/// Whether something on this computer accepts connections at `url`'s port (loopback
/// addresses only; anything else is not contacted).
fn accepts_connections(url: &url::Url) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    if !pagelamp_llm::is_loopback(url) {
        return false;
    }
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    (host, port).to_socket_addrs().is_ok_and(|mut addrs| {
        addrs.any(|addr| {
            addr.ip().is_loopback()
                && TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
        })
    })
}

impl App {
    /// The disclosure to read before using a model of `backend`, and whether its current
    /// version is acknowledged. `runs_here`: the model runs on this computer, as a run decides
    /// it (`model_runs_here`). A model that a server on this computer may run in the cloud has a
    /// disclosure of its own, acknowledged apart from the server's ("nothing leaves it").
    pub fn model_disclosure(
        &self,
        backend: &BackendRef,
        runs_here: bool,
    ) -> Result<(DisclosureFacts, bool)> {
        let (facts, key) = self.disclosure_and_key(backend, runs_here)?;
        let acknowledged = settings::disclosures(&self.read_store()?)?
            .get(&key)
            .is_some_and(|ack| ack.version == facts.version);
        Ok((facts, acknowledged))
    }

    /// The student read `model_disclosure(backend, runs_here)`; `version` must be the one shown.
    pub fn acknowledge_model_disclosure(
        &self,
        backend: &BackendRef,
        runs_here: bool,
        version: u32,
    ) -> Result<()> {
        let (facts, key) = self.disclosure_and_key(backend, runs_here)?;
        if *backend == BackendRef::Codex {
            self.require_chatgpt_plan()?;
        }
        if version != facts.version {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                "What this service does with your data changed; please read it again.",
            ));
        }
        let store = self.write_store()?;
        let mut acks = settings::disclosures(&store)?;
        acks.insert(
            key,
            settings::Acknowledgement {
                version,
                at: chrono::Utc::now(),
            },
        );
        store.set_setting(settings::DISCLOSURES, &acks)?;
        Ok(())
    }

    /// The disclosure for a model of `backend` and the key its acknowledgement is kept under.
    pub(crate) fn disclosure_and_key(
        &self,
        backend: &BackendRef,
        runs_here: bool,
    ) -> Result<(DisclosureFacts, String)> {
        if *backend == BackendRef::Codex {
            return Ok((self.disclosure(backend)?, settings::backend_key(backend)));
        }
        let profile = self.provider_profile(backend)?;
        Ok(if profile.on_device() && !runs_here {
            (
                cloud_via_local_disclosure(&profile),
                settings::cloud_key(backend),
            )
        } else {
            (disclosure_for(&profile), settings::backend_key(backend))
        })
    }

    /// The disclosure facts of any backend (Codex: with the plan type as far as known).
    pub(crate) fn disclosure(&self, backend: &BackendRef) -> Result<DisclosureFacts> {
        match backend {
            BackendRef::Codex => Ok(codex::codex_disclosure(None)),
            _ => Ok(disclosure_for(&self.provider_profile(backend)?)),
        }
    }
}

/// The disclosure facts of an API-key or local provider.
pub(crate) fn disclosure_for(provider: &ProviderProfile) -> DisclosureFacts {
    let policy = &provider.data_policy;
    // By address, like the gate: a local app (Ollama, LM Studio) on another computer is not on
    // this device, and what happens to the data there is up to whoever runs it.
    let on_device = provider.on_device();
    let elsewhere = provider.local && !on_device;
    let facts = DisclosureFacts {
        version: 0,
        sends: vec![SentData::Structure, SentData::MaterialText],
        recipient: Recipient {
            name: provider.display_name.clone(),
            terms_url: policy.terms_url.clone(),
        },
        training: match policy.training {
            _ if elsewhere => TrainingFact::Unknown,
            Training::NoTraining => TrainingFact::NoTraining,
            Training::MayTrainFreeTier => TrainingFact::MayTrainFreeTier,
            Training::MayTrain => TrainingFact::MayTrain {
                how_to_turn_off_url: None,
            },
            Training::Unknown => TrainingFact::Unknown,
        },
        retention: match policy.retention {
            _ if elsewhere => RetentionFact::ProviderTerms,
            Retention::NotStored => RetentionFact::NotStored,
            Retention::StoredDays(days) => RetentionFact::StoredDays { days },
            Retention::ProviderTerms => RetentionFact::ProviderTerms,
            Retention::OnDevice => RetentionFact::OnDevice,
        },
        admin_visibility: AdminVisibility::No,
        min_age: policy.min_age,
        guardian_permission: policy.guardian_permission,
        cost: if on_device {
            CostKind::FreeOnDevice
        } else if elsewhere {
            CostKind::SelfHosted
        } else {
            CostKind::ApiBilling
        },
        on_device,
        location: policy.location.clone(),
    };
    with_version(facts)
}

/// The disclosure for a model that a server on this computer may run in the cloud: PageLamp can't
/// confirm where it runs, what is kept or whether it is trained on, and doesn't know its price.
pub(crate) fn cloud_via_local_disclosure(provider: &ProviderProfile) -> DisclosureFacts {
    let mut facts = disclosure_for(provider);
    facts.training = TrainingFact::Unknown;
    facts.retention = RetentionFact::ProviderTerms;
    facts.cost = CostKind::CloudViaLocal;
    facts.on_device = false;
    with_version(facts)
}

/// `facts` with `version` set to a hash of everything else and of the fixed wording.
pub(crate) fn with_version(mut facts: DisclosureFacts) -> DisclosureFacts {
    facts.version = 0;
    let serialized = serde_json::to_string(&facts).unwrap_or_default();
    let digest = sha256_hex(format!("{DISCLOSURE_WORDING_VERSION}:{serialized}").as_bytes());
    facts.version = u32::from_str_radix(&digest[..8], 16).unwrap_or(0);
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_carry_their_data_policy_and_a_stable_version() {
        let presets: Vec<ProviderPreset> = profile::presets().iter().map(provider_preset).collect();
        let openai = presets.iter().find(|p| p.id == "openai").unwrap();
        assert!(openai.needs_key && !openai.local);
        assert_eq!(
            openai.data_policy.retention,
            RetentionFact::StoredDays { days: 30 }
        );
        assert_eq!(openai.data_policy.min_age, Some(13));
        assert_eq!(openai.data_policy.cost, CostKind::ApiBilling);
        let gemini = presets.iter().find(|p| p.id == "gemini").unwrap();
        assert_eq!(gemini.data_policy.training, TrainingFact::MayTrainFreeTier);
        assert_eq!(gemini.data_policy.min_age, Some(18));
        let ollama = presets.iter().find(|p| p.id == "ollama").unwrap();
        assert!(!ollama.needs_key && ollama.local && ollama.data_policy.on_device);
        assert_eq!(ollama.data_policy.cost, CostKind::FreeOnDevice);
        // The version is a hash: the same facts give the same number, others another.
        let again = disclosure_for(profile::preset("openai").unwrap());
        assert_eq!(again.version, openai.data_policy.version);
        assert_ne!(openai.data_policy.version, gemini.data_policy.version);
        assert_ne!(openai.data_policy.version, 0);
    }
}
