//! When a failed call may be tried again: only before the first streamed byte, only for
//! failures that a wait can fix, and never for quota, spend or billing errors.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pagelamp_core::ai::ModelErrorKind;

use crate::error::ModelError;

/// Tries in total, the first one included.
pub(crate) const MAX_TRIES: u32 = 3;
/// The longest `retry-after` we wait for; longer waits fail instead.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);
/// First backoff for 5xx / connection errors; doubled per try, plus up to 50% jitter.
const BASE_DELAY: Duration = Duration::from_millis(500);

/// How long to wait before try `tries_done + 1`, or `None` to give up.
pub(crate) fn delay_before_retry(error: &ModelError, tries_done: u32) -> Option<Duration> {
    if tries_done >= MAX_TRIES || error.after_output {
        return None;
    }
    match error.kind {
        ModelErrorKind::RateLimited => match error.retry_after {
            Some(wait) if wait <= MAX_RETRY_AFTER => Some(wait),
            Some(_) => None,
            None => Some(backoff(tries_done)),
        },
        ModelErrorKind::Overloaded | ModelErrorKind::Network | ModelErrorKind::Timeout => {
            Some(backoff(tries_done))
        }
        _ => None,
    }
}

/// `BASE_DELAY × 2^(tries_done − 1)` plus up to 50% jitter.
fn backoff(tries_done: u32) -> Duration {
    let base = BASE_DELAY * 2u32.saturating_pow(tries_done.saturating_sub(1));
    // Jitter from the clock's nanoseconds: good enough to spread retries, no RNG needed.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    base + base.mul_f64(f64::from(nanos % 1000) / 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(kind: ModelErrorKind) -> ModelError {
        ModelError::new(kind, "demo")
    }

    #[test]
    fn only_waitable_failures_are_retried_and_at_most_three_tries() {
        for kind in [
            ModelErrorKind::Overloaded,
            ModelErrorKind::Network,
            ModelErrorKind::Timeout,
            ModelErrorKind::RateLimited,
        ] {
            let first = delay_before_retry(&error(kind), 1).unwrap();
            assert!(
                first >= BASE_DELAY && first <= BASE_DELAY * 3 / 2,
                "{kind:?} {first:?}"
            );
            assert!(delay_before_retry(&error(kind), 2).unwrap() >= BASE_DELAY * 2);
            assert_eq!(delay_before_retry(&error(kind), MAX_TRIES), None);
        }
        for kind in [
            ModelErrorKind::BillingOrQuota,
            ModelErrorKind::AuthRejected,
            ModelErrorKind::InvalidRequest,
            ModelErrorKind::ModelNotFound,
            ModelErrorKind::ContextTooLong,
            ModelErrorKind::Refused,
            ModelErrorKind::BadOutput,
        ] {
            assert_eq!(delay_before_retry(&error(kind), 1), None, "{kind:?}");
        }
    }

    #[test]
    fn retry_after_is_honoured_up_to_a_minute() {
        let limited = |secs| {
            error(ModelErrorKind::RateLimited).with_retry_after(Some(Duration::from_secs(secs)))
        };
        assert_eq!(
            delay_before_retry(&limited(7), 1),
            Some(Duration::from_secs(7))
        );
        assert_eq!(
            delay_before_retry(&limited(60), 1),
            Some(Duration::from_secs(60))
        );
        assert_eq!(delay_before_retry(&limited(61), 1), None);
    }

    #[test]
    fn nothing_is_retried_after_text_was_shown() {
        let mut overloaded = error(ModelErrorKind::Overloaded);
        overloaded.after_output = true;
        assert_eq!(delay_before_retry(&overloaded, 1), None);
    }
}
