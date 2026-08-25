//! Discovery domain events.

use chrono::{DateTime, Utc};

/// Event emitted when the Discovery aggregate's state transitions.
#[derive(Debug, Clone)]
pub struct DiscoveryChanged {
    pub kind: DiscoveryChangeKind,
    pub timestamp: DateTime<Utc>,
}

/// Discriminant for `DiscoveryChanged` events.
#[derive(Debug, Clone)]
pub enum DiscoveryChangeKind {
    /// mDNS service registered or re-registered after IP/MAC change.
    Registered,
    /// mDNS TXT record updated with new health status.
    HealthUpdated,
    /// Certmesh CA service registered on mDNS.
    CertmeshRegistered,
}

impl DiscoveryChangeKind {
    /// Stable name for metrics per-kind counter lookup.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::HealthUpdated => "health_updated",
            Self::CertmeshRegistered => "certmesh_registered",
        }
    }

    /// All variant names for metrics registration.
    pub const ALL_NAMES: &'static [&'static str] =
        &["registered", "health_updated", "certmesh_registered"];
}
