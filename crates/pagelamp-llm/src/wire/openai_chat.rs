//! OpenAI-compatible Chat Completions (`POST /chat/completions`, streamed): OpenRouter, Gemini's
//! compatible endpoint, LM Studio, llama.cpp and custom endpoints. Usage needs
//! `stream_options.include_usage`; effort is sent only where the preset says how.

use pagelamp_core::ai::{Effort, ModelErrorKind};
use serde_json::{Value, json};

use super::{
    BodyOptions, Dialect, REPAIR_INSTRUCTION, Step, count, event_json, kind_for_status,
    status_message,
};
use crate::error::ModelError;
use crate::profile::{EffortParam, ProviderProfile};
use crate::request::{GenerateRequest, JsonTier, OutputSpec, StopReason, Usage};
use crate::sse::{Framing, RawEvent};

pub(crate) struct OpenAiChat;

#[derive(Default)]
pub(crate) struct State {
    finish: Option<StopReason>,
    refusal: String,
    model: Option<String>,
}

impl State {
    fn done(&mut self) -> Step {
        let stop = if !self.refusal.is_empty() {
            StopReason::Refusal(std::mem::take(&mut self.refusal))
        } else {
            self.finish.take().unwrap_or(StopReason::Complete)
        };
        Step::Done {
            stop,
            model: self.model.take(),
        }
    }
}

impl Dialect for OpenAiChat {
    const FRAMING: Framing = Framing::Sse;
    type State = State;

    fn generate_path(_model: &str) -> String {
        "/chat/completions".to_string()
    }

    fn request_body(
        req: &GenerateRequest,
        profile: &ProviderProfile,
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
            "max_tokens": req.max_output_tokens,
        });
        if !profile.quirks.no_stream_usage {
            body["stream_options"] = json!({ "include_usage": true });
        }
        let quirks = profile.quirks.for_model(&req.model);
        if !quirks.no_effort {
            let effort = match req.effort {
                Effort::Lowest => quirks.lowest_effort.as_deref().unwrap_or("low"),
                Effort::Low => "low",
                Effort::Medium => "medium",
                Effort::High => "high",
            };
            match profile.quirks.effort_param {
                EffortParam::None => {}
                EffortParam::ReasoningEffort => body["reasoning_effort"] = json!(effort),
                EffortParam::ReasoningObject => body["reasoning"] = json!({ "effort": effort }),
            }
        }
        if let OutputSpec::Json { name, schema } = &req.output {
            match options.tier {
                JsonTier::NativeSchema => {
                    body["response_format"] = json!({
                        "type": "json_schema",
                        "json_schema": { "name": name, "schema": schema, "strict": true },
                    });
                }
                JsonTier::JsonObject => {
                    body["response_format"] = json!({ "type": "json_object" });
                }
                JsonTier::PromptOnly => {}
            }
        }
        body
    }

    fn on_event(state: &mut State, event: &RawEvent) -> Result<Vec<Step>, ModelError> {
        if event.data.trim() == "[DONE]" {
            return Ok(vec![state.done()]);
        }
        let data = event_json(event)?;
        // OpenRouter and others report a failure mid-stream as an `error` object.
        if data["error"].is_object() {
            let error = &data["error"];
            let code = error["code"]
                .as_str()
                .map(str::to_string)
                .or_else(|| error["code"].as_u64().map(|c| c.to_string()));
            let status = error["code"].as_u64().and_then(|c| u16::try_from(c).ok());
            let fallback = status.map_or(ModelErrorKind::Overloaded, kind_for_status);
            return Err(from_code(
                code.as_deref(),
                error["message"].as_str(),
                fallback,
            ));
        }
        if state.model.is_none() {
            state.model = data["model"].as_str().map(str::to_string);
        }
        let mut steps = Vec::new();
        if let Some(choice) = data["choices"].get(0) {
            let delta = &choice["delta"];
            if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                steps.push(Step::Text(text.to_string()));
            }
            if let Some(refusal) = delta["refusal"].as_str() {
                state.refusal.push_str(refusal);
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                state.finish = Some(match reason {
                    "length" => StopReason::MaxTokens,
                    "content_filter" => StopReason::ContentFilter,
                    _ => StopReason::Complete,
                });
            }
        }
        if data["usage"].is_object() {
            steps.push(Step::Usage(usage(&data["usage"])));
        }
        Ok(steps)
    }

    /// Some servers end without `[DONE]`: complete if a finish reason came.
    fn on_end(state: &mut State) -> Option<Step> {
        state.finish.is_some().then(|| state.done())
    }

    fn map_error(status: u16, body: &str) -> ModelError {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        // `{"error": {...}}`, or (some servers) `[{"error": {...}}]`, or `{"error": "text"}`.
        let error = if parsed.is_array() {
            &parsed[0]["error"]
        } else {
            &parsed["error"]
        };
        let (code, message) = if error.is_string() {
            (None, error.as_str())
        } else {
            (
                error["code"]
                    .as_str()
                    .or(error["type"].as_str())
                    .or(error["status"].as_str()),
                error["message"].as_str(),
            )
        };
        let mut mapped = from_code(code, message, kind_for_status(status));
        if mapped.message.is_empty() {
            mapped.message = status_message(status);
        }
        mapped.with_status(status)
    }
}

/// An error code of an OpenAI-compatible server as a model error.
fn from_code(code: Option<&str>, message: Option<&str>, fallback: ModelErrorKind) -> ModelError {
    let kind = match code.unwrap_or_default() {
        "insufficient_quota"
        | "credit_balance_exhausted"
        | "billing_hard_limit_reached"
        | "402" => ModelErrorKind::BillingOrQuota,
        spend if spend.ends_with("_spend_limit_exceeded") => ModelErrorKind::BillingOrQuota,
        "context_length_exceeded" => ModelErrorKind::ContextTooLong,
        "model_not_found" => ModelErrorKind::ModelNotFound,
        "rate_limit_exceeded" | "RESOURCE_EXHAUSTED" | "429" => ModelErrorKind::RateLimited,
        "invalid_api_key" | "UNAUTHENTICATED" | "PERMISSION_DENIED" | "401" | "403" => {
            ModelErrorKind::AuthRejected
        }
        "content_filter" | "content_policy_violation" => ModelErrorKind::ContentFiltered,
        _ => fallback,
    };
    ModelError::new(kind, message.unwrap_or_default()).with_code(code.map(str::to_string))
}

/// `usage` of a Chat Completions stream (`prompt_tokens` includes cached ones).
fn usage(usage: &Value) -> Usage {
    let input = count(&usage["prompt_tokens"]);
    let cached = count(&usage["prompt_tokens_details"]["cached_tokens"]);
    Usage {
        input_uncached: input.saturating_sub(cached),
        cache_read: cached,
        cache_write: 0,
        output: count(&usage["completion_tokens"]),
        reasoning: usage["completion_tokens_details"]["reasoning_tokens"].as_u64(),
        estimated: false,
    }
}
