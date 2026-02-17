//! Exponential backoff policy and sleep-with-abort utility.
//!
//! Ports OpenClaw's `src/infra/backoff.ts` to idiomatic Rust, providing:
//! - [`BackoffPolicy`] configuration for retry strategies
//! - [`compute_backoff`] to calculate delay for a given attempt
//! - [`sleep_with_abort`] for cancellable async sleep

use rand::Rng;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// BackoffPolicy
// ---------------------------------------------------------------------------

/// Configuration for exponential backoff with optional jitter.
///
/// ```
/// use claw_core::backoff::{BackoffPolicy, compute_backoff};
///
/// let policy = BackoffPolicy { jitter: 0.0, ..BackoffPolicy::default() };
/// let delay = compute_backoff(&policy, 0);
/// assert_eq!(delay.as_millis(), 1000);
/// ```
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    /// Base delay in milliseconds (before exponential scaling).
    pub base_ms: u64,
    /// Maximum delay in milliseconds (cap).
    pub max_ms: u64,
    /// Multiplier applied per attempt (e.g. 2.0 for doubling).
    pub factor: f64,
    /// Random jitter factor in `[0.0, 1.0]`. 0 means no jitter.
    pub jitter: f64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            base_ms: 1_000,
            max_ms: 30_000,
            factor: 2.0,
            jitter: 0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// compute_backoff
// ---------------------------------------------------------------------------

/// Compute the backoff delay for a given `attempt` (0-indexed).
///
/// Formula: `min(base_ms * factor^attempt, max_ms)` with additive jitter.
///
/// ```
/// use claw_core::backoff::{BackoffPolicy, compute_backoff};
///
/// let policy = BackoffPolicy { base_ms: 100, max_ms: 5000, factor: 2.0, jitter: 0.0 };
/// assert_eq!(compute_backoff(&policy, 0).as_millis(), 100);
/// assert_eq!(compute_backoff(&policy, 1).as_millis(), 200);
/// assert_eq!(compute_backoff(&policy, 2).as_millis(), 400);
/// assert_eq!(compute_backoff(&policy, 10).as_millis(), 5000); // capped
/// ```
pub fn compute_backoff(policy: &BackoffPolicy, attempt: u32) -> Duration {
    let raw = (policy.base_ms as f64) * policy.factor.powi(attempt as i32);
    let capped = raw.min(policy.max_ms as f64);

    let jittered = if policy.jitter > 0.0 {
        let jitter_amount = capped * policy.jitter;
        let offset = rand::rng().random_range(-jitter_amount..=jitter_amount);
        (capped + offset).max(0.0)
    } else {
        capped
    };

    Duration::from_millis(jittered as u64)
}

// ---------------------------------------------------------------------------
// sleep_with_abort
// ---------------------------------------------------------------------------

/// Sleep for `duration`, but return early with `Err` if `cancel` fires.
///
/// Returns `Ok(())` if the full duration elapsed, or an error if cancelled.
///
/// ```no_run
/// # use tokio_util::sync::CancellationToken;
/// # use std::time::Duration;
/// # async fn example() {
/// let cancel = CancellationToken::new();
/// let result = claw_core::backoff::sleep_with_abort(
///     Duration::from_secs(5),
///     cancel,
/// ).await;
/// # }
/// ```
pub async fn sleep_with_abort(
    duration: Duration,
    cancel: CancellationToken,
) -> Result<(), SleepAbortedError> {
    tokio::select! {
        _ = cancel.cancelled() => Err(SleepAbortedError),
        _ = tokio::time::sleep(duration) => Ok(()),
    }
}

/// Returned by [`sleep_with_abort`] when the cancellation token fires.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("sleep aborted by cancellation")]
pub struct SleepAbortedError;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- BackoffPolicy defaults -------------------------------------------

    #[test]
    fn default_policy() {
        let p = BackoffPolicy::default();
        assert_eq!(p.base_ms, 1_000);
        assert_eq!(p.max_ms, 30_000);
        assert!((p.factor - 2.0).abs() < f64::EPSILON);
        assert!((p.jitter - 0.1).abs() < f64::EPSILON);
    }

    // -- compute_backoff --------------------------------------------------

    #[test]
    fn backoff_no_jitter_doubles() {
        let policy = BackoffPolicy {
            base_ms: 100,
            max_ms: 10_000,
            factor: 2.0,
            jitter: 0.0,
        };
        assert_eq!(compute_backoff(&policy, 0).as_millis(), 100);
        assert_eq!(compute_backoff(&policy, 1).as_millis(), 200);
        assert_eq!(compute_backoff(&policy, 2).as_millis(), 400);
        assert_eq!(compute_backoff(&policy, 3).as_millis(), 800);
    }

    #[test]
    fn backoff_caps_at_max() {
        let policy = BackoffPolicy {
            base_ms: 1_000,
            max_ms: 5_000,
            factor: 2.0,
            jitter: 0.0,
        };
        assert_eq!(compute_backoff(&policy, 0).as_millis(), 1_000);
        assert_eq!(compute_backoff(&policy, 3).as_millis(), 5_000); // 8000 capped
        assert_eq!(compute_backoff(&policy, 10).as_millis(), 5_000);
    }

    #[test]
    fn backoff_with_jitter_stays_in_range() {
        let policy = BackoffPolicy {
            base_ms: 1_000,
            max_ms: 30_000,
            factor: 2.0,
            jitter: 0.5,
        };
        for attempt in 0..5 {
            let raw = (policy.base_ms as f64) * policy.factor.powi(attempt as i32);
            let capped = raw.min(policy.max_ms as f64);
            let max_jitter = capped * policy.jitter;

            for _ in 0..20 {
                let delay = compute_backoff(&policy, attempt);
                let ms = delay.as_millis() as f64;
                assert!(
                    ms >= (capped - max_jitter).max(0.0) && ms <= capped + max_jitter,
                    "delay {ms}ms out of range for attempt {attempt}"
                );
            }
        }
    }

    #[test]
    fn backoff_attempt_zero_returns_base() {
        let policy = BackoffPolicy {
            base_ms: 500,
            max_ms: 60_000,
            factor: 3.0,
            jitter: 0.0,
        };
        assert_eq!(compute_backoff(&policy, 0).as_millis(), 500);
    }

    // -- sleep_with_abort -------------------------------------------------

    #[tokio::test]
    async fn sleep_completes_normally() {
        let cancel = CancellationToken::new();
        let result = sleep_with_abort(Duration::from_millis(10), cancel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn sleep_aborted_by_cancel() {
        let cancel = CancellationToken::new();
        let token = cancel.clone();

        // Cancel immediately
        token.cancel();

        let result = sleep_with_abort(Duration::from_secs(60), cancel).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "sleep aborted by cancellation");
    }

    #[tokio::test]
    async fn sleep_aborted_mid_wait() {
        let cancel = CancellationToken::new();
        let token = cancel.clone();

        // Cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token.cancel();
        });

        let result = sleep_with_abort(Duration::from_secs(60), cancel).await;
        assert!(result.is_err());
    }
}
