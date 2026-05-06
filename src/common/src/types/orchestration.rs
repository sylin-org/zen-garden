//! Orchestration types — coordination mode, roles, orchestration state.

use serde::{Deserialize, Serialize};

/// How instances of this offering coordinate across stones (ORCH-0006).
///
/// Controls whether the offering participates in Primary/Replica election.
/// Stateless services (inference engines, proxies) use `Independent` (default).
/// Stateful services (databases, message brokers) opt in with `Elected`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationMode {
    /// Each instance operates independently. No election, no roles.
    /// Safe default for most offerings (inference, proxies, stateless APIs).
    #[default]
    Independent,
    /// One Primary, rest Replica. Election determines the active writer.
    /// For stateful services (databases, registries, seed banks).
    Elected,
}

impl CoordinationMode {
    /// Returns `true` if this offering participates in Primary/Replica election.
    pub fn is_elected(&self) -> bool {
        matches!(self, Self::Elected)
    }

    /// Whether only the Primary instance should be announced via mDNS/DNS.
    ///
    /// For `Independent`, all instances are announced (each is autonomous).
    /// For `Elected`, only the Primary is announced — Replica/Degraded are
    /// reachable but not discoverable by clients.
    pub fn announce_primary_only(&self) -> bool {
        matches!(self, Self::Elected)
    }
}

impl std::fmt::Display for CoordinationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Independent => write!(f, "independent"),
            Self::Elected => write!(f, "elected"),
        }
    }
}

/// Orchestration role for multi-instance coordination.
///
/// Drives the four-state lifecycle:
/// - **Joining**: Stone is bootstrapping this offering, not yet ready.
/// - **Primary**: Active instance serving traffic and owning writes.
/// - **Replica**: Active replica pulling data from the current primary
///   and (configurably) serving read traffic. Promotable.
/// - **Degraded**: Formerly primary, stepped down due to health failures.
///
/// `Deserialize` is implemented manually to be tolerant of unknown strings
/// — any value outside the canonical four falls back to [`Self::Joining`]
/// with a warning. This keeps every wire/persistence consumer of this type
/// resilient to enum evolution without per-field annotations.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OfferingRole {
    /// Bootstrapping — not yet ready to participate.
    Joining,
    /// Active instance: serves traffic, owns writes.
    #[default]
    Primary,
    /// Active replica: pulls from primary, ready to promote.
    Replica,
    /// Stepped down due to consecutive health failures.
    Degraded,
}

impl<'de> Deserialize<'de> for OfferingRole {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "joining" => Self::Joining,
            "primary" => Self::Primary,
            "replica" => Self::Replica,
            "degraded" => Self::Degraded,
            other => {
                tracing::warn!(
                    role = %other,
                    "Unrecognised OfferingRole — falling back to Joining for re-election"
                );
                Self::Joining
            }
        })
    }
}

impl OfferingRole {
    /// Whether this role should be announced via mDNS/DNS.
    ///
    /// Only Primary instances are discoverable by clients. Replica and
    /// Degraded instances are reachable (for replication, health checks)
    /// but not announced as service endpoints.
    pub fn is_announced(&self) -> bool {
        matches!(self, Self::Primary)
    }
}

impl std::fmt::Display for OfferingRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Joining => write!(f, "joining"),
            Self::Primary => write!(f, "primary"),
            Self::Replica => write!(f, "replica"),
            Self::Degraded => write!(f, "degraded"),
        }
    }
}

/// Orchestration state tracked per offering instance.
///
/// Persisted alongside the runtime `Offering`. All fields use `Option`/`Default`
/// for backward-compatible deserialization of existing JSON on disk.
///
/// `role` tolerance: `OfferingRole` has a custom `Deserialize` impl that maps
/// any unrecognised string to [`OfferingRole::Joining`] with a warning. This
/// isolates wire/enum-evolution tolerance to the type itself — every consumer
/// (persistence, API, any future wire shape) inherits the same behaviour
/// without needing per-field annotations.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct OrchestrationState {
    /// Current role in the orchestration lifecycle.
    #[serde(default)]
    pub role: OfferingRole,

    /// Stone ID of the current primary (if known).
    /// `None` during first deploy (self becomes primary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_stone_id: Option<String>,

    /// Whether this instance has been administratively pinned as primary.
    #[serde(default)]
    pub pinned: bool,

    /// ISO 8601 timestamp of when the pin was set.
    /// Used as a tiebreaker when multiple candidates are pinned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_timestamp: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offering_role_deserializes_canonical_strings() {
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""primary""#).unwrap(),
            OfferingRole::Primary,
        );
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""replica""#).unwrap(),
            OfferingRole::Replica,
        );
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""joining""#).unwrap(),
            OfferingRole::Joining,
        );
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""degraded""#).unwrap(),
            OfferingRole::Degraded,
        );
    }

    #[test]
    fn offering_role_falls_back_on_unknown_string() {
        // "dormant" is the legacy string from before ARCH-0038's rename.
        // Falls back to Joining with a warning so the orchestration loop
        // re-elects on the next tick instead of failing the whole load.
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""dormant""#).unwrap(),
            OfferingRole::Joining,
        );
        // Arbitrary unknown values get the same fallback.
        assert_eq!(
            serde_json::from_str::<OfferingRole>(r#""garbage""#).unwrap(),
            OfferingRole::Joining,
        );
    }

    #[test]
    fn orchestration_state_loads_with_unknown_role() {
        // The whole struct must deserialize even when role is unknown —
        // this is the persistence-tolerance contract.
        let json = r#"{"role":"dormant","pinned":true}"#;
        let state: OrchestrationState = serde_json::from_str(json).unwrap();
        assert_eq!(state.role, OfferingRole::Joining);
        assert!(state.pinned);
    }
}
