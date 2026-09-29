//! Errors of a model call. UIs branch on `ModelErrorKind` / `BlockReason`, never on messages.

use std::time::Duration;

use pagelamp_core::ai::{BlockReason, ModelErrorKind};

/// Longest provider message kept in a `ModelError` (it is shown to the student, never logged).
const MAX_MESSAGE_CHARS: usize = 300;

#[derive(Debug, Clone, thiserror::Error)]
pub enum LlmError {
    /// The caller's `CancellationToken` fired.
    #[error("cancelled")]
    Cancelled,
    /// Refused before anything was sent (e.g. a coding-plan key). `message` may quote the
    /// vendor's own terms.
    #[error("{message}")]
    Blocked {
        reason: BlockReason,
        message: String,
    },
    #[error("{0}")]
    Model(ModelError),
}

impl LlmError {
    /// The model error kind, if this is a model error.
    pub fn model_kind(&self) -> Option<ModelErrorKind> {
        match self {
            LlmError::Model(error) => Some(error.kind),
            _ => None,
        }
    }
}

impl From<ModelError> for LlmError {
    fn from(error: ModelError) -> Self {
        LlmError::Model(error)
    }
}

/// A failed model call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ModelError {
    pub kind: ModelErrorKind,
    /// For the student: ours, or the provider's own message clipped to 300 characters. Never
    /// contains the key. Not logged (a provider may echo parts of the request).
    pub message: String,
    /// How long the provider asked us to wait (`retry-after`).
    pub retry_after: Option<Duration>,
    pub http_status: Option<u16>,
    /// The provider's request id, for a support request.
    pub request_id: Option<String>,
    /// The provider's error code (`insufficient_quota`, `overloaded_error`, …), for logs.
    pub provider_code: Option<String>,
    /// Text had already been streamed when it failed: the caller offers "Try again" instead of
    /// retrying by itself.
    pub after_output: bool,
}

impl ModelError {
    pub fn new(kind: ModelErrorKind, message: impl Into<String>) -> ModelError {
        ModelError {
            kind,
            message: clip(&message.into()),
            retry_after: None,
            http_status: None,
            request_id: None,
            provider_code: None,
            after_output: false,
        }
    }

    pub(crate) fn with_status(mut self, status: u16) -> ModelError {
        self.http_status = Some(status);
        self
    }

    pub(crate) fn with_code(mut self, code: Option<String>) -> ModelError {
        self.provider_code = code;
        self
    }

    pub(crate) fn with_retry_after(mut self, retry_after: Option<Duration>) -> ModelError {
        self.retry_after = retry_after;
        self
    }

    pub(crate) fn with_request_id(mut self, request_id: Option<String>) -> ModelError {
        self.request_id = request_id;
        self
    }
}

/// `text` cut to `MAX_MESSAGE_CHARS` characters (with "…"), on one line.
pub(crate) fn clip(text: &str) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= MAX_MESSAGE_CHARS {
        return one_line;
    }
    let mut clipped: String = one_line.chars().take(MAX_MESSAGE_CHARS - 1).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_messages_are_clipped_to_one_line() {
        assert_eq!(clip("model\n  not   found"), "model not found");
        let long = "x".repeat(MAX_MESSAGE_CHARS * 2);
        let clipped = clip(&long);
        assert_eq!(clipped.chars().count(), MAX_MESSAGE_CHARS);
        assert!(clipped.ends_with('…'));
    }
}
