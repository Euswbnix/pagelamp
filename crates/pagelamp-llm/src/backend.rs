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
use crate::request::{GenerateRequest, Outcome, OutputSpec, StopReason, StreamEvent, Usage};
use crate::retry::delay_before_retry;
use crate::sse::EventParser;
use crate::wire::anthropic::AnthropicMessages;
use crate::wire::openai_responses::OpenAiResponses;
use crate::wire::{Dialect, Step};

/// The most answer text accepted from one call.
const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// The most error-response body read (it only carries a code and a message).
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

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
                self.run::<OpenAiResponses>(&request, on_event, &cancel)
                    .await
            }
            Wire::AnthropicMessages => {
                self.run::<AnthropicMessages>(&request, on_event, &cancel)
                    .await
            }
            Wire::OpenAiChat | Wire::OllamaNative => Err(LlmError::Model(ModelError::new(
                ModelErrorKind::Unsupported,
                "this provider type is not available yet",
            ))),
        }
    }

    /// The full request body for `request` on this provider (also used by the per-wire golden
    /// tests: exactly these bytes are sent).
    pub fn request_body(&self, request: &GenerateRequest) -> serde_json::Value {
        match self.profile.wire {
            Wire::OpenAiResponses => self.body::<OpenAiResponses>(request),
            Wire::AnthropicMessages => self.body::<AnthropicMessages>(request),
            Wire::OpenAiChat | Wire::OllamaNative => serde_json::Value::Null,
        }
    }

    fn body<D: Dialect>(&self, request: &GenerateRequest) -> serde_json::Value {
        let mut body = D::request_body(request, &self.profile);
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

    async fn run<D: Dialect>(
        &self,
        request: &GenerateRequest,
        on_event: &(dyn Fn(StreamEvent) + Send + Sync),
        cancel: &CancellationToken,
    ) -> Result<Outcome, LlmError> {
        let url = join_path(&self.base_url, &D::generate_path(&request.model));
        let body = self.body::<D>(request);
        let headers = self.headers::<D>()?;
        let mut tries = 0;
        loop {
            tries += 1;
            let error = match self
                .attempt::<D>(&url, &headers, &body, request, on_event, cancel)
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
        request: &GenerateRequest,
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
            let body = read_error_body(&mut response, self.timeouts.idle, cancel).await?;
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
            for event in &events {
                let steps = D::on_event(&mut state, event).map_err(|error| after(error, &text))?;
                for step in steps {
                    match step {
                        Step::Text(delta) => {
                            if text.len() + delta.len() > MAX_OUTPUT_BYTES {
                                return Err(after(too_long(), &text).into());
                            }
                            text.push_str(&delta);
                            on_event(StreamEvent::TextDelta(delta));
                        }
                        Step::Usage(new) => {
                            usage = new;
                            on_event(StreamEvent::Usage(usage));
                        }
                        Step::Done { stop, model } => finished = Some((stop, model)),
                    }
                }
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
        let json = match (&request.output, &stop) {
            (OutputSpec::Json { .. }, StopReason::Complete) => {
                Some(serde_json::from_str(&text).map_err(|_| {
                    ModelError::new(ModelErrorKind::BadOutput, "the answer is not valid JSON")
                })?)
            }
            _ => None,
        };
        Ok(Outcome {
            text,
            json,
            stop,
            usage,
            request_id,
            model_reported,
        })
    }
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

/// The body of an error response, capped (it only carries a code and a message).
async fn read_error_body(
    response: &mut reqwest::Response,
    limit: Duration,
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
                if body.len() >= MAX_ERROR_BODY_BYTES {
                    break;
                }
            }
            // The status is enough without a body.
            Ok(Ok(None)) | Ok(Err(_)) | Err(_) => break,
        }
    }
    body.truncate(MAX_ERROR_BODY_BYTES);
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
