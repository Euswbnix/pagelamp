//! One module per wire dialect: how a `GenerateRequest` becomes a request body, how the stream
//! becomes `Step`s, and how an error response becomes a `ModelError`. The backend runs them
//! (`backend.rs`); nothing here does I/O.

pub(crate) mod anthropic;
pub(crate) mod openai_responses;

use pagelamp_core::ai::ModelErrorKind;
use reqwest::header::HeaderMap;

use crate::error::ModelError;
use crate::profile::ProviderProfile;
use crate::request::{GenerateRequest, StopReason, Usage};
use crate::sse::{Framing, RawEvent};

/// What one stream event means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Text(String),
    Usage(Usage),
    /// The answer is complete.
    Done {
        stop: StopReason,
        model: Option<String>,
    },
}

/// A wire dialect.
pub(crate) trait Dialect {
    const FRAMING: Framing;
    /// Per-stream parsing state.
    type State: Default;

    /// The path under the base URL that generates.
    fn generate_path(model: &str) -> String;
    /// Headers every request of this wire needs (not the key).
    fn headers() -> HeaderMap {
        HeaderMap::new()
    }
    fn request_body(req: &GenerateRequest, profile: &ProviderProfile) -> serde_json::Value;
    fn on_event(state: &mut Self::State, event: &RawEvent) -> Result<Vec<Step>, ModelError>;
    /// An error response (`status` ≥ 400) as a model error.
    fn map_error(status: u16, body: &str) -> ModelError;
}

/// The kind an HTTP status means when the body says nothing more specific.
pub(crate) fn kind_for_status(status: u16) -> ModelErrorKind {
    match status {
        401 | 403 => ModelErrorKind::AuthRejected,
        402 => ModelErrorKind::BillingOrQuota,
        404 => ModelErrorKind::ModelNotFound,
        408 => ModelErrorKind::Timeout,
        413 => ModelErrorKind::ContextTooLong,
        429 => ModelErrorKind::RateLimited,
        500..=599 => ModelErrorKind::Overloaded,
        _ => ModelErrorKind::InvalidRequest,
    }
}

/// Parse an event's JSON data (a broken event is the provider's bad output).
pub(crate) fn event_json(event: &RawEvent) -> Result<serde_json::Value, ModelError> {
    serde_json::from_str(&event.data).map_err(|_| {
        ModelError::new(
            ModelErrorKind::BadOutput,
            "the service sent a stream event that isn't valid JSON",
        )
    })
}

/// A JSON number as u64 (missing or negative → 0).
pub(crate) fn count(value: &serde_json::Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// Generic text for a status when the provider gave no message.
pub(crate) fn status_message(status: u16) -> String {
    format!("the service answered with HTTP {status}")
}
