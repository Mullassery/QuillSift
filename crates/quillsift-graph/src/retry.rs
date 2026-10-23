//! Bounded exponential backoff.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use quillsift_core::RetryPolicy;

/// Computes the backoff before retry number `attempt` (1-based).
///
/// Jitter is deterministic for a given node name and attempt so tests can be
/// exact; it is not cryptographic randomness.
pub fn backoff(policy: &RetryPolicy, attempt: u32, node: &str) -> Duration {
    let shift = attempt.saturating_sub(1).min(10);
    let exponential = policy.initial_backoff.saturating_mul(1u32 << shift);
    let capped = exponential.min(policy.max_backoff);
    if !policy.jitter {
        return capped;
    }

    let mut hasher = DefaultHasher::new();
    node.hash(&mut hasher);
    attempt.hash(&mut hasher);
    // Map to [0, 1000] without a lossy cast, then scale into [0.5, 1.0].
    let bucket = u32::try_from(hasher.finish() % 1001).unwrap_or(0);
    let factor = 0.5 + 0.5 * (f64::from(bucket) / 1000.0);
    capped.mul_f64(factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(jitter: bool) -> RetryPolicy {
        RetryPolicy {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(1000),
            jitter,
        }
    }

    #[test]
    fn grows_exponentially_and_caps() {
        let p = policy(false);
        assert_eq!(backoff(&p, 1, "n"), Duration::from_millis(100));
        assert_eq!(backoff(&p, 2, "n"), Duration::from_millis(200));
        assert_eq!(backoff(&p, 3, "n"), Duration::from_millis(400));
        assert_eq!(backoff(&p, 4, "n"), Duration::from_millis(800));
        assert_eq!(backoff(&p, 5, "n"), Duration::from_millis(1000));
        assert_eq!(backoff(&p, 50, "n"), Duration::from_millis(1000));
    }

    #[test]
    fn jitter_stays_within_half_and_full_backoff() {
        let p = policy(true);
        for attempt in 1..=5 {
            let d = backoff(&p, attempt, "node");
            let base = backoff(&policy(false), attempt, "node");
            assert!(d >= base.mul_f64(0.5), "{d:?} < half of {base:?}");
            assert!(d <= base, "{d:?} > {base:?}");
        }
    }

    #[test]
    fn jitter_is_deterministic() {
        let p = policy(true);
        assert_eq!(backoff(&p, 2, "a"), backoff(&p, 2, "a"));
    }
}
