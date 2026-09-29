//! What a model call takes and gives back.

use pagelamp_core::ai::Effort;
use pagelamp_core::ai_gate::RenderedPrompt;

/// One model call. The prompt is the ONLY text sent (the policy gate's `RenderedPrompt`).
#[derive(Debug)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: RenderedPrompt,
    pub output: OutputSpec,
    pub effort: Effort,
    /// The most the model may write (thinking included where the provider counts it so).
    pub max_output_tokens: u32,
}

/// What kind of answer is wanted.
#[derive(Clone, Debug, PartialEq)]
pub enum OutputSpec {
    Text,
    /// JSON matching `schema` (a lowest-common-denominator schema, built from a Rust type).
    Json {
        name: &'static str,
        schema: serde_json::Value,
    },
}

/// How a JSON answer is asked for, best first. A server that rejects one tier (HTTP 400)
/// is asked again with the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum JsonTier {
    /// The provider enforces the schema (strict JSON schema).
    NativeSchema,
    /// The provider guarantees JSON, not the schema.
    JsonObject,
    /// Only the instructions ask for the format.
    PromptOnly,
}

impl JsonTier {
    pub fn as_str(self) -> &'static str {
        match self {
            JsonTier::NativeSchema => "native_schema",
            JsonTier::JsonObject => "json_object",
            JsonTier::PromptOnly => "prompt_only",
        }
    }

    /// The next tier down, if any.
    pub(crate) fn next(self) -> Option<JsonTier> {
        match self {
            JsonTier::NativeSchema => Some(JsonTier::JsonObject),
            JsonTier::JsonObject => Some(JsonTier::PromptOnly),
            JsonTier::PromptOnly => None,
        }
    }
}

/// Something worth telling the student while the model answers (codes the UIs translate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The server didn't take the JSON format; asked again with a weaker one.
    JsonFallback,
    /// The answer didn't match the format; the model is asked once to correct it.
    Repairing,
}

/// Something that happened while the model answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// More answer text.
    TextDelta(String),
    /// Token counts so far (the last one is final).
    Usage(Usage),
    Notice(Notice),
}

/// Token counts of one call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Input tokens not read from a cache.
    pub input_uncached: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Output tokens, reasoning included.
    pub output: u64,
    /// Of `output`, the tokens spent thinking, if the provider says.
    pub reasoning: Option<u64>,
    /// Estimated by us (the call was cancelled before the provider reported usage).
    pub estimated: bool,
}

/// Why the model stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    Complete,
    /// Hit `max_output_tokens`.
    MaxTokens,
    /// The model declined; the text is its explanation, if any.
    Refusal(String),
    ContentFilter,
}

impl Usage {
    /// Both calls' counts (a repair).
    pub(crate) fn plus(self, other: Usage) -> Usage {
        Usage {
            input_uncached: self.input_uncached + other.input_uncached,
            cache_read: self.cache_read + other.cache_read,
            cache_write: self.cache_write + other.cache_write,
            output: self.output + other.output,
            reasoning: match (self.reasoning, other.reasoning) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
            },
            estimated: self.estimated || other.estimated,
        }
    }
}

/// A finished call.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub text: String,
    /// The parsed answer for `OutputSpec::Json`.
    pub json: Option<serde_json::Value>,
    pub stop: StopReason,
    pub usage: Usage,
    pub request_id: Option<String>,
    /// The model id the provider reports (may differ from the requested alias).
    pub model_reported: Option<String>,
    /// How the JSON answer was asked for (`None` for text).
    pub json_tier: Option<JsonTier>,
    /// A repair call was made (its usage is included).
    pub repaired: bool,
}
