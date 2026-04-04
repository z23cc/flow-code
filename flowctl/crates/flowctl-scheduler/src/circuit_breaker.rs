//! Circuit breaker: N consecutive failures triggers scheduler halt.
//!
//! When the breaker opens, the scheduler cancels all in-flight tasks
//! via `CancellationToken` and stops dispatching new work.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use tracing::error;

/// Circuit breaker that opens after N consecutive task failures.
///
/// Thread-safe via atomics — no mutex needed for the hot path.
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Number of consecutive failures before tripping.
    threshold: u32,
    /// Current consecutive failure count.
    consecutive_failures: AtomicU32,
    /// Whether the breaker is open (tripped).
    open: AtomicBool,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given failure threshold.
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            consecutive_failures: AtomicU32::new(0),
            open: AtomicBool::new(false),
        }
    }

    /// Record a successful task completion. Resets the failure counter.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
    }

    /// Record a task failure. If consecutive failures reach the threshold,
    /// the breaker opens.
    pub fn record_failure(&self) {
        let prev = self.consecutive_failures.fetch_add(1, Ordering::SeqCst);
        let count = prev + 1;
        if count >= self.threshold {
            error!(
                consecutive_failures = count,
                threshold = self.threshold,
                "circuit breaker tripped — halting scheduler"
            );
            self.open.store(true, Ordering::SeqCst);
        }
    }

    /// Check whether the circuit breaker is open (tripped).
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    /// Reset the circuit breaker to closed state.
    pub fn reset(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.open.store(false, Ordering::SeqCst);
    }

    /// Get the current consecutive failure count.
    pub fn failure_count(&self) -> u32 {
        self.consecutive_failures.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let cb = CircuitBreaker::new(3);
        assert!(!cb.is_open());
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_trips_at_threshold() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure();
        assert!(!cb.is_open());
        cb.record_failure();
        assert!(!cb.is_open());
        cb.record_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn test_success_resets_counter() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.failure_count(), 2);
        cb.record_success();
        assert_eq!(cb.failure_count(), 0);
        assert!(!cb.is_open());
    }

    #[test]
    fn test_success_between_failures_prevents_trip() {
        let cb = CircuitBreaker::new(3);
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // reset
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_open());
    }

    #[test]
    fn test_reset() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open());
        cb.reset();
        assert!(!cb.is_open());
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn test_threshold_one() {
        let cb = CircuitBreaker::new(1);
        assert!(!cb.is_open());
        cb.record_failure();
        assert!(cb.is_open());
    }

    #[test]
    fn test_stays_open_after_trip() {
        let cb = CircuitBreaker::new(2);
        cb.record_failure();
        cb.record_failure();
        assert!(cb.is_open());
        // Success doesn't close the breaker — only reset does.
        cb.record_success();
        assert!(cb.is_open());
    }
}
