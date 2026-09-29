//! PageLamp's own model calls, as the facade offers them (v0.3 M1; design §3.8). The logic lives
//! here, so the Tauri app, the Swift shell and the CLI can't diverge (rule 12): disclosure facts,
//! policy questions, budgets and estimates are computed in Rust, and the UIs only render them.
//!
//! Model code is in `pagelamp-llm` (drivers) and `pagelamp_core::ai_gate` (the only producer of
//! prompt text).

mod types;

pub use types::*;

use std::time::Duration;

use chrono::NaiveDate;
use pagelamp_core::ai::{AiFeature, MaterialSharing};
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

    // ----- to be implemented in M1 (schema v4 and the facade logic) ------------------------------

    /// Providers, feature routing, backends with their disclosure facts, and the budget.
    pub fn ai_status(&self) -> Result<AiStatus> {
        Err(not_yet("AI status"))
    }

    /// Add an API-key or local provider: checks the address (HTTPS unless this computer), refuses
    /// coding-plan endpoints and keys, validates the key with a free model-list call, and keeps
    /// the key in the keychain (`llm:<provider_id>`).
    pub async fn add_model_provider(
        &self,
        _preset: &str,
        _base_url: Option<&str>,
        _api_key: Option<&str>,
    ) -> Result<ModelProviderRecord> {
        Err(not_yet("Adding a model provider"))
    }

    pub async fn update_model_provider_key(
        &self,
        _provider_id: &str,
        _api_key: &str,
    ) -> Result<ModelProviderRecord> {
        Err(not_yet("Replacing a key"))
    }

    /// Remove a provider: its row, its keychain entry and every feature routed to it.
    pub fn remove_model_provider(&self, _provider_id: &str) -> Result<()> {
        Err(not_yet("Removing a model provider"))
    }

    /// The models a backend offers (a live list merged with PageLamp's price list).
    pub async fn list_models(&self, _backend: &BackendRef) -> Result<Vec<ModelInfo>> {
        Err(not_yet("Listing models"))
    }

    /// "Test": a tiny request with no course data.
    pub async fn test_model(&self, _backend: &BackendRef, _model: &str) -> Result<ProbeReport> {
        Err(not_yet("Testing a model"))
    }

    pub fn set_feature_model(
        &self,
        _feature: AiFeature,
        _choice: Option<ModelChoice>,
    ) -> Result<()> {
        Err(not_yet("Choosing a model"))
    }

    pub fn acknowledge_ai_disclosure(&self, _backend: &BackendRef, _version: u32) -> Result<()> {
        Err(not_yet("Acknowledging a disclosure"))
    }

    /// "The budget is not enforced for this model": needed before the first run of a model
    /// without a known price.
    pub fn acknowledge_unpriced_model(&self, _backend: &BackendRef, _model: &str) -> Result<()> {
        Err(not_yet("Acknowledging an unpriced model"))
    }

    /// The monthly soft cap for API keys, in micro-USD (`None`: no cap).
    pub fn set_monthly_budget(&self, _micro_usd: Option<u64>) -> Result<()> {
        Err(not_yet("Setting the budget"))
    }

    /// "≈ $x" before Generate (an upper bound), and what would block the run. Local and cheap:
    /// no network call.
    pub fn estimate_generation(&self, _request: &EstimateRequest) -> Result<CostEstimate> {
        Err(not_yet("Estimating a run"))
    }

    pub fn usage_summary(&self, _month: Option<NaiveDate>) -> Result<UsageSummary> {
        Err(not_yet("Usage"))
    }

    /// The student's answer to "may this course's materials be shared with an AI service?".
    pub fn set_course_material_sharing(
        &self,
        _course: &str,
        _answer: MaterialSharing,
    ) -> Result<()> {
        Err(not_yet("Answering the materials question"))
    }

    /// Keys, providers, generations, the usage ledger, AI settings and the pre-update backup.
    pub fn remove_all_ai_data(&self) -> Result<RemoveAiDataReport> {
        Err(not_yet("Removing AI data"))
    }
}

fn not_yet(what: &str) -> AppError {
    AppError::new(
        AppErrorKind::Internal,
        format!("{what} is not available in this build yet."),
    )
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

/// The disclosure facts of an API-key or local provider.
pub(crate) fn disclosure_for(provider: &ProviderProfile) -> DisclosureFacts {
    let policy = &provider.data_policy;
    let on_device = provider.local || provider.on_device();
    let facts = DisclosureFacts {
        version: 0,
        sends: vec![SentData::Structure, SentData::MaterialText],
        recipient: Recipient {
            name: provider.display_name.clone(),
            terms_url: policy.terms_url.clone(),
        },
        training: match policy.training {
            Training::NoTraining => TrainingFact::NoTraining,
            Training::MayTrainFreeTier => TrainingFact::MayTrainFreeTier,
            Training::MayTrain => TrainingFact::MayTrain {
                how_to_turn_off_url: None,
            },
            Training::Unknown => TrainingFact::Unknown,
        },
        retention: match policy.retention {
            Retention::NotStored => RetentionFact::NotStored,
            Retention::StoredDays(days) => RetentionFact::StoredDays { days },
            Retention::ProviderTerms => RetentionFact::ProviderTerms,
            Retention::OnDevice => RetentionFact::OnDevice,
        },
        admin_visibility: false,
        min_age: policy.min_age,
        guardian_permission: policy.guardian_permission,
        cost: if on_device {
            CostKind::FreeOnDevice
        } else {
            CostKind::ApiBilling
        },
        on_device,
        location: policy.location.clone(),
    };
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
