//! Oplog guardian — evaluates oplog health and safety margins.
//!
//! Monitors the oplog window to ensure secondaries can catch up.
//! The safety ratio is: `oplog_window / max_secondary_lag`.
//! When this drops below thresholds, the oplog is at risk.

use serde::Serialize;

/// Oplog health snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct OplogHealth {
    /// Oplog window in seconds (time range covered by oplog).
    pub window_secs: f64,
    /// Oplog used in MB.
    pub used_mb: f64,
    /// Oplog max size in MB.
    pub total_mb: f64,
    /// Maximum replication lag across all secondaries (seconds).
    pub max_lag_secs: f64,
    /// Safety ratio = window / max_lag (higher is safer).
    pub safety_ratio: f64,
    /// Severity assessment.
    pub severity: OplogSeverity,
}

/// Oplog severity levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OplogSeverity {
    /// Window >> lag. No risk.
    Healthy,
    /// Window is 5-10x lag. Monitor closely.
    Warning,
    /// Window is 2-5x lag. Risk of falling behind.
    Danger,
    /// Window is 1-2x lag. Imminent data loss risk.
    Critical,
    /// Window < lag. Secondary cannot catch up.
    Unrecoverable,
}

impl std::fmt::Display for OplogSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "HEALTHY"),
            Self::Warning => write!(f, "WARNING"),
            Self::Danger => write!(f, "DANGER"),
            Self::Critical => write!(f, "CRITICAL"),
            Self::Unrecoverable => write!(f, "UNRECOVERABLE"),
        }
    }
}

/// Evaluate oplog health given replication info and secondary lag.
pub fn evaluate_oplog(
    window_secs: f64,
    used_mb: f64,
    total_mb: f64,
    max_lag_secs: f64,
) -> OplogHealth {
    let safety_ratio = if max_lag_secs > 0.0 {
        window_secs / max_lag_secs
    } else {
        // No lag = infinitely safe, but cap for serialization
        f64::MAX.min(999.0)
    };

    let severity = oplog_severity(safety_ratio);

    OplogHealth {
        window_secs,
        used_mb,
        total_mb,
        max_lag_secs,
        safety_ratio,
        severity,
    }
}

/// Map safety ratio to severity level.
fn oplog_severity(safety_ratio: f64) -> OplogSeverity {
    if safety_ratio >= 10.0 {
        OplogSeverity::Healthy
    } else if safety_ratio >= 5.0 {
        OplogSeverity::Warning
    } else if safety_ratio >= 2.0 {
        OplogSeverity::Danger
    } else if safety_ratio >= 1.0 {
        OplogSeverity::Critical
    } else {
        OplogSeverity::Unrecoverable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healthy_oplog() {
        let health = evaluate_oplog(86400.0, 100.0, 1024.0, 2.0);
        assert_eq!(health.severity, OplogSeverity::Healthy);
        assert!(health.safety_ratio > 10.0);
    }

    #[test]
    fn test_warning_oplog() {
        let health = evaluate_oplog(50.0, 100.0, 1024.0, 8.0);
        assert_eq!(health.severity, OplogSeverity::Warning);
    }

    #[test]
    fn test_danger_oplog() {
        let health = evaluate_oplog(20.0, 100.0, 1024.0, 8.0);
        assert_eq!(health.severity, OplogSeverity::Danger);
    }

    #[test]
    fn test_critical_oplog() {
        let health = evaluate_oplog(10.0, 100.0, 1024.0, 8.0);
        assert_eq!(health.severity, OplogSeverity::Critical);
    }

    #[test]
    fn test_unrecoverable_oplog() {
        let health = evaluate_oplog(5.0, 100.0, 1024.0, 10.0);
        assert_eq!(health.severity, OplogSeverity::Unrecoverable);
    }

    #[test]
    fn test_no_lag() {
        let health = evaluate_oplog(86400.0, 100.0, 1024.0, 0.0);
        assert_eq!(health.severity, OplogSeverity::Healthy);
    }
}
