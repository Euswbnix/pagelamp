//! The closed set of backends (enum dispatch; no plugin surface) and the HTTP driver that runs a
//! request: retries before the first byte, time limits, cancellation, streaming.

use std::time::Duration;

use pagelamp_core::ai::{BlockReason, ModelErrorKind};
use pagelamp_core::brand;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::client::{
    auth_headers, build_client, join_path, request_id, retry_after, transport_error,
};
use crate::error::{LlmError, ModelError};
use crate::profile::{ApiKey, EndpointCheck, ProviderProfile, Wire, check_endpoint};
use crate::request::{
    GenerateRequest, JsonTier, ModelInfo, Notice, Outcome, OutputSpec, ProbeReport, StopReason,
    StreamEvent, Usage,
};
use crate::retry::delay_before_retry;
use crate::sse::EventParser;
use crate::wire::anthropic::AnthropicMessages;
use crate::wire::ollama::OllamaNative;
use crate::wire::openai_chat::OpenAiChat;
use crate::wire::openai_responses::OpenAiResponses;
use crate::wire::{BodyOptions, Dialect, Step};

/// The most answer text accepted from one call.
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// The most error-response body read (it only carries a code and a message).
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;
/// The most model-list body read (OpenRouter lists hundreds of models).
const MAX_LIST_BYTES: usize = 16 * 1024 * 1024;
/// How long a model list may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// The fixed wording of a "Test": no course data, a tiny JSON answer.
const PROBE_INSTRUCTIONS: &str = "This is a connection test from PageLamp. Answer with the JSON \
     object {\"ok\": true} and nothing else.";
/// The probe's own (static) user text, sent through the gate like a student's note.
const PROBE_NOTE: &str = "Connection test.";
/// Output budget of a probe: room for a little thinking on models that always think.
const PROBE_MAX_OUTPUT_TOKENS: u32 = 1024;

/// The probe's answer.
#[allow(dead_code)]
#[derive(schemars::JsonSchema)]
struct ProbeAnswer {
    ok: bool,
}

/// How long a call may be silent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// Until the first byte of the answer: reasoning models can think for minutes first.
    pub first_byte: Duration,
    /// Between two pieces of the stream after that.
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            first_byte: Duration::from_secs(10 * 60),
            idle: Duration::from_secs(3 * 60),
        }
    }
}

/// Where a model runs. v0.3.0 M1 has the HTTP drivers; the Codex runtime joins in M2.
#[derive(Debug)]
pub enum Backend {
    Http(HttpDriver),
}

impl Backend {
    /// Run one request, reporting text and usage through `on_event` as they arrive.
    pub async fn generate(
        &self,
        request: GenerateRequest,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: CancellationToken,
    ) -> Result<Outcome, LlmError> {
        match self {
            Backend::Http(driver) => driver.generate(request, on_event, cancel).await,
        }
    }

    /// The models on offer (a free call that also checks the key).
    pub async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        match self {
            Backend::Http(driver) => driver.list_models(cancel).await,
        }
    }

    /// Try `model` with a tiny JSON request ("Test").
    pub async fn probe(
        &self,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<ProbeReport, LlmError> {
        match self {
            Backend::Http(driver) => driver.probe(model, cancel).await,
        }
    }
}

/// A provider reached over HTTP with one of the wire dialects.
pub struct HttpDriver {
    profile: ProviderProfile,
    base_url: Url,
    key: Option<ApiKey>,
    client: reqwest::Client,
    timeouts: Timeouts,
}

impl std::fmt::Debug for HttpDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpDriver")
            .field("provider", &self.profile.id)
            .field("wire", &self.profile.wire)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl HttpDriver {
    /// A driver for `profile` (its base URL must be set) with `key`. Refuses coding-plan
    /// endpoints and keys (`BlockReason::CodingPlanKey`, quoting the vendor).
    pub fn new(profile: ProviderProfile, key: Option<ApiKey>) -> Result<HttpDriver, LlmError> {
        let base_url = profile.base_url.clone().ok_or_else(|| {
            LlmError::Model(ModelError::new(
                ModelErrorKind::InvalidRequest,
                "this provider has no address",
            ))
        })?;
        if let EndpointCheck::Blocked {
            vendor, sentence, ..
        } = check_endpoint(&base_url, key.as_ref())
        {
            return Err(LlmError::Blocked {
                reason: BlockReason::CodingPlanKey,
                message: format!("{vendor}: “{sentence}”"),
            });
        }
        Ok(HttpDriver {
            profile,
            base_url,
            key,
            client: build_client().map_err(LlmError::Model)?,
            timeouts: Timeouts::default(),
        })
    }

    /// The same driver with other time limits (tests).
    pub fn with_timeouts(mut self, timeouts: Timeouts) -> HttpDriver {
        self.timeouts = timeouts;
        self
    }

    pub fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    pub async fn generate(
        &self,
        request: GenerateRequest,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: CancellationToken,
    ) -> Result<Outcome, LlmError> {
        match self.profile.wire {
            Wire::OpenAiResponses => {
                self.generate_with::<OpenAiResponses>(&request, on_event, &cancel)
                    .await
            }
            Wire::AnthropicMessages => {
                self.generate_with::<AnthropicMessages>(&request, on_event, &cancel)
                    .await
            }
            Wire::OpenAiChat => {
                self.generate_with::<OpenAiChat>(&request, on_event, &cancel)
                    .await
            }
            Wire::OllamaNative => {
                self.generate_with::<OllamaNative>(&request, on_event, &cancel)
                    .await
            }
        }
    }

    /// The models this provider offers, minus the hidden ones (embeddings, speech, …).
    pub async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        match self.profile.wire {
            Wire::OpenAiResponses => self.list_with::<OpenAiResponses>(cancel).await,
            Wire::AnthropicMessages => self.list_with::<AnthropicMessages>(cancel).await,
            Wire::OpenAiChat => self.list_with::<OpenAiChat>(cancel).await,
            Wire::OllamaNative => self.list_with::<OllamaNative>(cancel).await,
        }
    }

    async fn list_with<D: Dialect>(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<ModelInfo>, LlmError> {
        let path = D::models_path();
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let mut url = join_path(&self.base_url, path);
        if !query.is_empty() {
            url.set_query(Some(query));
        }
        let request = self.client.get(url).headers(self.headers::<D>()?).send();
        let mut response = tokio::select! {
            _ = cancel.cancelled() => return Err(LlmError::Cancelled),
            result = tokio::time::timeout(LIST_TIMEOUT, request) => match result {
                Err(_) => return Err(timed_out().into()),
                Ok(Err(error)) => return Err(transport_error(&error).into()),
                Ok(Ok(response)) => response,
            },
        };
        let status = response.status().as_u16();
        let body = match read_body(&mut response, LIST_TIMEOUT, MAX_LIST_BYTES, cancel).await {
            Ok(body) => body,
            Err(Attempt::Cancelled) => return Err(LlmError::Cancelled),
            Err(Attempt::Failed(error)) => return Err(error.into()),
        };
        if !response.status().is_success() {
            return Err(D::map_error(status, &body).into());
        }
        let parsed: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
            ModelError::new(
                ModelErrorKind::BadOutput,
                "the model list is not valid JSON",
            )
        })?;
        let local = self.profile.on_device();
        let mut models: Vec<ModelInfo> = D::parse_models(&parsed)
            .into_iter()
            .filter(|model| !self.profile.quirks.hides(&model.id))
            .map(|model| ModelInfo {
                on_device: local && !model.remote,
                id: model.id,
                display_name: model.display_name,
                context_window: model.context_window,
            })
            .collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));
        models.dedup_by(|a, b| a.id == b.id);
        Ok(models)
    }

    /// "Test": a tiny JSON request with no course data, through the gate like any other.
    pub async fn probe(
        &self,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<ProbeReport, LlmError> {
        let note = pagelamp_core::ai_gate::StudentNote::new(PROBE_NOTE);
        let prompt = pagelamp_core::ai_gate::assemble(
            PROBE_INSTRUCTIONS,
            &pagelamp_core::ai_gate::GatedContext::empty(),
            note.as_ref(),
        );
        let output = OutputSpec::for_type::<ProbeAnswer>("connection_test").map_err(|_| {
            ModelError::new(ModelErrorKind::Unsupported, "the test format is not usable")
        })?;
        let request = GenerateRequest {
            model: model.to_string(),
            prompt,
            output,
            effort: pagelamp_core::ai::Effort::Lowest,
            max_output_tokens: PROBE_MAX_OUTPUT_TOKENS,
        };
        let started = std::time::Instant::now();
        let outcome = self.generate(request, &|_| {}, cancel).await?;
        let json_tier = outcome.json_tier.unwrap_or(JsonTier::PromptOnly);
        if outcome.json.is_none() {
            return Err(ModelError::new(
                ModelErrorKind::BadOutput,
                "the model answered, but not in the requested format",
            )
            .into());
        }
        Ok(ProbeReport {
            latency: started.elapsed(),
            json_tier,
            thinking_always_on: self.profile.model_quirks(model).thinking_always_on,
            usage: outcome.usage,
            model_reported: outcome.model_reported,
        })
    }

    /// The full request body for `request` on this provider, as first sent (also used by the
    /// per-wire golden tests: exactly these bytes are sent).
    pub fn request_body(&self, request: &GenerateRequest) -> serde_json::Value {
        let options = BodyOptions {
            tier: self.first_tier(),
            repair: None,
        };
        match self.profile.wire {
            Wire::OpenAiResponses => self.body::<OpenAiResponses>(request, options),
            Wire::AnthropicMessages => self.body::<AnthropicMessages>(request, options),
            Wire::OpenAiChat => self.body::<OpenAiChat>(request, options),
            Wire::OllamaNative => self.body::<OllamaNative>(request, options),
        }
    }

    /// The best JSON tier this provider is known to take.
    fn first_tier(&self) -> JsonTier {
        let quirks = &self.profile.quirks;
        if quirks.no_json_mode {
            JsonTier::PromptOnly
        } else if quirks.json_object_only {
            JsonTier::JsonObject
        } else {
            JsonTier::NativeSchema
        }
    }

    /// Run `request`; for JSON, fall back a tier when the server rejects the format, then
    /// validate the answer locally and, below the native tier, ask once for a repair.
    async fn generate_with<D: Dialect>(
        &self,
        request: &GenerateRequest,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: &CancellationToken,
    ) -> Result<Outcome, LlmError> {
        let OutputSpec::Json { schema, .. } = &request.output else {
            let options = BodyOptions {
                tier: JsonTier::NativeSchema,
                repair: None,
            };
            return self.run::<D>(request, options, on_event, cancel).await;
        };
        let validator = jsonschema::validator_for(schema).map_err(|_| {
            ModelError::new(
                ModelErrorKind::Unsupported,
                "the answer format is not usable",
            )
        })?;
        let mut tier = self.first_tier();
        let first = loop {
            let options = BodyOptions { tier, repair: None };
            match self.run::<D>(request, options, on_event, cancel).await {
                Err(LlmError::Model(error)) if rejects_format(&error) => match tier.next() {
                    Some(next) => {
                        tracing::info!(
                            target: "pagelamp::llm",
                            provider = %self.profile.id,
                            from = tier.as_str(),
                            to = next.as_str(),
                            "JSON format refused; trying the next"
                        );
                        on_event(StreamEvent::Notice(Notice::JsonFallback));
                        tier = next;
                    }
                    None => return Err(LlmError::Model(error)),
                },
                other => break other?,
            }
        };
        let mut outcome = Outcome {
            json_tier: Some(tier),
            ..first
        };
        if outcome.stop != StopReason::Complete {
            return Ok(outcome); // cut off or refused: nothing to validate
        }
        if let Some(value) = valid_json(&outcome.text, &validator) {
            outcome.json = Some(value);
            return Ok(outcome);
        }
        if tier == JsonTier::NativeSchema {
            return Err(bad_json());
        }
        on_event(StreamEvent::Notice(Notice::Repairing));
        let options = BodyOptions {
            tier,
            repair: Some(&outcome.text),
        };
        let second = self.run::<D>(request, options, on_event, cancel).await?;
        let usage = outcome.usage.plus(second.usage);
        let json = (second.stop == StopReason::Complete)
            .then(|| valid_json(&second.text, &validator))
            .flatten()
            .ok_or_else(bad_json)?;
        Ok(Outcome {
            json: Some(json),
            usage,
            json_tier: Some(tier),
            repaired: true,
            ..second
        })
    }

    fn body<D: Dialect>(
        &self,
        request: &GenerateRequest,
        options: BodyOptions<'_>,
    ) -> serde_json::Value {
        let mut body = D::request_body(request, &self.profile, options);
        if let Some(extra) = &self.profile.quirks.extra_body
            && let Ok(extra) = serde_json::to_value(extra)
        {
            merge(&mut body, extra);
        }
        body
    }

    fn headers<D: Dialect>(&self) -> Result<HeaderMap, ModelError> {
        let mut headers = auth_headers(self.profile.auth, self.key.as_ref())?;
        headers.extend(D::headers());
        for (name, value) in &self.profile.quirks.extra_headers {
            let value = value.replace("{product}", brand::PRODUCT_NAME);
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(&value),
            ) {
                headers.insert(name, value);
            }
        }
        Ok(headers)
    }

    /// One request with retries before the first byte.
    async fn run<D: Dialect>(
        &self,
        request: &GenerateRequest,
        options: BodyOptions<'_>,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: &CancellationToken,
    ) -> Result<Outcome, LlmError> {
        let url = join_path(&self.base_url, &D::generate_path(&request.model));
        let body = self.body::<D>(request, options);
        let headers = self.headers::<D>()?;
        let mut tries = 0;
        loop {
            tries += 1;
            let error = match self
                .attempt::<D>(&url, &headers, &body, on_event, cancel)
                .await
            {
                Ok(outcome) => return Ok(outcome),
                Err(Attempt::Cancelled) => return Err(LlmError::Cancelled),
                Err(Attempt::Failed(error)) => error,
            };
            tracing::info!(
                target: "pagelamp::llm",
                provider = %self.profile.id,
                kind = error.kind.as_str(),
                status = ?error.http_status,
                code = ?error.provider_code,
                tries,
                "model call failed"
            );
            let Some(wait) = delay_before_retry(&error, tries) else {
                return Err(LlmError::Model(error));
            };
            tokio::select! {
                _ = cancel.cancelled() => return Err(LlmError::Cancelled),
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }

    /// One try: send, then read the stream to its end.
    async fn attempt<D: Dialect>(
        &self,
        url: &Url,
        headers: &HeaderMap,
        body: &serde_json::Value,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: &CancellationToken,
    ) -> Result<Outcome, Attempt> {
        let send = self
            .client
            .post(url.clone())
            .headers(headers.clone())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send();
        let mut response = tokio::select! {
            _ = cancel.cancelled() => return Err(Attempt::Cancelled),
            result = tokio::time::timeout(self.timeouts.first_byte, send) => match result {
                Err(_) => return Err(timed_out().into()),
                Ok(Err(error)) => return Err(transport_error(&error).into()),
                Ok(Ok(response)) => response,
            },
        };
        let status = response.status().as_u16();
        let request_id = request_id(response.headers());
        if !response.status().is_success() {
            let wait = retry_after(response.headers());
            let body = read_body(
                &mut response,
                self.timeouts.idle,
                MAX_ERROR_BODY_BYTES,
                cancel,
            )
            .await?;
            return Err(D::map_error(status, &body)
                .with_retry_after(wait)
                .with_request_id(request_id)
                .into());
        }

        let mut parser = EventParser::new(D::FRAMING);
        let mut state = D::State::default();
        let mut text = String::new();
        let mut usage = Usage::default();
        let mut finished: Option<(StopReason, Option<String>)> = None;
        let mut first = true;
        while finished.is_none() {
            let limit = if first {
                self.timeouts.first_byte
            } else {
                self.timeouts.idle
            };
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Err(Attempt::Cancelled),
                result = tokio::time::timeout(limit, response.chunk()) => result,
            };
            let ended = matches!(chunk, Ok(Ok(None)));
            let events = match chunk {
                Err(_) => return Err(after(timed_out(), &text).into()),
                Ok(Err(error)) => return Err(after(transport_error(&error), &text).into()),
                Ok(Ok(Some(bytes))) => {
                    first = false;
                    parser.feed(&bytes).map_err(|_| after(too_long(), &text))?
                }
                Ok(Ok(None)) => parser.finish().map_err(|_| after(too_long(), &text))?,
            };
            // Event by event, so an error event knows whether text came before it.
            for event in &events {
                let steps = D::on_event(&mut state, event).map_err(|error| after(error, &text))?;
                for step in steps {
                    apply(step, &mut text, &mut usage, &mut finished, on_event)?;
                }
            }
            if ended
                && finished.is_none()
                && let Some(step) = D::on_end(&mut state)
            {
                apply(step, &mut text, &mut usage, &mut finished, on_event)?;
            }
            if ended && finished.is_none() {
                let error = ModelError::new(
                    ModelErrorKind::Network,
                    "the connection ended before the answer was complete",
                );
                return Err(after(error, &text).into());
            }
        }
        let (stop, model_reported) = finished.expect("loop ends only when finished");
        let stop = match stop {
            StopReason::Refusal(reason) if reason.is_empty() => StopReason::Refusal(text.clone()),
            other => other,
        };
        Ok(Outcome {
            text,
            json: None,
            stop,
            usage,
            request_id,
            model_reported,
            json_tier: None,
            repaired: false,
        })
    }
}

/// Take one step of the stream into the answer so far.
fn apply(
    step: Step,
    text: &mut String,
    usage: &mut Usage,
    finished: &mut Option<(StopReason, Option<String>)>,
    on_event: &(dyn Fn(StreamEvent) + Send + Sync),
) -> Result<(), ModelError> {
    match step {
        Step::Text(delta) => {
            if text.len() + delta.len() > MAX_OUTPUT_BYTES {
                return Err(after(too_long(), text));
            }
            text.push_str(&delta);
            on_event(StreamEvent::TextDelta(delta));
        }
        Step::Usage(new) => {
            *usage = new;
            on_event(StreamEvent::Usage(new));
        }
        Step::Done { stop, model } => *finished = Some((stop, model)),
    }
    Ok(())
}

/// How one try ended when it didn't produce an outcome.
enum Attempt {
    Cancelled,
    Failed(ModelError),
}

impl From<ModelError> for Attempt {
    fn from(error: ModelError) -> Self {
        Attempt::Failed(error)
    }
}

/// A 400/422 before any output while a JSON format was asked for: the server may not know the
/// format (the next tier is tried).
fn rejects_format(error: &ModelError) -> bool {
    error.kind == ModelErrorKind::InvalidRequest
        && !error.after_output
        && matches!(error.http_status, Some(400 | 422))
}

/// `text` parsed and valid against the schema (a code fence around it is tolerated).
fn valid_json(text: &str, validator: &jsonschema::Validator) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(trimmed);
    let value: serde_json::Value = serde_json::from_str(unfenced.trim()).ok()?;
    validator.is_valid(&value).then_some(value)
}

fn bad_json() -> LlmError {
    LlmError::Model(ModelError::new(
        ModelErrorKind::BadOutput,
        "the answer did not match the required format",
    ))
}

fn timed_out() -> ModelError {
    ModelError::new(
        ModelErrorKind::Timeout,
        "the service took too long to answer",
    )
}

fn too_long() -> ModelError {
    ModelError::new(ModelErrorKind::BadOutput, "the answer was far too long")
}

/// `error`, marked as happening after answer text was shown (then it isn't retried).
fn after(mut error: ModelError, text: &str) -> ModelError {
    error.after_output = !text.is_empty();
    error
}

/// A response body, capped at `max` bytes (an error body only carries a code and a message).
async fn read_body(
    response: &mut reqwest::Response,
    limit: Duration,
    max: usize,
    cancel: &CancellationToken,
) -> Result<String, Attempt> {
    let mut body = Vec::new();
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(Attempt::Cancelled),
            result = tokio::time::timeout(limit, response.chunk()) => result,
        };
        match chunk {
            Ok(Ok(Some(bytes))) => {
                body.extend_from_slice(&bytes);
                if body.len() >= max {
                    break;
                }
            }
            // The status is enough without a body.
            Ok(Ok(None)) | Ok(Err(_)) | Err(_) => break,
        }
    }
    body.truncate(max);
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Merge `extra` into `body`: objects key by key, anything else replaced.
fn merge(body: &mut serde_json::Value, extra: serde_json::Value) {
    match (body, extra) {
        (serde_json::Value::Object(body), serde_json::Value::Object(extra)) => {
            for (key, value) in extra {
                merge(body.entry(key).or_insert(serde_json::Value::Null), value);
            }
        }
        (body, extra) => *body = extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extra_body_merges_into_nested_objects() {
        let mut body = json!({ "model": "m", "provider": { "order": ["a"] } });
        merge(
            &mut body,
            json!({ "provider": { "data_collection": "deny" }, "x": 1 }),
        );
        assert_eq!(
            body,
            json!({ "model": "m", "provider": { "order": ["a"], "data_collection": "deny" }, "x": 1 })
        );
    }
}
