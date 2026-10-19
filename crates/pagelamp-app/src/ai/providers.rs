//! API-key and local providers: added, checked, used and removed. Keys live only in the
//! keychain (`llm:<provider_id>`); the database holds the rest.

use chrono::{SubsecRound, Utc};
use pagelamp_core::ai::{BlockReason, ProviderRow};
use pagelamp_core::ingest::sha256_hex;
use pagelamp_core::store::Store;
use pagelamp_llm::profile::{self, ApiKey, Auth, EndpointCheck, ProviderProfile};
use pagelamp_llm::{Backend, CancellationToken, HttpDriver, check_base_url};

use super::{BackendRef, ModelProviderRecord, provider_wire};
use crate::{App, AppError, AppErrorKind, Result};

/// The keychain account of a provider's key.
pub(crate) fn key_account(provider_id: &str) -> String {
    format!("llm:{provider_id}")
}

/// A provider ready to use: its row, its profile (with its own base URL) and its key.
pub(crate) struct Provider {
    pub row: ProviderRow,
    pub profile: ProviderProfile,
    pub key: Option<ApiKey>,
}

impl Provider {
    /// The driver for this provider (refuses coding-plan endpoints and keys again).
    pub fn backend(self) -> Result<Backend> {
        Ok(Backend::Http(HttpDriver::new(self.profile, self.key)?))
    }

    /// Whether this provider can't be used without a key.
    pub fn needs_key(&self) -> bool {
        self.profile.auth != Auth::None && !key_optional(&self.profile)
    }
}

/// A custom server on this computer may take no key (many local servers don't), so it isn't
/// set up as a keyless preset that PageLamp trusts to run its models here.
pub(crate) fn key_optional(profile: &ProviderProfile) -> bool {
    profile.id == "custom" && profile.on_device()
}

impl App {
    /// Add a provider (see `ai::App::add_model_provider`).
    pub(crate) async fn add_provider(
        &self,
        preset_id: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<ModelProviderRecord> {
        let preset = profile::preset(preset_id).ok_or_else(|| {
            AppError::new(
                AppErrorKind::NotFound,
                format!("unknown provider type '{preset_id}'"),
            )
        })?;
        // Everything below up to the model list is local: bad input never reaches the network.
        let url = match (
            base_url.map(str::trim).filter(|u| !u.is_empty()),
            &preset.base_url,
        ) {
            (Some(url), _) if preset.base_url_editable => check_base_url(url)
                .map_err(|problem| AppError::new(AppErrorKind::Invalid, problem.to_string()))?,
            (_, Some(default)) => default.clone(),
            (_, None) => {
                return Err(AppError::new(
                    AppErrorKind::Invalid,
                    "Enter the provider's address (it starts with https://).",
                ));
            }
        };
        let profile = preset.with_base_url(url.clone());
        let key = if profile.auth == Auth::None {
            None
        } else {
            let key = api_key.map(str::trim).filter(|k| !k.is_empty());
            if key.is_none() && !key_optional(&profile) {
                return Err(AppError::new(AppErrorKind::Invalid, "Enter the API key."));
            }
            key.map(ApiKey::new)
        };
        refuse_coding_plans(&profile, key.as_ref())?;
        let provider_id = provider_id(preset_id, &url);
        if self.read_store()?.model_provider(&provider_id)?.is_some() {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                format!(
                    "{} is already set up; replace its key instead.",
                    profile.display_name
                ),
            ));
        }
        // A free model-list call checks the address and the key before anything is saved.
        let last4 = key.as_ref().map(ApiKey::last4);
        let raw_key = api_key.map(|k| k.trim().to_string());
        check_access(profile.clone(), key).await?;
        let row = ProviderRow {
            id: provider_id.clone(),
            preset: preset_id.to_string(),
            label: profile.display_name.clone(),
            wire: profile.wire.as_str().to_string(),
            base_url: url.as_str().trim_end_matches('/').to_string(),
            created_at: Utc::now().trunc_subsecs(0),
            last_probe_json: None,
        };
        // A custom server here added without a key: none of its is ever in the keychain.
        let keyless = profile.auth != Auth::None && last4.is_none();
        if let (Some(raw), true) = (
            raw_key.as_ref().filter(|raw| !raw.is_empty()),
            profile.auth != Auth::None,
        ) {
            self.secrets.set(&key_account(&provider_id), raw)?;
        } else if keyless {
            // An entry an earlier provider at this address left is never read for this one
            // (`KEYLESS`), but it goes now: only a keychain that can't be reached at all may
            // keep it; one that holds it and won't delete it stops the add.
            let account = key_account(&provider_id);
            if let Err(err) = self.secrets.delete(&account)
                && matches!(self.secrets.get(&account), Ok(Some(_)))
            {
                let mut err: AppError = err.into();
                err.message = format!(
                    "An earlier API key for this address is in the keychain, under \
                     `{account}`, and couldn't be deleted ({}). Nothing was added; delete it \
                     there, or try again when the keychain can be reached.",
                    err.message
                );
                return Err(err);
            }
        }
        // The provider and whether it is keyless, together.
        let inserted = self.write_store()?.in_transaction(|store| {
            store.insert_model_provider(&row)?;
            let mut ids = super::settings::keyless(store)?;
            let changed = if keyless {
                ids.insert(provider_id.clone())
            } else {
                ids.remove(&provider_id)
            };
            if changed {
                store.set_setting(super::settings::KEYLESS, &ids)?;
            }
            Ok(())
        });
        if let Err(err) = inserted {
            if !keyless {
                let _ = self.secrets.delete(&key_account(&provider_id));
            }
            return Err(err.into());
        }
        Ok(record(&row, &profile, last4))
    }

    /// Replace a provider's key (checked with a free model-list call first).
    pub(crate) async fn replace_provider_key(
        &self,
        provider_id: &str,
        api_key: &str,
    ) -> Result<ModelProviderRecord> {
        let provider = self.provider(provider_id)?;
        if provider.profile.auth == Auth::None {
            return Err(AppError::new(
                AppErrorKind::Invalid,
                format!("{} doesn't use a key.", provider.profile.display_name),
            ));
        }
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            return Err(AppError::new(AppErrorKind::Invalid, "Enter the API key."));
        }
        let key = ApiKey::new(trimmed);
        refuse_coding_plans(&provider.profile, Some(&key))?;
        let last4 = key.last4();
        check_access(provider.profile.clone(), Some(key)).await?;
        // No longer keyless before the key is stored (so a removal always looks for it); put
        // back if the keychain doesn't take the key.
        let store = self.write_store()?;
        let mut ids = super::settings::keyless(&store)?;
        let was_keyless = ids.remove(provider_id);
        if was_keyless {
            store.set_setting(super::settings::KEYLESS, &ids)?;
        }
        if let Err(err) = self.secrets.set(&key_account(provider_id), trimmed) {
            if was_keyless {
                ids.insert(provider_id.to_string());
                store.set_setting(super::settings::KEYLESS, &ids)?;
            }
            return Err(err.into());
        }
        Ok(record(&provider.row, &provider.profile, Some(last4)))
    }

    /// Remove a provider: its row, its key and every setting that refers to it.
    pub(crate) fn delete_provider(&self, provider_id: &str) -> Result<()> {
        let store = self.write_store()?;
        let row = store
            .model_provider(provider_id)?
            .ok_or_else(|| unknown_provider(provider_id))?;
        // The key first: if it can't be deleted, nothing changes and the removal can be tried
        // again. Then the row and every setting that refers to it, together.
        let keyless = super::settings::keyless(&store)?.contains(provider_id);
        forget_key(&*self.secrets, &row, keyless)?;
        store.in_transaction(|store| {
            store.delete_model_provider(provider_id)?;
            super::settings::forget_backend(
                store,
                &BackendRef::Provider {
                    provider_id: provider_id.to_string(),
                },
            )
        })?;
        Ok(())
    }

    /// A stored provider with its key (`NotFound` if there is none; the key may be missing).
    pub(crate) fn provider(&self, provider_id: &str) -> Result<Provider> {
        let row = self.provider_row(provider_id)?;
        let keyless = super::settings::keyless(&self.read_store()?)?.contains(provider_id);
        provider_from_row(row, &*self.secrets, keyless)
    }

    /// The provider behind `backend`, with its key: for calls that reach the provider
    /// (Codex and Claude Code arrive with M2 and M4).
    pub(crate) fn provider_for(&self, backend: &BackendRef) -> Result<Provider> {
        self.provider(provider_id_of(backend)?)
    }

    /// The provider behind `backend` without its key: for local work (routing, acknowledgements,
    /// estimates), which never reads the keychain.
    pub(crate) fn provider_profile(&self, backend: &BackendRef) -> Result<ProviderProfile> {
        profile_from_row(&self.provider_row(provider_id_of(backend)?)?)
    }

    fn provider_row(&self, provider_id: &str) -> Result<ProviderRow> {
        self.read_store()?
            .model_provider(provider_id)?
            .ok_or_else(|| unknown_provider(provider_id))
    }

    /// Every provider as the settings page shows it.
    pub(crate) fn provider_records(
        &self,
        store: &Store,
    ) -> Result<Vec<(ModelProviderRecord, Provider)>> {
        let mut records = Vec::new();
        let keyless = super::settings::keyless(store)?;
        for row in store.model_providers()? {
            let keyless = keyless.contains(&row.id);
            let provider = provider_from_row(row, &*self.secrets, keyless)?;
            let last4 = provider.key.as_ref().map(ApiKey::last4);
            records.push((record(&provider.row, &provider.profile, last4), provider));
        }
        Ok(records)
    }
}

/// The id of a provider backend (Codex and Claude Code arrive with M2 and M4).
fn provider_id_of(backend: &BackendRef) -> Result<&str> {
    match backend {
        BackendRef::Provider { provider_id } => Ok(provider_id),
        BackendRef::Codex | BackendRef::ClaudeCode => Err(AppError::blocked(
            BlockReason::BackendDisabledInThisBuild,
            "This way of using a model isn't available in this build yet.",
        )),
    }
}

pub(crate) fn profile_from_row(row: &ProviderRow) -> Result<ProviderProfile> {
    let preset = profile::preset(&row.preset).ok_or_else(|| {
        AppError::new(
            AppErrorKind::Internal,
            format!("provider '{}' has an unknown type '{}'", row.id, row.preset),
        )
    })?;
    let url = check_base_url(&row.base_url)
        .map_err(|problem| AppError::new(AppErrorKind::Invalid, problem.to_string()))?;
    Ok(preset.with_base_url(url))
}

/// Whether a provider keeps a key in the keychain (a row of an unknown type is assumed to).
pub(crate) fn uses_key(row: &ProviderRow) -> bool {
    profile::preset(&row.preset).is_none_or(|preset| preset.auth != Auth::None)
}

/// `keyless`: added without a key and never given one (`KEYLESS`): the keychain isn't read.
fn provider_from_row(
    row: ProviderRow,
    secrets: &dyn pagelamp_core::secrets::SecretBackend,
    keyless: bool,
) -> Result<Provider> {
    let profile = profile_from_row(&row)?;
    let key = if profile.auth == Auth::None || keyless {
        None
    } else {
        match secrets.get(&key_account(&row.id)) {
            Ok(key) => key.map(|k| ApiKey::new(&k)),
            // A custom server here may take no key: where the keychain can't be read, it is
            // used without one (a server that wants one says so).
            Err(err) if key_optional(&profile) => {
                tracing::warn!(target: "pagelamp::ai", "no keychain for a keyless server: {err}");
                None
            }
            Err(err) => return Err(err.into()),
        }
    };
    Ok(Provider { row, profile, key })
}

/// Delete a provider's key from the keychain. A failed delete counts only when no key is
/// provably left: the keychain then says there is no such entry, or, for a provider added
/// without a key (`keyless`), the keychain can't be reached at all. Otherwise the error says
/// the key is still there, and the caller changes nothing.
pub(crate) fn forget_key(
    secrets: &dyn pagelamp_core::secrets::SecretBackend,
    row: &ProviderRow,
    keyless: bool,
) -> Result<()> {
    if !uses_key(row) {
        return Ok(());
    }
    let account = key_account(&row.id);
    let Err(err) = secrets.delete(&account) else {
        return Ok(());
    };
    match secrets.get(&account) {
        Ok(None) => return Ok(()),
        Err(_) if keyless => return Ok(()),
        _ => {}
    }
    let mut err: AppError = err.into();
    err.message = format!(
        "The API key of {} couldn't be deleted from the keychain ({}): it is still there, under \
         `{account}`. Nothing was removed; try again when the keychain can be reached.",
        row.label, err.message
    );
    Err(err)
}

/// Refuse coding-plan endpoints and keys, quoting the vendor (before any network call).
fn refuse_coding_plans(profile: &ProviderProfile, key: Option<&ApiKey>) -> Result<()> {
    let Some(url) = &profile.base_url else {
        return Ok(());
    };
    match profile::check_endpoint(url, key) {
        EndpointCheck::Blocked {
            vendor, sentence, ..
        } => Err(AppError::blocked(
            BlockReason::CodingPlanKey,
            format!("{vendor}: “{sentence}”"),
        )),
        EndpointCheck::Warn { vendor, .. } => {
            tracing::info!(target: "pagelamp::ai", "provider added with a {vendor} key (caution)");
            Ok(())
        }
        EndpointCheck::Allowed => Ok(()),
    }
}

/// A free model-list call: the address answers and the key is accepted.
async fn check_access(profile: ProviderProfile, key: Option<ApiKey>) -> Result<()> {
    let backend = Backend::Http(HttpDriver::new(profile, key)?);
    backend.list_models(&CancellationToken::new()).await?;
    Ok(())
}

/// The preset's id, or for a custom endpoint (or a second local server) one derived from its
/// address.
fn provider_id(preset_id: &str, url: &url::Url) -> String {
    let default = profile::preset(preset_id).and_then(|p| p.base_url.clone());
    if preset_id != "custom" && default.as_ref() == Some(url) {
        return preset_id.to_string();
    }
    let hash = sha256_hex(url.as_str().trim_end_matches('/').as_bytes());
    format!("{preset_id}-{}", &hash[..8])
}

fn record(
    row: &ProviderRow,
    profile: &ProviderProfile,
    key_last4: Option<String>,
) -> ModelProviderRecord {
    ModelProviderRecord {
        provider_id: row.id.clone(),
        preset: row.preset.clone(),
        label: row.label.clone(),
        wire: provider_wire(profile.wire),
        base_url: row.base_url.clone(),
        key_last4,
        on_device: profile.on_device(),
        created_at: row.created_at,
    }
}

fn unknown_provider(provider_id: &str) -> AppError {
    AppError::new(
        AppErrorKind::NotFound,
        format!("no model provider '{provider_id}' is set up"),
    )
}
