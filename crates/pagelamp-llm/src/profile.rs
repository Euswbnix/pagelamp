//! Provider profiles: data, not code. The presets (`data/presets.toml`) and the coding-plan block
//! list (`data/blocklist.toml`) are embedded at build time; a "custom" provider is a preset with
//! the student's base URL.

use std::sync::LazyLock;

use serde::Deserialize;
use url::Url;

use crate::client::{check_base_url, is_loopback};

/// The request dialect a provider speaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// OpenAI Responses (`POST /responses`).
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    /// OpenAI-compatible Chat Completions (`POST /chat/completions`): OpenRouter, Gemini's
    /// compatible endpoint, LM Studio, llama.cpp, custom endpoints.
    #[serde(rename = "openai_chat")]
    OpenAiChat,
    /// Anthropic Messages (`POST /v1/messages`).
    AnthropicMessages,
    /// Ollama's own API (`POST /api/chat`): `num_ctx`, `think`, `format`.
    OllamaNative,
}

impl Wire {
    pub fn as_str(self) -> &'static str {
        match self {
            Wire::OpenAiResponses => "openai_responses",
            Wire::OpenAiChat => "openai_chat",
            Wire::AnthropicMessages => "anthropic_messages",
            Wire::OllamaNative => "ollama_native",
        }
    }
}

/// How the key is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Auth {
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// `x-api-key: <key>` (Anthropic).
    XApiKey,
    /// No key (local servers).
    None,
}

/// A provider API key. Its `Debug` shows only the last 4 characters; there is no `Display`,
/// `Serialize` or `Clone`, so it can't end up in logs, JSON or copies by accident.
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: &str) -> ApiKey {
        ApiKey(key.trim().to_string())
    }

    /// The key itself, for the one header that carries it.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// The last 4 characters, for display.
    pub fn last4(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    }

    pub(crate) fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ApiKey(…{})", self.last4())
    }
}

/// Differences between servers of one wire that change the request.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Quirks {
    /// Structured output: this server knows `json_object` but not JSON schemas.
    pub json_object_only: bool,
    /// Structured output: no JSON mode at all (the schema goes into the prompt).
    pub no_json_mode: bool,
    /// How effort is sent on the Chat Completions wire.
    pub effort_param: EffortParam,
    /// Don't send `stream_options: {include_usage: true}` (Chat Completions).
    pub no_stream_usage: bool,
    /// Extra JSON merged into every request body (e.g. OpenRouter's privacy routing).
    pub extra_body: Option<toml::Table>,
    /// Extra headers, `{product}` replaced by the product name.
    pub extra_headers: Vec<(String, String)>,
    /// Per-model differences, first matching pattern wins.
    pub models: Vec<ModelQuirks>,
}

/// How the Chat Completions wire sends effort.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffortParam {
    /// Not sent (servers that reject unknown fields).
    #[default]
    None,
    /// `reasoning_effort: "low"` (OpenAI-compatible, Gemini).
    ReasoningEffort,
    /// `reasoning: {effort: "low"}` (OpenRouter).
    ReasoningObject,
}

/// Differences of the models whose id matches `pattern` (`*` matches any run of characters).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelQuirks {
    pub pattern: String,
    /// Thinking can't be turned off: "lowest" costs a reasoning allowance, and the UI says so.
    pub thinking_always_on: bool,
    /// The model takes no effort setting at all.
    pub no_effort: bool,
    /// The wire value for `Effort::Lowest` when it isn't the preset's default.
    pub lowest_effort: Option<String>,
}

impl Quirks {
    /// The quirks of `model` (defaults when no pattern matches).
    pub fn for_model(&self, model: &str) -> ModelQuirks {
        self.models
            .iter()
            .find(|quirks| glob_match(&quirks.pattern, model))
            .cloned()
            .unwrap_or_default()
    }
}

/// Whether a provider trains on inputs (codes; the UIs translate them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Training {
    NoTraining,
    /// The free tier may train on inputs (and humans may review them); the paid tier doesn't.
    MayTrainFreeTier,
    MayTrain,
    Unknown,
}

/// How long a provider keeps inputs and outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    NotStored,
    /// Kept for this many days (e.g. abuse monitoring).
    StoredDays(u32),
    /// See the provider's terms.
    ProviderTerms,
    OnDevice,
}

/// The data-policy line of a provider (shown at setup and in the disclosure, Canvas §2E).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataPolicy {
    pub training: Training,
    pub retention: Retention,
    /// Minimum age in the provider's terms, if known.
    pub min_age: Option<u8>,
    /// Under 18 needs a parent's or guardian's permission.
    #[serde(default)]
    pub guardian_permission: bool,
    /// Where data is processed (ISO country code), if known.
    pub location: Option<String>,
    pub terms_url: Option<String>,
}

/// One provider as PageLamp talks to it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProfile {
    /// Preset id (`openai`, `anthropic`, `gemini`, `openrouter`, `ollama`, `lm_studio`, `custom`).
    pub id: String,
    pub display_name: String,
    pub wire: Wire,
    /// Empty for `custom` (the student gives it).
    #[serde(default, deserialize_with = "optional_url")]
    pub base_url: Option<Url>,
    #[serde(default)]
    pub base_url_editable: bool,
    pub auth: Auth,
    /// Display-only hints ("keys start with sk-"); never used to pick a vendor.
    #[serde(default)]
    pub key_hints: Vec<String>,
    /// Runs on this computer when its base URL is loopback (Ollama, LM Studio).
    #[serde(default)]
    pub local: bool,
    pub data_policy: DataPolicy,
    #[serde(default)]
    pub quirks: Quirks,
}

impl ProviderProfile {
    /// A copy of this preset talking to `base_url` instead.
    pub fn with_base_url(&self, base_url: Url) -> ProviderProfile {
        ProviderProfile {
            base_url: Some(base_url),
            ..self.clone()
        }
    }

    /// Whether requests stay on this computer (a loopback base URL). Ollama cloud models are
    /// checked per model (`remote_host`, `-cloud`) when listing models.
    pub fn on_device(&self) -> bool {
        self.base_url.as_ref().is_some_and(is_loopback)
    }
}

fn optional_url<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Url>, D::Error> {
    let raw: Option<String> = Option::deserialize(deserializer)?;
    raw.map(|raw| check_base_url(&raw).map_err(serde::de::Error::custom))
        .transpose()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetFile {
    preset: Vec<ProviderProfile>,
}

static PRESETS: LazyLock<Vec<ProviderProfile>> = LazyLock::new(|| {
    toml::from_str::<PresetFile>(include_str!("../data/presets.toml"))
        .expect("data/presets.toml is valid (tested)")
        .preset
});

/// The shipped presets, in display order.
pub fn presets() -> &'static [ProviderProfile] {
    &PRESETS
}

pub fn preset(id: &str) -> Option<&'static ProviderProfile> {
    presets().iter().find(|preset| preset.id == id)
}

// ----- coding-plan block list ------------------------------------------------------------------

/// One vendor's coding plan.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockRule {
    vendor: String,
    /// Hosts whose paths under `path_prefixes` belong to the plan.
    #[serde(default)]
    hosts: Vec<String>,
    /// `[""]` blocks the whole host.
    #[serde(default)]
    path_prefixes: Vec<String>,
    #[serde(default)]
    key_prefixes: Vec<String>,
    /// Refuse (true) or only warn (false, terms not checked yet).
    block: bool,
    /// The vendor's own words, shown as a quote.
    sentence: String,
    source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockFile {
    rule: Vec<BlockRule>,
}

static BLOCK_RULES: LazyLock<Vec<BlockRule>> = LazyLock::new(|| {
    toml::from_str::<BlockFile>(include_str!("../data/blocklist.toml"))
        .expect("data/blocklist.toml is valid (tested)")
        .rule
});

/// What the block list says about an endpoint and key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EndpointCheck {
    Allowed,
    /// Allowed, with a caution to show (terms not checked yet).
    Warn {
        vendor: String,
        sentence: String,
    },
    /// A coding plan whose terms forbid use from other applications.
    Blocked {
        vendor: String,
        sentence: String,
        source: String,
    },
}

/// Check a base URL and key against the coding-plan block list.
pub fn check_endpoint(base_url: &Url, key: Option<&ApiKey>) -> EndpointCheck {
    let host = base_url.host_str().unwrap_or_default().to_ascii_lowercase();
    let path = base_url.path();
    let mut warning = None;
    for rule in BLOCK_RULES.iter() {
        let by_endpoint = rule.hosts.iter().any(|h| h.eq_ignore_ascii_case(&host))
            && rule
                .path_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix.as_str()));
        let by_key = key.is_some_and(|key| rule.key_prefixes.iter().any(|p| key.starts_with(p)));
        if !(by_endpoint || by_key) {
            continue;
        }
        if rule.block {
            return EndpointCheck::Blocked {
                vendor: rule.vendor.clone(),
                sentence: rule.sentence.clone(),
                source: rule.source.clone(),
            };
        }
        warning.get_or_insert(EndpointCheck::Warn {
            vendor: rule.vendor.clone(),
            sentence: rule.sentence.clone(),
        });
    }
    warning.unwrap_or(EndpointCheck::Allowed)
}

/// `*` matches any run of characters; everything else literally (ASCII case-insensitive).
fn glob_match(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.to_ascii_lowercase(), text.to_ascii_lowercase());
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text.as_str();
    for (index, part) in parts.iter().enumerate() {
        if index == 0 {
            let Some(after) = rest.strip_prefix(part) else {
                return false;
            };
            rest = after;
        } else if index == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            let Some(at) = rest.find(part) else {
                return false;
            };
            rest = &rest[at + part.len()..];
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_in_data_match_as_str() {
        for wire in [
            Wire::OpenAiResponses,
            Wire::OpenAiChat,
            Wire::AnthropicMessages,
            Wire::OllamaNative,
        ] {
            let parsed: Wire = serde_json::from_value(serde_json::json!(wire.as_str())).unwrap();
            assert_eq!(parsed, wire);
        }
    }

    #[test]
    fn the_presets_load_in_order_and_are_complete() {
        let ids: Vec<&str> = presets().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "openai",
                "anthropic",
                "gemini",
                "openrouter",
                "ollama",
                "lm_studio",
                "custom"
            ]
        );
        for preset in presets() {
            assert_eq!(
                preset.base_url.is_none(),
                preset.id == "custom",
                "{}",
                preset.id
            );
            assert_eq!(preset.local, preset.on_device(), "{}", preset.id);
            if preset.local {
                assert_eq!(preset.auth, Auth::None);
                assert_eq!(preset.data_policy.retention, Retention::OnDevice);
            }
        }
        assert_eq!(preset("anthropic").unwrap().wire, Wire::AnthropicMessages);
        assert_eq!(preset("openai").unwrap().wire, Wire::OpenAiResponses);
        assert_eq!(preset("ollama").unwrap().wire, Wire::OllamaNative);
        assert_eq!(preset("lm_studio").unwrap().wire, Wire::OpenAiChat);
    }

    #[test]
    fn openrouter_denies_data_collection_and_names_the_app() {
        let quirks = &preset("openrouter").unwrap().quirks;
        let body = quirks.extra_body.as_ref().unwrap();
        assert_eq!(body["provider"]["data_collection"].as_str(), Some("deny"));
        assert_eq!(body["provider"]["require_parameters"].as_bool(), Some(true));
        assert!(
            quirks
                .extra_headers
                .contains(&("X-OpenRouter-Title".to_string(), "{product}".to_string()))
        );
    }

    #[test]
    fn coding_plan_endpoints_and_keys_are_refused_with_the_vendors_words() {
        let url = |raw: &str| check_base_url(raw).unwrap();
        for endpoint in [
            "https://api.z.ai/api/coding/paas/v4",
            "https://coding-intl.dashscope.aliyuncs.com/v1",
            "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic",
            "https://api.kimi.ai/coding/v1",
            "https://api.kimi.com/coding/v1",
            "https://API.Z.AI/api/coding/paas/v4",
        ] {
            match check_endpoint(&url(endpoint), None) {
                EndpointCheck::Blocked {
                    sentence, source, ..
                } => {
                    assert!(!sentence.is_empty() && source.starts_with("https://"));
                }
                other => panic!("{endpoint}: {other:?}"),
            }
        }
        // The same vendors' pay-as-you-go endpoints are fine.
        for endpoint in [
            "https://api.z.ai/api/paas/v4",
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
            "https://api.moonshot.ai/v1",
            "https://api.openai.com/v1",
        ] {
            assert_eq!(
                check_endpoint(&url(endpoint), None),
                EndpointCheck::Allowed,
                "{endpoint}"
            );
        }
        // A plan key on an ordinary endpoint.
        let key = ApiKey::new("sk-sp-demo-not-a-real-key");
        assert!(matches!(
            check_endpoint(
                &url("https://dashscope-intl.aliyuncs.com/compatible-mode/v1"),
                Some(&key)
            ),
            EndpointCheck::Blocked { .. }
        ));
        let key = ApiKey::new("sk-cp-demo-not-a-real-key");
        assert!(matches!(
            check_endpoint(&url("https://api.minimax.io/v1"), Some(&key)),
            EndpointCheck::Warn { .. }
        ));
    }

    #[test]
    fn a_key_shows_only_its_last_four_characters() {
        let key = ApiKey::new("  sk-demo-not-a-real-key-ABCD \n");
        assert_eq!(key.last4(), "ABCD");
        assert_eq!(format!("{key:?}"), "ApiKey(…ABCD)");
        assert_eq!(ApiKey::new("ab").last4(), "ab");
    }

    #[test]
    fn model_patterns_match_with_stars() {
        assert!(glob_match("claude-opus-5-5*", "claude-opus-5-5-20260901"));
        assert!(glob_match("*-cloud", "gpt-oss:120b-cloud"));
        assert!(glob_match("gemini-3*flash*", "gemini-3.8-flash-preview"));
        assert!(glob_match("GPT-6*", "gpt-6-luna"));
        assert!(!glob_match("claude-opus-5-5*", "claude-opus-5"));
        assert!(!glob_match("exact", "exactly"));
        assert!(glob_match("exact", "EXACT"));
    }
}
