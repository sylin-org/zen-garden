//! Ceremony policy types for offering lifecycle management
//!
//! Defines policies that control how offerings are backed up and updated
//! during nourishment ceremonies. Templates can specify their snapshot
//! capabilities to enable safe, data-preserving updates.

use serde::{Deserialize, Serialize};

/// Ceremony mode determines snapshot strategy during nourishment
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CeremonyMode {
    /// Must stop container before snapshot (default, safest)
    #[default]
    Unsafe,
    /// Can freeze/thaw without stopping (databases with fsync)
    Quiesceable,
    /// No persistent data, commit anytime
    Stateless,
}

/// Command execution configuration for ceremony hooks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecConfig {
    /// Command and arguments to execute
    pub exec: Vec<String>,
    /// Timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
}

fn default_timeout() -> u32 {
    30
}

/// Rollback behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RollbackConfig {
    /// Auto-rollback on failure (default: true)
    #[serde(default = "default_true")]
    pub automatic: bool,
    /// Maximum rollback attempts (default: 2)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Preserve harvest on failure (default: true)
    #[serde(default = "default_true")]
    pub preserve_harvest: bool,
    /// Harvest retention duration (default: "168h" = 7 days)
    #[serde(default = "default_retention")]
    pub harvest_retention: String,
}

fn default_true() -> bool {
    true
}

fn default_max_attempts() -> u32 {
    2
}

fn default_retention() -> String {
    "168h".to_string()
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            automatic: true,
            max_attempts: 2,
            preserve_harvest: true,
            harvest_retention: "168h".to_string(),
        }
    }
}

/// Ceremony policy for an offering template
///
/// Embedded in offering manifests to declare snapshot capabilities:
/// ```yaml
/// ceremony:
///   mode: quiesceable
///   quiesce:
///     exec: ["mongosh", "--eval", "db.fsyncLock()"]
///     timeout_seconds: 30
///   resume:
///     exec: ["mongosh", "--eval", "db.fsyncUnlock()"]
///   rollback:
///     automatic: true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CeremonyPolicy {
    /// Snapshot mode (default: unsafe)
    #[serde(default)]
    pub mode: CeremonyMode,

    /// Quiesce command (required for quiesceable mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiesce: Option<ExecConfig>,

    /// Resume command (required for quiesceable mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<ExecConfig>,

    /// Post-nourish verification command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<ExecConfig>,

    /// Maximum seconds in quiesced state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_quiesce_seconds: Option<u32>,

    /// Rollback behavior
    #[serde(default)]
    pub rollback: RollbackConfig,
}

impl CeremonyPolicy {
    /// Validate policy configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == CeremonyMode::Quiesceable {
            if self.quiesce.is_none() {
                return Err("Quiesceable mode requires quiesce command".to_string());
            }
            if self.resume.is_none() {
                return Err("Quiesceable mode requires resume command".to_string());
            }
        }
        Ok(())
    }

    /// Check if this mode supports live snapshots
    pub fn supports_live_snapshot(&self) -> bool {
        matches!(self.mode, CeremonyMode::Quiesceable | CeremonyMode::Stateless)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_ceremony_mode() {
        let mode = CeremonyMode::default();
        assert_eq!(mode, CeremonyMode::Unsafe);
    }

    #[test]
    fn test_default_rollback_config() {
        let config = RollbackConfig::default();
        assert!(config.automatic);
        assert_eq!(config.max_attempts, 2);
        assert!(config.preserve_harvest);
        assert_eq!(config.harvest_retention, "168h");
    }

    #[test]
    fn test_unsafe_policy_validates() {
        let policy = CeremonyPolicy::default();
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn test_quiesceable_requires_commands() {
        let policy = CeremonyPolicy {
            mode: CeremonyMode::Quiesceable,
            ..Default::default()
        };
        assert!(policy.validate().is_err());

        let policy_with_quiesce = CeremonyPolicy {
            mode: CeremonyMode::Quiesceable,
            quiesce: Some(ExecConfig {
                exec: vec!["fsync".to_string()],
                timeout_seconds: 30,
            }),
            resume: Some(ExecConfig {
                exec: vec!["resume".to_string()],
                timeout_seconds: 30,
            }),
            ..Default::default()
        };
        assert!(policy_with_quiesce.validate().is_ok());
    }

    #[test]
    fn test_ceremony_policy_serialization() {
        let policy = CeremonyPolicy {
            mode: CeremonyMode::Quiesceable,
            quiesce: Some(ExecConfig {
                exec: vec!["mongosh".to_string(), "--eval".to_string(), "db.fsyncLock()".to_string()],
                timeout_seconds: 30,
            }),
            resume: Some(ExecConfig {
                exec: vec!["mongosh".to_string(), "--eval".to_string(), "db.fsyncUnlock()".to_string()],
                timeout_seconds: 30,
            }),
            verify: None,
            max_quiesce_seconds: Some(60),
            rollback: RollbackConfig::default(),
        };

        let json = serde_json::to_string(&policy).unwrap();
        assert!(json.contains("quiesceable"));
        assert!(json.contains("fsyncLock"));

        // Verify optional fields are omitted (skip_serializing_if)
        assert!(!json.contains("verify"));
    }

    #[test]
    fn test_stateless_supports_live_snapshot() {
        let policy = CeremonyPolicy {
            mode: CeremonyMode::Stateless,
            ..Default::default()
        };
        assert!(policy.supports_live_snapshot());
    }
}
