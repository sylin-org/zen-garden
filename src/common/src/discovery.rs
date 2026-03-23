//! Service discovery response types
//!
//! Shared between moss (server) and rake (client) so both sides
//! use the same wire-format types without duplication.

use serde::{Deserialize, Serialize};

/// Resolved connection information for a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConnection {
    /// Hostname (e.g., "stone-02.local")
    pub hostname: String,
    /// IP address (e.g., "192.168.1.102")
    pub ip: String,
    /// Service port
    pub port: u16,
    /// Protocol (e.g., "mongodb", "postgresql", "redis")
    pub protocol: String,
    /// Connection URIs - hostname-first, then IP (for resilience)
    pub uris: Vec<String>,
}

/// Reference to a stone
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoneRef {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

/// Found service with connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundService {
    /// Unique identifier for this offering instance (GUIDv7)
    /// Survives renames, migrations, used for backup keying.
    #[serde(default)]
    pub offering_id: String,

    /// Service name
    pub name: String,

    /// Offering type (e.g., "mongodb", "redis")
    pub offering: String,

    /// Service category
    pub category: String,

    /// Service tags
    pub tags: Vec<String>,

    /// Current status
    pub status: String,

    /// Stone hosting this service
    pub stone: StoneRef,

    /// Resolved connection information
    pub connection: ResolvedConnection,

    /// Sub-capabilities (e.g., models, collections)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub sub_capabilities: Vec<crate::SubCapability>,

    /// Source identifier — who registered this entry (e.g. orchestrator name).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// Service discovery response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDiscoveryResponse {
    /// Whether services were found
    pub found: bool,

    /// Found services
    pub services: Vec<FoundService>,

    /// Data source ("cache" or "fresh")
    pub source: String,

    /// Cache age in seconds (if from cache)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_seconds: Option<u64>,

    /// Response timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
