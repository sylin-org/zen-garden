//! Per-port exponential backoff.
//!
//! Each port failing an identity probe or spawn sequence enters
//! backoff. Subsequent attempts wait progressively longer: 5 s →
//! 30 s → 2 min → 5 min (capped). The schedule resets on any success
//! or on detach. Prevents busy-loops on flaky cables, dying flash,
//! or wrong-firmware devices and keeps logs readable.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SCHEDULE: &[Duration] = &[
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(2 * 60),
    Duration::from_secs(5 * 60),
];

#[derive(Debug, Clone)]
struct PortState {
    failures: u32,
    next_eligible: Instant,
}

/// Tracks per-port backoff state. Keyed by a stable port identifier
/// (the `DeviceHandle` / port path).
#[derive(Default)]
pub struct BackoffTracker {
    inner: Mutex<HashMap<String, PortState>>,
}

impl BackoffTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if the port is currently eligible (either never
    /// failed or past its next-eligible instant). `false` means skip
    /// this port this tick.
    pub fn is_eligible(&self, port: &str) -> bool {
        self.is_eligible_at(port, Instant::now())
    }

    /// Test hook: eligibility check against a caller-supplied `now`.
    pub fn is_eligible_at(&self, port: &str, now: Instant) -> bool {
        let inner = self.inner.lock().unwrap();
        match inner.get(port) {
            Some(state) => now >= state.next_eligible,
            None => true,
        }
    }

    /// Record a successful probe + spawn. Drops any backoff state.
    pub fn note_success(&self, port: &str) {
        self.inner.lock().unwrap().remove(port);
    }

    /// Record a probe / spawn failure. Advances the port's schedule
    /// index and sets the next-eligible instant.
    pub fn note_failure(&self, port: &str) {
        self.note_failure_at(port, Instant::now());
    }

    /// Test hook: record failure against a caller-supplied `now`.
    pub fn note_failure_at(&self, port: &str, now: Instant) {
        let mut inner = self.inner.lock().unwrap();
        let state = inner
            .entry(port.to_string())
            .or_insert(PortState {
                failures: 0,
                next_eligible: now,
            });
        state.failures = state.failures.saturating_add(1);
        let idx = ((state.failures as usize).saturating_sub(1)).min(SCHEDULE.len() - 1);
        state.next_eligible = now + SCHEDULE[idx];
    }

    /// Drop backoff state on detach. Next attach starts fresh.
    pub fn clear(&self, port: &str) {
        self.note_success(port);
    }

    /// Current consecutive failure count for `port` (0 if not tracked).
    pub fn failure_count(&self, port: &str) -> u32 {
        self.inner
            .lock()
            .unwrap()
            .get(port)
            .map(|s| s.failures)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_port_is_eligible() {
        let b = BackoffTracker::new();
        assert!(b.is_eligible("/dev/ttyUSB0"));
    }

    #[test]
    fn failure_blocks_eligibility_for_the_schedule_window() {
        let b = BackoffTracker::new();
        let t0 = Instant::now();
        b.note_failure_at("/dev/ttyUSB0", t0);
        // 5s backoff — not eligible at t0+4s, eligible at t0+6s.
        assert!(!b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(4)));
        assert!(b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(6)));
    }

    #[test]
    fn repeated_failures_escalate_through_schedule() {
        let b = BackoffTracker::new();
        let t0 = Instant::now();
        b.note_failure_at("/dev/ttyUSB0", t0);
        b.note_failure_at("/dev/ttyUSB0", t0);
        // Second failure = schedule[1] = 30s
        assert!(!b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(20)));
        assert!(b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(35)));
    }

    #[test]
    fn escalation_caps_at_final_schedule_entry() {
        let b = BackoffTracker::new();
        let t0 = Instant::now();
        for _ in 0..20 {
            b.note_failure_at("/dev/ttyUSB0", t0);
        }
        // Capped at 5 min.
        assert!(!b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(4 * 60)));
        assert!(b.is_eligible_at("/dev/ttyUSB0", t0 + Duration::from_secs(6 * 60)));
    }

    #[test]
    fn success_resets_backoff() {
        let b = BackoffTracker::new();
        b.note_failure_at("/dev/ttyUSB0", Instant::now());
        assert_eq!(b.failure_count("/dev/ttyUSB0"), 1);
        b.note_success("/dev/ttyUSB0");
        assert_eq!(b.failure_count("/dev/ttyUSB0"), 0);
        assert!(b.is_eligible("/dev/ttyUSB0"));
    }

    #[test]
    fn clear_is_alias_for_success() {
        let b = BackoffTracker::new();
        b.note_failure("/dev/ttyUSB0");
        b.clear("/dev/ttyUSB0");
        assert_eq!(b.failure_count("/dev/ttyUSB0"), 0);
    }

    #[test]
    fn per_port_state_is_independent() {
        let b = BackoffTracker::new();
        b.note_failure("/dev/ttyUSB0");
        assert!(b.is_eligible("/dev/ttyACM0"));
        assert_eq!(b.failure_count("/dev/ttyUSB0"), 1);
        assert_eq!(b.failure_count("/dev/ttyACM0"), 0);
    }
}
