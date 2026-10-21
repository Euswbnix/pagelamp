//! AI settings in the `settings` table (JSON by key). Everything here is the student's choice;
//! no key or content is ever stored.

use std::collections::{BTreeMap, BTreeSet};

use pagelamp_core::ai::AiFeature;
use pagelamp_core::model::Timestamp;
use pagelamp_core::store::Store;
use serde::{Deserialize, Serialize};

use super::{BackendRef, DEFAULT_MONTHLY_BUDGET_MICRO_USD, ModelChoice};
use crate::Result;

/// Which model each feature uses.
pub(crate) const ROUTING: &str = "ai.routing";
/// The monthly budget (`BudgetSetting`).
pub(crate) const BUDGET: &str = "ai.budget";
/// Disclosures acknowledged, by backend key.
pub(crate) const DISCLOSURES: &str = "ai.disclosures";
/// Models without a known price the student accepted, by backend key.
pub(crate) const UNPRICED: &str = "ai.unpriced_acknowledged";
/// Which Codex runs (`CodexSource`; absent: managed).
pub(crate) const CODEX_SOURCE: &str = "ai.codex_source";
/// Mode A's weekly run cap (`WeeklyCap`; absent: the default).
pub(crate) const MODE_A_WEEKLY_CAP: &str = "ai.mode_a_weekly_cap";
/// The last "requires a newer version of Codex" seen, and with which version.
pub(crate) const CODEX_OUTDATED: &str = "ai.codex_outdated";
/// From the last model list of a model server on this computer that may run models in the
/// cloud (Ollama), by backend key: which models it runs here (`true`) and which it runs in the
/// cloud (`false`). Estimates read it; a run asks the server again.
pub(crate) const LISTED_MODELS: &str = "ai.listed_models";
/// Providers added without a key (a custom server on this computer) that never got one: their
/// keychain entry is never read, so an old entry under the same id can't come back. A removal
/// still deletes any entry, and accepts a keychain that can't be reached at all.
pub(crate) const KEYLESS: &str = "ai.keyless_providers";

/// Every AI settings key ("Remove all AI data" deletes them).
pub(crate) const ALL_KEYS: [&str; 9] = [
    ROUTING,
    BUDGET,
    DISCLOSURES,
    UNPRICED,
    CODEX_SOURCE,
    MODE_A_WEEKLY_CAP,
    CODEX_OUTDATED,
    LISTED_MODELS,
    KEYLESS,
];

/// A stable key for a backend in settings maps.
pub(crate) fn backend_key(backend: &BackendRef) -> String {
    match backend {
        BackendRef::Codex => "codex".to_string(),
        BackendRef::ClaudeCode => "claude_code".to_string(),
        BackendRef::Provider { provider_id } => format!("provider:{provider_id}"),
    }
}

/// The key of the disclosure for a model that a server on this computer may run in the cloud:
/// acknowledged apart from the server's own ("nothing leaves this computer").
pub(crate) fn cloud_key(backend: &BackendRef) -> String {
    format!("{}:cloud", backend_key(backend))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Routing(pub BTreeMap<AiFeature, ModelChoice>);

/// Absent: the default cap (US$5); `monthly_micro_usd: None`: no cap.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct BudgetSetting {
    pub monthly_micro_usd: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Acknowledgement {
    pub version: u32,
    pub at: Timestamp,
}

/// Absent: the default cap; `runs: None`: no cap.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct WeeklyCap {
    pub runs: Option<u32>,
}

/// A `runtime_outdated` error, and the Codex version that gave it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OutdatedSeen {
    pub version: String,
    pub at: Timestamp,
}

pub(crate) fn routing(store: &Store) -> pagelamp_core::Result<Routing> {
    Ok(store.setting(ROUTING)?.unwrap_or_default())
}

pub(crate) fn monthly_budget(store: &Store) -> Result<Option<u64>> {
    Ok(store
        .setting::<BudgetSetting>(BUDGET)?
        .map_or(Some(DEFAULT_MONTHLY_BUDGET_MICRO_USD), |b| {
            b.monthly_micro_usd
        }))
}

pub(crate) fn disclosures(
    store: &Store,
) -> pagelamp_core::Result<BTreeMap<String, Acknowledgement>> {
    Ok(store.setting(DISCLOSURES)?.unwrap_or_default())
}

pub(crate) fn unpriced(store: &Store) -> pagelamp_core::Result<BTreeMap<String, Vec<String>>> {
    Ok(store.setting(UNPRICED)?.unwrap_or_default())
}

/// Ollama's last model lists: backend key → model id → it runs on this computer.
pub(crate) type ListedModels = BTreeMap<String, BTreeMap<String, bool>>;

pub(crate) fn listed_models(store: &Store) -> pagelamp_core::Result<ListedModels> {
    Ok(store.setting(LISTED_MODELS)?.unwrap_or_default())
}

/// The ids of `KEYLESS` providers.
pub(crate) fn keyless(store: &Store) -> pagelamp_core::Result<BTreeSet<String>> {
    Ok(store.setting(KEYLESS)?.unwrap_or_default())
}

/// Forget everything about a removed backend: its routing, acknowledgements.
pub(crate) fn forget_backend(store: &Store, backend: &BackendRef) -> pagelamp_core::Result<()> {
    let key = backend_key(backend);
    let mut routes = routing(store)?;
    routes.0.retain(|_, choice| &choice.backend != backend);
    store.set_setting(ROUTING, &routes)?;
    let mut acks = disclosures(store)?;
    acks.remove(&key);
    acks.remove(&cloud_key(backend));
    store.set_setting(DISCLOSURES, &acks)?;
    let mut models = unpriced(store)?;
    models.remove(&key);
    store.set_setting(UNPRICED, &models)?;
    let mut listed = listed_models(store)?;
    if listed.remove(&key).is_some() {
        store.set_setting(LISTED_MODELS, &listed)?;
    }
    if let BackendRef::Provider { provider_id } = backend {
        let mut ids = keyless(store)?;
        if ids.remove(provider_id) {
            store.set_setting(KEYLESS, &ids)?;
        }
    }
    Ok(())
}
