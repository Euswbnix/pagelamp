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

/// Something that happened while the model answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// More answer text.
    TextDelta(String),
    /// Token counts so far (the last one is final).
    Usage(Usage),
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
}
