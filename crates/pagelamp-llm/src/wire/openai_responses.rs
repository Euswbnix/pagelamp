//! OpenAI Responses (`POST /responses`, streamed). Always `store: false` (else OpenAI keeps the
//! response for 30 days). Quota and spend 429s are billing errors, never retried.

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

pub(crate) struct OpenAiResponses;

#[derive(Default)]
pub(crate) struct State {
    refusal: String,
}

impl Dialect for OpenAiResponses {
    const FRAMING: Framing = Framing::Sse;
    type State = State;

    fn generate_path(_model: &str) -> String {
        "/responses".to_string()
    }

    fn request_body(
        req: &GenerateRequest,
        profile: &ProviderProfile,
        options: BodyOptions,
    ) -> Value {
        let mut input = vec![json!({
            "role": "user",
            "content": [{ "type": "input_text", "text": req.prompt.user_text() }],
        })];
        if let Some(previous) = options.repair {
            input.push(json!({
                "role": "assistant",
                "content": [{ "type": "output_text", "text": previous }],
            }));
            input.push(json!({
                "role": "user",
                "content": [{ "type": "input_text", "text": REPAIR_INSTRUCTION }],
            }));
        }
        let mut body = json!({
            "model": req.model,
            "instructions": req.prompt.instructions(),
            "input": input,
            "stream": true,
            "store": false,
            "max_output_tokens": req.max_output_tokens,
        });
        let quirks = profile.model_quirks(&req.model);
        if !quirks.no_effort {
            let effort = match req.effort {
                Effort::Lowest => quirks.lowest_effort.as_deref().unwrap_or("low"),
                Effort::Low => "low",
                Effort::Medium => "medium",
                Effort::High => "high",
            };
            body["reasoning"] = json!({ "effort": effort });
        }
        if let OutputSpec::Json { name, schema } = &req.output {
            match options.tier {
                JsonTier::NativeSchema => {
                    body["text"] = json!({
                        "format": { "type": "json_schema", "name": name, "schema": schema, "strict": true }
                    });
                }
                JsonTier::JsonObject => {
                    body["text"] = json!({ "format": { "type": "json_object" } });
                }
                JsonTier::PromptOnly => {}
            }
        }
        body
    }

    fn on_event(state: &mut State, event: &RawEvent) -> Result<Vec<Step>, ModelError> {
        let data = event_json(event)?;
        let kind = data["type"].as_str().unwrap_or_default();
        Ok(match kind {
            "response.output_text.delta" => match data["delta"].as_str() {
                Some(delta) if !delta.is_empty() => vec![Step::Text(delta.to_string())],
                _ => Vec::new(),
            },
            "response.refusal.delta" => {
                state
                    .refusal
                    .push_str(data["delta"].as_str().unwrap_or_default());
                Vec::new()
            }
            "response.completed" | "response.incomplete" => {
                let response = &data["response"];
                let stop = if !state.refusal.is_empty() {
                    StopReason::Refusal(std::mem::take(&mut state.refusal))
                } else if kind == "response.completed" {
                    StopReason::Complete
                } else {
                    match response["incomplete_details"]["reason"].as_str() {
                        Some("content_filter") => StopReason::ContentFilter,
                        _ => StopReason::MaxTokens,
                    }
                };
                let mut steps = Vec::new();
                if response["usage"].is_object() {
                    steps.push(Step::Usage(usage(&response["usage"])));
                }
                steps.push(Step::Done {
                    stop,
                    model: response["model"].as_str().map(str::to_string),
                });
                steps
            }
            "response.failed" => {
                let error = &data["response"]["error"];
                return Err(from_code(
                    error["code"].as_str(),
                    error["message"].as_str(),
                    ModelErrorKind::Overloaded,
                ));
            }
            "error" => {
                return Err(from_code(
                    data["code"].as_str(),
                    data["message"].as_str(),
                    ModelErrorKind::Overloaded,
                ));
            }
            _ => Vec::new(),
        })
    }

    fn models_path() -> &'static str {
        "/models"
    }

    fn parse_models(body: &Value) -> Vec<super::ListedModel> {
        super::openai_style_models(body)
    }

    fn map_error(status: u16, body: &str) -> ModelError {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let error = &parsed["error"];
        let code = error["code"].as_str().or(error["type"].as_str());
        let mut mapped = from_code(code, error["message"].as_str(), kind_for_status(status));
        if mapped.message.is_empty() {
            mapped.message = status_message(status);
        }
        // A 403 for the region the student is in is not a key problem.
        if status == 403 && code == Some("unsupported_country_region_territory") {
            mapped.kind = ModelErrorKind::Unsupported;
        }
        mapped.with_status(status)
    }
}

/// An OpenAI error code (from a response or a stream event) as a model error; `fallback` when
/// the code doesn't decide it.
fn from_code(code: Option<&str>, message: Option<&str>, fallback: ModelErrorKind) -> ModelError {
    let kind = match code.unwrap_or_default() {
        "insufficient_quota" | "credit_balance_exhausted" | "billing_hard_limit_reached" => {
            ModelErrorKind::BillingOrQuota
        }
        spend if spend.ends_with("_spend_limit_exceeded") => ModelErrorKind::BillingOrQuota,
        "context_length_exceeded" => ModelErrorKind::ContextTooLong,
        "model_not_found" => ModelErrorKind::ModelNotFound,
        "rate_limit_exceeded" | "slow_down" => ModelErrorKind::RateLimited,
        "server_error" | "server_is_overloaded" => ModelErrorKind::Overloaded,
        "content_policy_violation" | "content_filter" => ModelErrorKind::ContentFiltered,
        "invalid_api_key" => ModelErrorKind::AuthRejected,
        "invalid_prompt" | "invalid_request_error" => ModelErrorKind::InvalidRequest,
        _ => fallback,
    };
    ModelError::new(kind, message.unwrap_or_default()).with_code(code.map(str::to_string))
}

/// `usage` of a completed response (`input_tokens` includes the cached ones).
fn usage(usage: &Value) -> Usage {
    let input = count(&usage["input_tokens"]);
    let cached = count(&usage["input_tokens_details"]["cached_tokens"]);
    Usage {
        input_uncached: input.saturating_sub(cached),
        cache_read: cached,
        cache_write: 0,
        output: count(&usage["output_tokens"]),
        reasoning: usage["output_tokens_details"]["reasoning_tokens"].as_u64(),
        estimated: false,
    }
}
