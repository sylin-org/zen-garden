//! Capacity watermarks and the disk-pressure state machine (STORAGE-0020).
//!
//! The [`Budget`] holds the policy — at what fill levels the governor
//! observes, reclaims, and refuses writes. [`Pressure`] is the classified
//! state; it is an enum, not a set of bool flags, so impossible states
//! (e.g. "critical but healthy") are unrepresentable.

use serde::Serialize;

/// Default fill percentage at which gentle hygiene begins.
pub const DEFAULT_ELEVATED_PERCENT: f64 = 75.0;
/// Default fill percentage at which retention tightens.
pub const DEFAULT_HIGH_PERCENT: f64 = 85.0;
/// Default fill percentage at which writes are denied and reclaim is aggressive.
pub const DEFAULT_CRITICAL_PERCENT: f64 = 95.0;
/// Default absolute free-space floor below which large writes are denied,
/// regardless of percentage. 3 GiB leaves room for the OS, logs, and an
/// in-flight container image even on a small appliance disk.
pub const DEFAULT_MIN_FREE_BYTES: u64 = 3 * 1024 * 1024 * 1024;

/// Disk-fill pressure, classified from a filesystem's used percentage.
///
/// Ordered: `Healthy < Elevated < High < Critical`. The ordering is load
/// bearing — the reclaim loop stops once pressure drops below `High`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pressure {
    /// Below the elevated watermark — nothing to do.
    Healthy,
    /// Gentle hygiene: reap orphaned captures, remove leaked images.
    Elevated,
    /// Tighten retention to reclaim space.
    High,
    /// Aggressive reclaim and deny new large writes.
    Critical,
}

impl Pressure {
    /// Stable name for metric per-kind counters.
    pub fn name(&self) -> &'static str {
        match self {
            Pressure::Healthy => "healthy",
            Pressure::Elevated => "elevated",
            Pressure::High => "high",
            Pressure::Critical => "critical",
        }
    }
}

/// How hard a [`Reclaimable`](super::Reclaimable) should work this pass.
///
/// The level scales aggressiveness; each reclaimer maps it to its own
/// floor and never deletes everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReclaimLevel {
    /// Cheap hygiene only — reap orphans, remove leaked images, keep the
    /// standard retention count.
    Routine,
    /// Tighten retention below the standard count.
    Pressure,
    /// Last resort before zero free space — keep the bare minimum.
    Critical,
}

/// Capacity policy: the watermarks and admission floor for one filesystem.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// Used-percentage at which `Elevated` pressure begins.
    pub elevated_percent: f64,
    /// Used-percentage at which `High` pressure begins.
    pub high_percent: f64,
    /// Used-percentage at which `Critical` pressure begins.
    pub critical_percent: f64,
    /// Absolute free-space floor for admission control.
    pub min_free_bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            elevated_percent: DEFAULT_ELEVATED_PERCENT,
            high_percent: DEFAULT_HIGH_PERCENT,
            critical_percent: DEFAULT_CRITICAL_PERCENT,
            min_free_bytes: DEFAULT_MIN_FREE_BYTES,
        }
    }
}

impl Budget {
    /// Build a [`Budget`] from the environment, falling back to defaults.
    ///
    /// Overrides: `ZG_CAPACITY_ELEVATED_PERCENT`, `ZG_CAPACITY_HIGH_PERCENT`,
    /// `ZG_CAPACITY_CRITICAL_PERCENT`, `ZG_CAPACITY_MIN_FREE_BYTES`. An
    /// unparseable value is ignored (default kept) with a warning, so a
    /// typo can never silently disable the floor.
    pub fn from_env() -> Self {
        let mut budget = Budget::default();
        budget.elevated_percent =
            env_f64("ZG_CAPACITY_ELEVATED_PERCENT", budget.elevated_percent);
        budget.high_percent = env_f64("ZG_CAPACITY_HIGH_PERCENT", budget.high_percent);
        budget.critical_percent =
            env_f64("ZG_CAPACITY_CRITICAL_PERCENT", budget.critical_percent);
        budget.min_free_bytes = env_u64("ZG_CAPACITY_MIN_FREE_BYTES", budget.min_free_bytes);
        budget
    }

    /// Classify a used-percentage into a [`Pressure`] level.
    pub fn classify(&self, used_percent: f64) -> Pressure {
        if used_percent >= self.critical_percent {
            Pressure::Critical
        } else if used_percent >= self.high_percent {
            Pressure::High
        } else if used_percent >= self.elevated_percent {
            Pressure::Elevated
        } else {
            Pressure::Healthy
        }
    }

    /// The reclaim level to run for a given pressure, or `None` when no
    /// reclamation is warranted (`Healthy`).
    pub fn reclaim_level(&self, pressure: Pressure) -> Option<ReclaimLevel> {
        match pressure {
            Pressure::Healthy => None,
            Pressure::Elevated => Some(ReclaimLevel::Routine),
            Pressure::High => Some(ReclaimLevel::Pressure),
            Pressure::Critical => Some(ReclaimLevel::Critical),
        }
    }

    /// Whether a large write must be denied: free space below the floor,
    /// or the filesystem already at `Critical`.
    pub fn deny_admission(&self, used_percent: f64, available_bytes: u64) -> bool {
        available_bytes < self.min_free_bytes
            || self.classify(used_percent) == Pressure::Critical
    }
}

fn env_f64(key: &str, fallback: f64) -> f64 {
    match std::env::var(key) {
        Ok(raw) => match raw.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => v,
            _ => {
                tracing::warn!(env = key, value = %raw, "ignoring unparseable capacity override");
                fallback
            }
        },
        Err(_) => fallback,
    }
}

fn env_u64(key: &str, fallback: u64) -> u64 {
    match std::env::var(key) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!(env = key, value = %raw, "ignoring unparseable capacity override");
                fallback
            }
        },
        Err(_) => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> Budget {
        Budget::default()
    }

    #[test]
    fn classify_spans_each_band() {
        let b = budget();
        assert_eq!(b.classify(0.0), Pressure::Healthy);
        assert_eq!(b.classify(74.9), Pressure::Healthy);
        assert_eq!(b.classify(75.0), Pressure::Elevated);
        assert_eq!(b.classify(84.9), Pressure::Elevated);
        assert_eq!(b.classify(85.0), Pressure::High);
        assert_eq!(b.classify(94.9), Pressure::High);
        assert_eq!(b.classify(95.0), Pressure::Critical);
        assert_eq!(b.classify(100.0), Pressure::Critical);
    }

    #[test]
    fn pressure_orders_ascending() {
        assert!(Pressure::Healthy < Pressure::Elevated);
        assert!(Pressure::Elevated < Pressure::High);
        assert!(Pressure::High < Pressure::Critical);
    }

    #[test]
    fn reclaim_level_is_graduated() {
        let b = budget();
        assert_eq!(b.reclaim_level(Pressure::Healthy), None);
        assert_eq!(
            b.reclaim_level(Pressure::Elevated),
            Some(ReclaimLevel::Routine)
        );
        assert_eq!(
            b.reclaim_level(Pressure::High),
            Some(ReclaimLevel::Pressure)
        );
        assert_eq!(
            b.reclaim_level(Pressure::Critical),
            Some(ReclaimLevel::Critical)
        );
    }

    #[test]
    fn admission_denied_below_floor_even_when_healthy() {
        let b = budget();
        // 10% used (healthy) but only 1 GiB free → still denied.
        assert!(b.deny_admission(10.0, 1024 * 1024 * 1024));
    }

    #[test]
    fn admission_denied_at_critical_even_with_bytes_free() {
        let b = budget();
        // 96% used but 100 GiB free (huge disk) → denied on percentage.
        assert!(b.deny_admission(96.0, 100 * 1024 * 1024 * 1024));
    }

    #[test]
    fn admission_allowed_when_healthy_and_above_floor() {
        let b = budget();
        assert!(!b.deny_admission(50.0, 50 * 1024 * 1024 * 1024));
    }
}
