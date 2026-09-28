//! `PageLampError`: the one error every export throws in Swift.
//!
//! The facade's `AppError` is a struct (`kind` + `message`); Swift wants an enum it can
//! `switch` over, so each `AppErrorKind` becomes a case carrying the message. Two cases are
//! added by this crate:
//! - `Schema`: the database was written by a newer (or not yet migrated by an older) PageLamp.
//!   The facade reports this as `Internal`; the text is recognised from `pagelamp-core`'s own
//!   `Display` of `Error::SchemaTooNew` / `Error::SchemaTooOld` (see `is_schema_message`), so a
//!   rewording in core cannot silently break it (a test pins it).
//! - `Panic`: a bug made the Rust side of the call panic (caught at the task boundary; the
//!   panic hook installed by `init_diagnostics` records it as the last crash).
//!
//! Messages are user-presentable English and never contain secrets; UIs branch on the case
//! and localise by case, never by message.

use pagelamp_app::{AppError, AppErrorKind};

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
    /// Anything else (database, keychain, I/O).
    #[error("{message}")]
    Internal { message: String },
    /// The Rust side panicked (a bug).
    #[error("{message}")]
    Panic { message: String },
}

impl From<AppError> for PageLampError {
    fn from(err: AppError) -> Self {
        let AppError { kind, message } = err;
        // Exhaustive on purpose: a new `AppErrorKind` must be mapped here.
        match kind {
            AppErrorKind::Auth => Self::Auth { message },
            AppErrorKind::Network => Self::Network { message },
            AppErrorKind::Invalid => Self::Invalid { message },
            AppErrorKind::NotFound => Self::NotFound { message },
            AppErrorKind::Ambiguous => Self::Ambiguous { message },
            AppErrorKind::Busy => Self::Busy { message },
            AppErrorKind::Internal if is_schema_message(&message) => Self::Schema { message },
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

/// Whether an `Internal` message is `pagelamp-core`'s `SchemaTooNew` / `SchemaTooOld` text.
/// The pattern is derived from core's own `Display` (rendered with sentinel numbers), so it
/// follows any rewording there.
fn is_schema_message(message: &str) -> bool {
    use pagelamp_core::Error;
    const SENTINEL: i64 = 987_654_321;
    [
        Error::SchemaTooNew {
            found: SENTINEL,
            supported: SENTINEL,
        },
        Error::SchemaTooOld {
            found: SENTINEL,
            supported: SENTINEL,
        },
    ]
    .iter()
    .any(|err| {
        let template = err.to_string();
        let parts: Vec<&str> = template.split(&SENTINEL.to_string()).collect();
        matches_with_numbers(message, &parts)
    })
}

/// `text` is `parts` joined by (possibly negative) integers.
fn matches_with_numbers(text: &str, parts: &[&str]) -> bool {
    let Some((first, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    for part in rest {
        let unsigned = remaining.strip_prefix('-').unwrap_or(remaining);
        let digits = unsigned.len()
            - unsigned
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        if digits == 0 {
            return false;
        }
        let Some(after) = unsigned[digits..].strip_prefix(part) else {
            return false;
        };
        remaining = after;
    }
    remaining.is_empty()
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
                AppErrorKind::Internal => PageLampError::Internal {
                    message: message.into(),
                },
            };
            assert_eq!(mapped, expected);
            assert_eq!(mapped.to_string(), message, "Display is the message");
        }
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
        // Other internal errors stay internal, including near misses.
        for message in [
            "database error: disk I/O error",
            "database schema version x is newer than supported version 7; please update PageLamp",
            "",
        ] {
            let mapped = PageLampError::from(AppError::new(AppErrorKind::Internal, message));
            assert!(
                matches!(mapped, PageLampError::Internal { .. }),
                "{message}"
            );
        }
    }

    #[test]
    fn numbers_between_parts() {
        assert!(matches_with_numbers("a1b-22c", &["a", "b", "c"]));
        assert!(!matches_with_numbers("a1bc", &["a", "b", "c"]));
        assert!(!matches_with_numbers("a1b2c!", &["a", "b", "c"]));
        assert!(matches_with_numbers("only", &["only"]));
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
