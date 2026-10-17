//! Exponential backoff with jitter, so thousands of launchers never retry in lockstep.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    /// First delay (before jitter).
    pub initial: Duration,
    /// Upper bound of any delay.
    pub max: Duration,
    /// Spread of the first reconnect after a connection that was up for a while. Everyone loses
    /// their socket at the same moment when Cloudflare restarts a server; this spreads the
    /// reconnects over a few seconds.
    pub spread: Duration,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(2),
            max: Duration::from_secs(300),
            spread: Duration::from_secs(5),
        }
    }
}

/// Doubles from `initial` up to `max`; each delay is drawn from `[d/2, d]` ("equal jitter").
#[derive(Debug, Clone)]
pub struct Backoff {
    policy: BackoffPolicy,
    attempt: u32,
}

impl Backoff {
    pub fn new(policy: BackoffPolicy) -> Self {
        Self { policy, attempt: 0 }
    }

    /// Back to the first step (after a healthy connection).
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    pub fn next_delay(&mut self) -> Duration {
        if self.attempt == 0 && !self.policy.spread.is_zero() {
            self.attempt = 1;
            return jitter(Duration::ZERO, self.policy.spread);
        }
        let exponent = self.attempt.saturating_sub(1).min(16);
        let ceiling = self
            .policy
            .initial
            .saturating_mul(1u32 << exponent)
            .min(self.policy.max);
        self.attempt = self.attempt.saturating_add(1);
        jitter(ceiling / 2, ceiling)
    }
}

/// A uniformly random duration in `[low, high]`.
pub(crate) fn jitter(low: Duration, high: Duration) -> Duration {
    if high <= low {
        return low;
    }
    let span = u64::try_from((high - low).as_millis()).unwrap_or(u64::MAX);
    low + Duration::from_millis(rand::random_range(0..=span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grows_caps_and_resets() {
        let policy = BackoffPolicy {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(10),
            spread: Duration::from_secs(5),
        };
        let mut backoff = Backoff::new(policy);
        let first = backoff.next_delay();
        assert!(first <= Duration::from_secs(5));
        let ceilings = [1u64, 2, 4, 8, 10, 10, 10];
        for ceiling in ceilings {
            let delay = backoff.next_delay();
            let ceiling = Duration::from_secs(ceiling);
            assert!(
                delay >= ceiling / 2 && delay <= ceiling,
                "{delay:?} vs {ceiling:?}"
            );
        }
        backoff.reset();
        assert!(backoff.next_delay() <= Duration::from_secs(5));
    }

    #[test]
    fn delays_are_spread_out() {
        let policy = BackoffPolicy {
            initial: Duration::from_secs(8),
            ..BackoffPolicy::default()
        };
        let delays: Vec<Duration> = (0..50)
            .map(|_| {
                let mut backoff = Backoff::new(policy);
                backoff.next_delay();
                backoff.next_delay()
            })
            .collect();
        let distinct: std::collections::HashSet<_> = delays.iter().collect();
        assert!(distinct.len() > 10, "jitter must spread clients apart");
        assert!(
            delays
                .iter()
                .all(|d| *d >= Duration::from_secs(4) && *d <= Duration::from_secs(8))
        );
    }

    #[test]
    fn jitter_bounds() {
        assert_eq!(
            jitter(Duration::from_secs(3), Duration::from_secs(3)),
            Duration::from_secs(3)
        );
        for _ in 0..100 {
            let d = jitter(Duration::from_millis(10), Duration::from_millis(20));
            assert!(d >= Duration::from_millis(10) && d <= Duration::from_millis(20));
        }
    }
}
