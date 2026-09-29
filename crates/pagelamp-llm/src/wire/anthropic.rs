//! Anthropic Messages (`POST /v1/messages`, streamed), the native API: structured output through
//! `output_config.format`, effort through `output_config.effort`. Never `temperature`, `top_p`,
//! `top_k`, prefill or a forced tool (rejected or unsupported on current models).

use pagelamp_core::ai::{Effort, ModelErrorKind};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{Value, json};

use super::{Dialect, Step, count, event_json, kind_for_status, status_message};
use crate::error::ModelError;
use crate::profile::ProviderProfile;
use crate::request::{GenerateRequest, OutputSpec, StopReason, Usage};
use crate::sse::{Framing, RawEvent};

/// The API version header every request carries.
const API_VERSION: &str = "2023-06-01";

pub(crate) struct AnthropicMessages;

#[derive(Default)]
pub(crate) struct State {
    usage: Usage,
    model: Option<String>,
    stop: Option<StopReason>,
}

impl Dialect for AnthropicMessages {
    const FRAMING: Framing = Framing::Sse;
    type State = State;

    fn generate_path(_model: &str) -> String {
        "/v1/messages".to_string()
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(API_VERSION),
        );
        headers
    }

    fn request_body(req: &GenerateRequest, profile: &ProviderProfile) -> Value {
        let mut body = json!({
            "model": req.model,
            "max_tokens": req.max_output_tokens,
            "system": req.prompt.instructions(),
            "messages": [{ "role": "user", "content": req.prompt.user_text() }],
            "stream": true,
        });
        let mut output_config = serde_json::Map::new();
        if !profile.quirks.for_model(&req.model).no_effort {
            let effort = match req.effort {
                Effort::Lowest | Effort::Low => "low",
                Effort::Medium => "medium",
                Effort::High => "high",
            };
            output_config.insert("effort".into(), json!(effort));
        }
        if let OutputSpec::Json { schema, .. } = &req.output {
            output_config.insert(
                "format".into(),
                json!({ "type": "json_schema", "schema": schema }),
            );
        }
        if !output_config.is_empty() {
            body["output_config"] = Value::Object(output_config);
        }
        body
    }

    fn on_event(state: &mut State, event: &RawEvent) -> Result<Vec<Step>, ModelError> {
        let data = event_json(event)?;
        Ok(match data["type"].as_str().unwrap_or_default() {
            "message_start" => {
                let message = &data["message"];
                state.model = message["model"].as_str().map(str::to_string);
                merge_usage(&mut state.usage, &message["usage"]);
                vec![Step::Usage(state.usage)]
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                match (delta["type"].as_str(), delta["text"].as_str()) {
                    (Some("text_delta"), Some(text)) if !text.is_empty() => {
                        vec![Step::Text(text.to_string())]
                    }
                    _ => Vec::new(), // thinking and signature deltas are not answer text
                }
            }
            "message_delta" => {
                if let Some(reason) = data["delta"]["stop_reason"].as_str() {
                    state.stop = Some(stop_reason(reason));
                }
                // Counts here are cumulative.
                merge_usage(&mut state.usage, &data["usage"]);
                vec![Step::Usage(state.usage)]
            }
            "message_stop" => vec![Step::Done {
                stop: state.stop.take().unwrap_or(StopReason::Complete),
                model: state.model.take(),
            }],
            "error" => {
                let error = &data["error"];
                let kind = error["type"].as_str();
                return Err(from_type(
                    kind,
                    error["message"].as_str(),
                    ModelErrorKind::Overloaded,
                ));
            }
            _ => Vec::new(), // ping, content_block_start/stop, and future event types
        })
    }

    fn map_error(status: u16, body: &str) -> ModelError {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let error = &parsed["error"];
        let mut mapped = from_type(
            error["type"].as_str(),
            error["message"].as_str(),
            kind_for_status(status),
        );
        if mapped.message.is_empty() {
            mapped.message = status_message(status);
        }
        mapped.with_status(status)
    }
}

/// Anthropic's `stop_reason` as ours.
fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "max_tokens" | "model_context_window_exceeded" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal(String::new()),
        _ => StopReason::Complete, // end_turn, stop_sequence, pause_turn, …
    }
}

/// An Anthropic error type as a model error; `fallback` when the type doesn't decide it.
fn from_type(kind: Option<&str>, message: Option<&str>, fallback: ModelErrorKind) -> ModelError {
    let message = message.unwrap_or_default();
    let mapped = match kind.unwrap_or_default() {
        "authentication_error" | "permission_error" => ModelErrorKind::AuthRejected,
        "billing_error" => ModelErrorKind::BillingOrQuota,
        "not_found_error" => ModelErrorKind::ModelNotFound,
        "request_too_large" => ModelErrorKind::ContextTooLong,
        "overloaded_error" | "api_error" => ModelErrorKind::Overloaded,
        "timeout_error" => ModelErrorKind::Timeout,
        // A spend limit is billing, not a wait: never retried.
        "rate_limit_error"
            if message.contains("spend limit") || message.contains("spend_limit") =>
        {
            ModelErrorKind::BillingOrQuota
        }
        "rate_limit_error" => ModelErrorKind::RateLimited,
        "invalid_request_error" if message.contains("prompt is too long") => {
            ModelErrorKind::ContextTooLong
        }
        "invalid_request_error" if message.contains("credit balance") => {
            ModelErrorKind::BillingOrQuota
        }
        "invalid_request_error" => ModelErrorKind::InvalidRequest,
        _ => fallback,
    };
    ModelError::new(mapped, message).with_code(kind.map(str::to_string))
}

/// Add the counts present in `usage` (absent ones stay as they were).
fn merge_usage(into: &mut Usage, usage: &Value) {
    if let Some(input) = usage["input_tokens"].as_u64() {
        into.input_uncached = input;
    }
    if let Some(read) = usage["cache_read_input_tokens"].as_u64() {
        into.cache_read = read;
    }
    if let Some(write) = usage["cache_creation_input_tokens"].as_u64() {
        into.cache_write = write;
    }
    if usage["output_tokens"].is_u64() {
        into.output = count(&usage["output_tokens"]);
    }
}
