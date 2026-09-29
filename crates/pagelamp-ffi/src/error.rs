//! `PageLampError`: the one error every export throws in Swift.
//!
//! The facade's `AppError` is a struct (`kind` + `message`); Swift wants an enum it can
//! `switch` over, so each `AppErrorKind` becomes a case carrying the message. One case is
//! added by this crate:
//! - `Panic`: a bug made the Rust side of the call panic (caught at the task boundary; the
//!   panic hook installed by `init_diagnostics` records it as the last crash).
//!
//! Messages are user-presentable English and never contain secrets; UIs branch on the case
//! and localise by case, never by message.

use pagelamp_app::{AppError, AppErrorKind};
use pagelamp_core::ai::{BlockReason, ModelErrorKind};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum PageLampError {
    /// Token/feed URL rejected (expired, revoked, wrong).
    #[error("{message}")]
    Auth { message: String },
    /// Could not reach the server (DNS, TLS, timeout, refused).
    #[error("{message}")]
    Network { message: String },
    /// Bad input (malformed URL/date, folder is not a directory, …).
    #[error("{message}")]
    Invalid { message: String },
    /// Course/source/material does not exist.
    #[error("{message}")]
    NotFound { message: String },
    /// A course reference matched several courses; `message` lists them.
    #[error("{message}")]
    Ambiguous { message: String },
    /// Another window or command is syncing (`sync.lock` is held).
    #[error("{message}")]
    Busy { message: String },
    /// The database schema is newer than this build understands, or older and not migrated.
    #[error("{message}")]
    Schema { message: String },
    /// A model call was refused before anything was sent.
    #[error("{message}")]
    Blocked {
        message: String,
        reason: Option<BlockReason>,
    },
    /// A model call failed.
    #[error("{message}")]
    Model {
        message: String,
        kind: Option<ModelErrorKind>,
        retry_after_secs: Option<u32>,
    },
    /// The call was cancelled (a sync, a generation, a download).
    #[error("{message}")]
    Cancelled { message: String },
    /// Anything else (database, keychain, I/O).
    #[error("{message}")]
    Internal { message: String },
    /// The Rust side panicked (a bug).
    #[error("{message}")]
    Panic { message: String },
}

impl From<AppError> for PageLampError {
    fn from(err: AppError) -> Self {
        let AppError {
            kind,
            message,
            blocked,
            model_error,
            retry_after_secs,
        } = err;
        // Exhaustive on purpose: a new `AppErrorKind` must be mapped here.
        match kind {
            AppErrorKind::Auth => Self::Auth { message },
            AppErrorKind::Network => Self::Network { message },
            AppErrorKind::Invalid => Self::Invalid { message },
            AppErrorKind::NotFound => Self::NotFound { message },
            AppErrorKind::Ambiguous => Self::Ambiguous { message },
            AppErrorKind::Busy => Self::Busy { message },
            // One Swift case for both for now; the shell splits them later.
            AppErrorKind::SchemaTooNew | AppErrorKind::SchemaTooOld => Self::Schema { message },
            AppErrorKind::Blocked => Self::Blocked {
                message,
                reason: blocked,
            },
            AppErrorKind::Model => Self::Model {
                message,
                kind: model_error,
                retry_after_secs,
            },
            AppErrorKind::Cancelled => Self::Cancelled { message },
            AppErrorKind::Internal => Self::Internal { message },
        }
    }
}

/// A Swift `SyncObserver` failed in a way the generated code could not express.
impl From<uniffi::UnexpectedUniFFICallbackError> for PageLampError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Internal {
            message: format!("unexpected error in a Swift callback: {}", err.reason),
        }
    }
}

/// A task on the PageLamp runtime did not finish: it panicked, or the runtime is shutting down.
pub(crate) fn join_error(err: tokio::task::JoinError) -> PageLampError {
    if err.is_panic() {
        let payload = err.into_panic();
        let text = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        PageLampError::Panic {
            message: format!(
                "PageLamp hit an internal error: {}",
                pagelamp_core::diagnostics::redact(&text)
            ),
        }
    } else {
        PageLampError::Internal {
            message: "the operation was cancelled because PageLamp is shutting down".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_error_kind_maps_to_its_case() {
        let cases = [
            (AppErrorKind::Auth, "auth"),
            (AppErrorKind::Network, "network"),
            (AppErrorKind::Invalid, "invalid"),
            (AppErrorKind::NotFound, "not_found"),
            (AppErrorKind::Ambiguous, "ambiguous"),
            (AppErrorKind::Busy, "busy"),
            (AppErrorKind::SchemaTooNew, "schema_too_new"),
            (AppErrorKind::SchemaTooOld, "schema_too_old"),
            (AppErrorKind::Blocked, "blocked"),
            (AppErrorKind::Model, "model"),
            (AppErrorKind::Cancelled, "cancelled"),
            (AppErrorKind::Internal, "internal"),
        ];
        for (kind, message) in cases {
            let mapped = PageLampError::from(AppError::new(kind, message));
            let expected = match kind {
                AppErrorKind::Auth => PageLampError::Auth {
                    message: message.into(),
                },
                AppErrorKind::Network => PageLampError::Network {
                    message: message.into(),
                },
                AppErrorKind::Invalid => PageLampError::Invalid {
                    message: message.into(),
                },
                AppErrorKind::NotFound => PageLampError::NotFound {
                    message: message.into(),
                },
                AppErrorKind::Ambiguous => PageLampError::Ambiguous {
                    message: message.into(),
                },
                AppErrorKind::Busy => PageLampError::Busy {
                    message: message.into(),
                },
                AppErrorKind::SchemaTooNew | AppErrorKind::SchemaTooOld => PageLampError::Schema {
                    message: message.into(),
                },
                AppErrorKind::Blocked => PageLampError::Blocked {
                    message: message.into(),
                    reason: None,
                },
                AppErrorKind::Model => PageLampError::Model {
                    message: message.into(),
                    kind: None,
                    retry_after_secs: None,
                },
                AppErrorKind::Cancelled => PageLampError::Cancelled {
                    message: message.into(),
                },
                AppErrorKind::Internal => PageLampError::Internal {
                    message: message.into(),
                },
            };
            assert_eq!(mapped, expected);
            assert_eq!(mapped.to_string(), message, "Display is the message");
        }
    }

    #[test]
    fn model_errors_keep_their_reason_kind_and_wait() {
        let blocked = AppError::blocked(BlockReason::MaterialSharingNotAllowed, "not allowed");
        assert_eq!(
            PageLampError::from(blocked),
            PageLampError::Blocked {
                message: "not allowed".into(),
                reason: Some(BlockReason::MaterialSharingNotAllowed),
            }
        );
        let mut error = pagelamp_llm::ModelError::new(ModelErrorKind::RateLimited, "slow down");
        error.retry_after = Some(std::time::Duration::from_secs(7));
        let limited = AppError::from(pagelamp_llm::LlmError::Model(error));
        assert_eq!(
            PageLampError::from(limited),
            PageLampError::Model {
                message: "slow down".into(),
                kind: Some(ModelErrorKind::RateLimited),
                retry_after_secs: Some(7),
            }
        );
    }

    #[test]
    fn schema_errors_from_core_become_schema() {
        for core in [
            pagelamp_core::Error::SchemaTooNew {
                found: 12,
                supported: 7,
            },
            pagelamp_core::Error::SchemaTooOld {
                found: 3,
                supported: 7,
            },
        ] {
            let text = core.to_string();
            let mapped = PageLampError::from(AppError::from(core));
            assert_eq!(mapped, PageLampError::Schema { message: text });
        }
    }

    #[test]
    fn panics_and_cancellation_are_reported() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let panicked = runtime.block_on(async {
            tokio::spawn(async { panic!("boom with Bearer abcdef0123456789abcdef") })
                .await
                .unwrap_err()
        });
        match join_error(panicked) {
            PageLampError::Panic { message } => {
                assert!(message.contains("boom"), "{message}");
                assert!(
                    !message.contains("abcdef0123456789abcdef"),
                    "redacted: {message}"
                );
            }
            other => panic!("expected Panic, got {other:?}"),
        }
        let cancelled = runtime.block_on(async {
            let handle = tokio::spawn(std::future::pending::<()>());
            handle.abort();
            handle.await.unwrap_err()
        });
        assert!(matches!(
            join_error(cancelled),
            PageLampError::Internal { .. }
        ));
    }

    #[test]
    fn callback_errors_are_internal() {
        let err = uniffi::UnexpectedUniFFICallbackError::new("swift blew up");
        assert!(matches!(
            PageLampError::from(err),
            PageLampError::Internal { message } if message.contains("swift blew up")
        ));
    }
}
