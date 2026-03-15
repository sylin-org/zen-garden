//! Orchestration types — coordination mode, roles, orchestration state.

use serde::{Deserialize, Serialize};

/// How instances of this offering coordinate across stones (ORCH-0006).
///
/// Controls whether the offering participates in Primary/Dormant election.
/// Stateless services (inference engines, proxies) use `Independent` (default).
/// Stateful services (databases, message brokers) opt in with `Elected`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationMode {
    /// Each instance operates independently. No election, no roles.
    /// Safe default for most offerings (inference, proxies, stateless APIs).
    #[default]
    Independent,
    /// One Primary, rest Dormant. Election determines the active writer.
    /// For stateful services (databases, registries, seed banks).
    Elected,
}

impl CoordinationMode {
    /// Returns `true` if this offering participates in Primary/Dormant election.
    pub fn is_elected(&self) -> bool {
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
/// - **Dormant**: Standby replica pulling data from the current primary.
/// - **Degraded**: Formerly primary, stepped down due to health failures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OfferingRole {
    /// Bootstrapping — not yet ready to participate.
    Joining,
    /// Active instance: serves traffic, owns writes.
    #[default]
    Primary,
    /// Standby replica: pulls from primary, ready to promote.
    Dormant,
    /// Stepped down due to consecutive health failures.
    Degraded,
}

impl std::fmt::Display for OfferingRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Joining => write!(f, "joining"),
            Self::Primary => write!(f, "primary"),
            Self::Dormant => write!(f, "dormant"),
            Self::Degraded => write!(f, "degraded"),
        }
    }
}

/// Orchestration state tracked per offering instance.
///
/// Persisted alongside the runtime `Offering`. All fields use `Option`/`Default`
/// for backward-compatible deserialization of existing JSON on disk.
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
