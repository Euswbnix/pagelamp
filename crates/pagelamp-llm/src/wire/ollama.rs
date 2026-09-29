//! Ollama's own API (`POST /api/chat`, NDJSON). Needed over its OpenAI-compatible endpoint:
//! only here can PageLamp set the context size (`options.num_ctx`; the default is small), turn
//! thinking off (`think`) and pass a JSON schema (`format`).

use pagelamp_core::ai::{Effort, ModelErrorKind};
use serde_json::{Value, json};

use super::{
    BodyOptions, Dialect, REPAIR_INSTRUCTION, Step, count, event_json, kind_for_status,
    status_message,
};
use crate::error::ModelError;
use crate::profile::ProviderProfile;
use crate::request::{GenerateRequest, JsonTier, OutputSpec, StopReason, Usage};
use crate::sse::{Framing, RawEvent};

/// Smallest context PageLamp asks Ollama for.
const MIN_NUM_CTX: u64 = 4096;
/// Largest context PageLamp asks Ollama for (memory grows with it).
const MAX_NUM_CTX: u64 = 131_072;

pub(crate) struct OllamaNative;

#[derive(Default)]
pub(crate) struct State;

impl Dialect for OllamaNative {
    const FRAMING: Framing = Framing::Ndjson;
    type State = State;

    fn generate_path(_model: &str) -> String {
        "/api/chat".to_string()
    }

    fn request_body(
        req: &GenerateRequest,
        _profile: &ProviderProfile,
        options: BodyOptions,
    ) -> Value {
        let mut messages = vec![
            json!({ "role": "system", "content": req.prompt.instructions() }),
            json!({ "role": "user", "content": req.prompt.user_text() }),
        ];
        if let Some(previous) = options.repair {
            messages.push(json!({ "role": "assistant", "content": previous }));
            messages.push(json!({ "role": "user", "content": REPAIR_INSTRUCTION }));
        }
        let mut body = json!({
            "model": req.model,
            "messages": messages,
            "stream": true,
            "options": {
                "num_ctx": num_ctx(req),
                "num_predict": req.max_output_tokens,
            },
        });
        // Thinking off for the lowest effort (summaries don't need it); otherwise the model's
        // default, since `think: true` is an error on models that can't think.
        if req.effort == Effort::Lowest {
            body["think"] = json!(false);
        }
        if let OutputSpec::Json { schema, .. } = &req.output {
            match options.tier {
                JsonTier::NativeSchema => body["format"] = schema.clone(),
                JsonTier::JsonObject => body["format"] = json!("json"),
                JsonTier::PromptOnly => {}
            }
        }
        body
    }

    fn on_event(_state: &mut State, event: &RawEvent) -> Result<Vec<Step>, ModelError> {
        let data = event_json(event)?;
        if let Some(message) = data["error"].as_str() {
            return Err(from_message(message, ModelErrorKind::Overloaded));
        }
        let mut steps = Vec::new();
        if let Some(text) = data["message"]["content"]
            .as_str()
            .filter(|t| !t.is_empty())
        {
            steps.push(Step::Text(text.to_string()));
        }
        if data["done"].as_bool() == Some(true) {
            steps.push(Step::Usage(Usage {
                input_uncached: count(&data["prompt_eval_count"]),
                cache_read: 0,
                cache_write: 0,
                output: count(&data["eval_count"]),
                reasoning: None,
                estimated: false,
            }));
            let stop = match data["done_reason"].as_str() {
                Some("length") => StopReason::MaxTokens,
                _ => StopReason::Complete,
            };
            steps.push(Step::Done {
                stop,
                model: data["model"].as_str().map(str::to_string),
            });
        }
        Ok(steps)
    }

    fn map_error(status: u16, body: &str) -> ModelError {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let mut mapped = from_message(
            parsed["error"].as_str().unwrap_or_default(),
            kind_for_status(status),
        );
        if mapped.message.is_empty() {
            mapped.message = status_message(status);
        }
        mapped.with_status(status)
    }
}

/// An Ollama error text as a model error (Ollama has no codes).
fn from_message(message: &str, fallback: ModelErrorKind) -> ModelError {
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("not found") && lower.contains("model") {
        ModelErrorKind::ModelNotFound
    } else if lower.contains("context") && (lower.contains("exceed") || lower.contains("too long"))
    {
        ModelErrorKind::ContextTooLong
    } else if lower.contains("unauthorized") {
        // A cloud model through the local daemon without `ollama signin`.
        ModelErrorKind::NotSignedIn
    } else {
        fallback
    };
    ModelError::new(kind, message)
}

/// A context size that fits the prompt and the answer: a conservative token count of the
/// prompt (1 per 3 bytes, CJK text included) plus the output budget, rounded up to 1024.
fn num_ctx(req: &GenerateRequest) -> u64 {
    let prompt_bytes = req.prompt.instructions().len() + req.prompt.user_text().len();
    let needed = (prompt_bytes as u64).div_ceil(3) + u64::from(req.max_output_tokens) + 512;
    needed
        .div_ceil(1024)
        .saturating_mul(1024)
        .clamp(MIN_NUM_CTX, MAX_NUM_CTX)
}
