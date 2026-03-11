//! Canonical stone types — the authoritative domain model for a garden node.
//!
//! A **stone** is a single node in the garden network. Whether it is the local
//! daemon (`AppState.current.stone`) or a remote peer in the topology cache, a
//! stone is always described by the same type. There are no DTO copies.
//!
//! ## Type hierarchy
//!
//! ```text
//! Current
//! ├── stone: Arc<RwLock<Stone>>   — mutable node identity + full description
//! └── environment: Environment    — static after startup
//!
//! Stone
//! ├── id:           String        — permanent (cryptographic/install identity)
//! ├── name:         String        — user-assigned display name (changeable)
//! ├── address:      PeerAddress   — network endpoint (changes on DHCP renewal)
//! ├── version:      String        — moss version running on this stone
//! ├── mac:          Option<…>     — for Wake-on-LAN
//! ├── health:       String        — health status constant
//! ├── status:       StoneStatus   — Online | Offline
//! ├── services:     Vec<…>        — services running on this stone
//! ├── capabilities: Option<…>     — hardware (None during early boot)
//! ├── discovered_at / last_seen   — lifecycle timestamps
//! ├── tags:         Vec<String>   — notification tags
//! └── gateways:     Vec<…>        — registered orchestrator gateways
//!
//! Environment
//! └── os: OsKind    — Linux | Windows, static after startup
//! ```
//!
//! `TopologyEntry` is a legacy name for `Stone`. Callers migrate wave-by-wave
//! (ARCH-0003); `TopologyEntry` will be removed after all consumers are updated.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::peer_address::PeerAddress;
use crate::types::{GatewayRegistration, HardwareCapabilities, StoneStatus, TopologyServiceEntry};

// ============================================================================
// Stone
// ============================================================================

/// A stone in the garden — the authoritative, complete description of a node.
///
/// Used for:
/// - **Local node**: `AppState.current.stone` — wrapped in `Arc<RwLock<Stone>>`
///   so name and address updates are atomic.
/// - **Remote peers**: the topology discovery cache — same type, no DTO copy.
/// - **P2P chirp payload**: stones broadcast a stripped `Stone` over UDP;
///   use [`Stone::stripped_for_chirp`] to produce the lightweight wire form.
///
/// ## Identity model
///
/// | Field     | Mutability | Description |
/// |-----------|-----------|-------------|
/// | `id`      | Permanent  | Cryptographic install identity (GUIDv7); never changes |
/// | `name`    | Mutable    | User-assigned display name; changes on rename |
/// | `address` | Mutable    | Network endpoint; changes on DHCP renewal or reconnect |
///
/// `id` is permanent. `name` and `host` (via `address`) are mutable at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stone {
    /// Permanent cryptographic identity — set once at install, never changes.
    pub id: String,

    /// User-assigned display name — changeable at runtime (user rename).
    pub name: String,

    /// Full network endpoint: IP, HTTP port, optional TLS port.
    ///
    /// Changes when DHCP renews the IP or when the stone reconnects on a new
    /// address. Replaces the old `stone_host: String` field, which was a
    /// stringly-typed fragment of the same concept.
    pub address: PeerAddress,

    /// Moss version string running on this stone (e.g., "0.2.0").
    pub version: String,

    /// MAC address for Wake-on-LAN support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,

    /// Health status — use `garden_common::constants` health constants
    /// (`HEALTH_HEALTHY`, `HEALTH_DEGRADED`, etc.).
    pub health: String,

    /// Connectivity status.
    pub status: StoneStatus,

    /// Services currently running on this stone (lightweight topology view).
    #[serde(default)]
    pub services: Vec<TopologyServiceEntry>,

    /// Hardware capabilities — `None` during early boot, populated after detection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<HardwareCapabilities>,

    /// When this stone was first discovered in this session.
    pub discovered_at: DateTime<Utc>,

    /// When this stone was last seen on the network.
    pub last_seen: DateTime<Utc>,

    /// Notification tags compiled from [`garden_common::notifications::NotificationRegistry`].
    /// Indicates stone has something noteworthy (opportunity, attention).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Gateway registrations — orchestrators fronting offerings on this stone.
    /// Empty for most stones. See ORCH-0004.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gateways: Vec<GatewayRegistration>,
}

impl Stone {
    /// Produce a lightweight clone suitable for UDP chirp broadcast.
    ///
    /// Strips fields that are heavy, never consumed from peers, or redundant
    /// in the wire context. See COMM-0005 for the field-by-field audit.
    pub fn stripped_for_chirp(&self) -> Self {
        let mut entry = self.clone();
        if let Some(ref mut caps) = entry.capabilities {
            // stone_id / stone_name on HardwareCapabilities are legacy fields
            // that duplicate identity already present on the parent Stone.
            // Zero them out; they will be removed from HardwareCapabilities
            // once all callers migrate (ARCH-0003 Wave 6).
            caps.stone_id = None;
            caps.stone_name = String::new();
            // Strip detection_status (never read from peers)
            caps.detection_status = crate::types::DetectionStatus::Complete;
            // Strip heavy / unused hardware sub-fields
            caps.hardware.cpu.features = None;
            caps.hardware.cpu.threads = None;
            caps.hardware.swap_mb = None;
            // Strip unused runtime sub-fields
            if let Some(ref mut rt) = caps.runtime {
                rt.docker_version = None;
            }
        }
        entry
    }
}

// ============================================================================
// Current — the local node's mutable self-description
// ============================================================================

/// The running node's mutable self-description.
///
/// `current` is not a domain context — it holds no operational state.
/// It is what *this* node *is* right now: its identity and runtime environment.
///
/// Access patterns:
/// ```text
/// state.current.stone.read().await.id         // permanent identity
/// state.current.stone.read().await.name       // display name — may change
/// state.current.stone.read().await.address    // network address — may change
/// state.current.environment.os               // Linux | Windows — static
/// ```
#[derive(Clone)]
pub struct Current {
    /// This node's stone description — wrapped for atomic mutable updates.
    ///
    /// `name` changes on user rename. `address` changes on DHCP renewal or
    /// reconnect. Wrap in `Arc<RwLock<>>` so updates are atomic.
    pub stone: Arc<RwLock<Stone>>,

    /// Static runtime environment — does not change after startup.
    pub environment: Environment,
}

// ============================================================================
// Environment
// ============================================================================

/// Static runtime environment for this node — set once at startup.
#[derive(Debug, Clone)]
pub struct Environment {
    /// Operating system this stone is running on.
    pub os: OsKind,
}

/// Operating system kind — determines platform-specific behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsKind {
    Linux,
    Windows,
}

impl std::fmt::Display for OsKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsKind::Linux => write!(f, "linux"),
            OsKind::Windows => write!(f, "windows"),
        }
    }
}
