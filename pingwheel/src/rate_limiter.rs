use std::time::{Duration, Instant};

/// Token-bucket rate limiter matching upstream `nx.pingwheel.common.util.RateLimiter`.
#[derive(Clone, Debug, Default)]
pub struct RateLimiter {
    start_time: Option<Instant>,
}

impl RateLimiter {
    /// Returns `true` if the player has exceeded the configured rate limit.
    pub fn check_exceeded(&mut self, now: Instant, ms_to_regenerate: u64, limit: u64) -> bool {
        if limit == 0 || ms_to_regenerate == 0 {
            return false;
        }
        let regen = Duration::from_millis(ms_to_regenerate);
        let window = Duration::from_millis(ms_to_regenerate.saturating_mul(limit));

        let Some(start) = self.start_time else {
            // First ping consumes 1 token from a full bucket.
            let initial_credit = window.saturating_sub(regen);
            self.start_time = now.checked_sub(initial_credit).or(Some(now));
            return false;
        };

        let elapsed = now.saturating_duration_since(start).min(window);
        let Some(left_over) = elapsed.checked_sub(regen) else {
            return true;
        };

        self.start_time = now.checked_sub(left_over).or(Some(now));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_burst_up_to_limit_then_throttles_until_regen() {
        let mut rl = RateLimiter::default();
        let t0 = Instant::now();

        for _ in 0..5 {
            assert!(!rl.check_exceeded(t0, 1000, 5));
        }
        // 6th immediate ping is throttled.
        assert!(rl.check_exceeded(t0, 1000, 5));

        // After 500ms still throttled.
        assert!(rl.check_exceeded(t0 + Duration::from_millis(500), 1000, 5));

        // After 1000ms one token regenerated.
        assert!(!rl.check_exceeded(t0 + Duration::from_millis(1000), 1000, 5));
        assert!(rl.check_exceeded(t0 + Duration::from_millis(1000), 1000, 5));
    }

    #[test]
    fn zero_limit_never_throttles() {
        let mut rl = RateLimiter::default();
        let t0 = Instant::now();
        for _ in 0..20 {
            assert!(!rl.check_exceeded(t0, 1000, 0));
        }
    }
}
