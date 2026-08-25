//! Discovery protocol types — UDP announcements, topology entries, gateway registration.

use crate::constants::*;
use crate::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};

use super::offering::{Offering, OfferingStatus};
use super::service::{ServiceInfo, ServiceStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    pub discover: String,
    pub request_id: String,
    pub requester: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    /// Unique stone identifier (GUID v7)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stone_id: Option<String>,
    pub stone_name: String,
    /// Network address of the responding stone.
    pub address: crate::PeerAddress,
    pub moss_version: String,
    pub lantern_endpoint: Option<String>,
}

// ── UDP Announcement Envelope (unified message format) ──────────────

/// UDP Announcement envelope for type-safe message routing
///
/// All UDP broadcasts use this envelope format. Consumers filter by `announcement_type`
/// and deserialize `data` into the appropriate typed payload.
///
/// # Example
/// ```ignore
/// let announcement = UdpAnnouncement {
///     msg_id: Some(generate_guidv7()),
///     announcement_type: announcement_types::STONE_CHIRP.to_string(),
///     data: serde_json::to_value(&chirp_payload)?,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpAnnouncement {
    /// Optional message ID for deduplication (GUIDv7)
    /// When present, receivers will deduplicate messages with same ID within 5s window.
    /// This handles multi-path delivery (multicast + broadcast arriving separately).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    /// Announcement type discriminator
    #[serde(rename = "type")]
    pub announcement_type: String,
    /// Typed payload (deserialize based on announcement_type)
    pub data: serde_json::Value,
    /// Base64-encoded ECDSA signature over the serialized `data` field.
    /// Present when the sender is enrolled in a pond (Phase 2 signed chirps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// PEM-encoded sender public key (SPKI format, `BEGIN PUBLIC KEY`).
    /// Phase 2: bare public key for direct signature verification.
    /// Phase 4: will add full cert for CA chain validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_cert: Option<String>,
}

/// Service information for topology entries and chirp payloads
///
/// Lightweight representation of service state for UDP topology broadcasts.
/// Full ServiceInfo (with health, ports, resources) is used in local registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyServiceEntry {
    /// Unique identifier for this offering instance (GUIDv7)
    /// Survives renames, migrations, used for backup keying.
    #[serde(default)]
    pub offering_id: String,
    /// Fully-qualified offering name (auto-normalizes legacy formats on deserialize).
    pub name: OfferingFqn,
    pub offering: String,
    pub category: String,
    pub status: String,
    /// Orchestration role: "primary", "replica", "joining", "degraded".
    /// `None` when orchestration is not active for this offering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Actual host ports, keyed by port name (e.g., "default", "management").
    /// Only includes ports that differ from manifest defaults (PORT-0001).
    /// Empty/absent = all ports match manifest defaults.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub ports: std::collections::HashMap<String, u16>,
}

impl TopologyServiceEntry {
    /// Convert full ServiceInfo to lightweight TopologyServiceEntry
    /// Used when syncing registry to self_entry for chirp broadcasts
    pub fn from_service_info(service: &ServiceInfo, category: Option<&str>) -> Self {
        let name = OfferingFqn::parse(&service.name).unwrap_or_else(|_| {
            OfferingFqn::new(&service.offering).unwrap_or_else(|_| OfferingFqn {
                source: None,
                offering: service.offering.clone(),
                instance: None,
                image_ref: None,
            })
        });
        Self {
            offering_id: service.offering_id.clone(),
            name,
            offering: service.offering.clone(),
            category: category.unwrap_or(&service.offering).to_string(),
            status: match service.status {
                ServiceStatus::Running => SERVICE_RUNNING,
                ServiceStatus::Stopped => SERVICE_STOPPED,
                ServiceStatus::Cordoned => SERVICE_CORDONED,
                ServiceStatus::Installing => SERVICE_INSTALLING,
                ServiceStatus::Maintenance => SERVICE_MAINTENANCE,
                ServiceStatus::Degraded => SERVICE_DEGRADED,
                ServiceStatus::Unknown => SERVICE_UNKNOWN,
            }
            .to_string(),
            role: None, // ServiceInfo doesn't carry orchestration state
            ports: std::collections::HashMap::new(), // ServiceInfo doesn't carry port_map
        }
    }

    /// Batch convert ServiceInfo vec to TopologyServiceEntry vec
    pub fn from_service_infos(services: &[ServiceInfo]) -> Vec<Self> {
        services
            .iter()
            .map(|svc| Self::from_service_info(svc, None))
            .collect()
    }

    /// Create from Offering
    ///
    /// Note: `Offering` does not carry its manifest category, so `category`
    /// Convert a runtime Offering to a lightweight topology entry.
    pub fn from_offering(offering: &Offering) -> Self {
        Self {
            offering_id: offering.offering_id.clone(),
            name: offering.name.clone(),
            offering: offering.offering.clone(),
            category: if offering.category.is_empty() {
                offering.offering.clone() // Fallback for legacy offerings
            } else {
                offering.category.clone()
            },
            status: match offering.status {
                OfferingStatus::Running => SERVICE_RUNNING,
                OfferingStatus::Stopped => SERVICE_STOPPED,
                OfferingStatus::Cordoned => SERVICE_CORDONED,
                OfferingStatus::Installing => SERVICE_INSTALLING,
                OfferingStatus::Maintenance => SERVICE_MAINTENANCE,
                OfferingStatus::Degraded => SERVICE_DEGRADED,
                OfferingStatus::Unknown => SERVICE_UNKNOWN,
            }
            .to_string(),
            role: offering.orchestration.as_ref().map(|o| o.role.to_string()),
            // Always include the actual port. If port_map was populated
            // (new adoptions), use it. Otherwise synthesize from location.port
            // so topology consumers always know the actual service port.
            ports: if offering.location.port_map.is_empty() && offering.location.port > 0 {
                let mut pm = std::collections::HashMap::new();
                pm.insert("default".to_string(), offering.location.port);
                pm
            } else {
                offering.location.port_map.clone()
            },
        }
    }

    /// Batch convert Offering vec to TopologyServiceEntry vec
    pub fn from_offerings(offerings: &[Offering]) -> Vec<Self> {
        offerings.iter().map(Self::from_offering).collect()
    }
}

/// Stone connectivity status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StoneStatus {
    /// Stone is actively announcing (seen within threshold)
    Online,
    /// Stone has stopped announcing but is remembered for WoL
    Offline,
}

impl std::fmt::Display for StoneStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoneStatus::Online => write!(f, "online"),
            StoneStatus::Offline => write!(f, "offline"),
        }
    }
}

/// A registered gateway — an orchestrator that fronts an offering.
///
/// Stored in-memory by Moss, included in chirp payloads, and used by
/// service discovery to resolve connection endpoints.
/// See: ORCH-0004 for the full gateway announcement design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRegistration {
    /// Offering FQN, e.g. "ollama:orchestrator"
    pub fqn: String,

    /// The offering(s) this gateway handles, e.g. ["ollama"]
    pub handler_for: Vec<String>,

    /// Self-reported hostname (registered via Koi mDNS)
    pub hostname: String,

    /// Self-reported IP address
    pub ip: String,

    /// Proxy port (e.g. 21434)
    pub port: u16,

    /// Protocol for URI construction
    pub protocol: String,

    /// URI template for connection resolution, e.g. "http://{host}:{port}"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_template: Option<String>,

    /// Category for service discovery (e.g. "orchestrator", "data", "ai").
    /// If absent, defaults to "orchestrator" in service discovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Tags for service discovery filtering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Identifier of the registering process
    pub source: String,

    /// When this registration was created/last refreshed
    pub registered_at: chrono::DateTime<chrono::Utc>,
}

/// Stone goodbye payload - sent when stone is shutting down gracefully
///
/// Enables immediate offline marking instead of waiting for chirp timeout.
/// Minimal payload - just identification fields needed to find the stone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneGoodbyePayload {
    pub stone_id: String,
    pub stone_name: String,
}
