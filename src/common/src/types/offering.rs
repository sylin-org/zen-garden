//! Offering types — deployment modes, offering instances, guidance.

use crate::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::hardware::ContainerResources;
use super::health::ServiceHealthStatus;
use super::orchestration::OrchestrationState;
use super::service::{ServiceStatus, SubCapability};

// ── Offering modes ──────────────────────────────────────────────────

/// Deployment mode for an offering
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfferingMode {
    /// Container-based offering managed by Moss (default, current system)
    Managed,
    /// Existing service (native or containerized) adopted by Moss
    Adopted,
    /// External network service announced by Moss
    Borrowed,
}

impl std::fmt::Display for OfferingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Managed => write!(f, "managed"),
            Self::Adopted => write!(f, "adopted"),
            Self::Borrowed => write!(f, "borrowed"),
        }
    }
}

/// Control level for adopted offerings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AdoptedControlLevel {
    /// Moss manages lifecycle (start/stop/restart)
    Full,
    /// Moss monitors health only (default - safe)
    #[default]
    Monitor,
    /// Moss announces existence only (discovery)
    Announce,
}

/// Health check method for borrowed offerings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthMethod {
    /// HTTP endpoint probe
    Http,
    /// TCP socket connectivity
    Tcp,
    /// No health check (always assume healthy)
    None,
}

// ── Unified Offering ────────────────────────────────────────────────

/// Offering instance representing any running/adopted/borrowed service
///
/// A unified structure that uses an enum for mode-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offering {
    // ═══════════════════════════════════════════════════════════════
    // IDENTITY (common to all modes)
    // ═══════════════════════════════════════════════════════════════
    /// Unique identifier (GUIDv7) - generated for all modes
    pub offering_id: String,

    /// Fully-qualified offering name (e.g., `mongodb`, `ollama::adopted`).
    /// Serializes as a plain string in JSON; auto-normalizes legacy formats on load.
    pub name: OfferingFqn,

    /// Offering type/template name (e.g., "mongodb", "ollama")
    pub offering: String,

    /// Category from manifest (e.g., "ai", "data", "cache").
    /// Set at creation time. Empty string for legacy persisted offerings
    /// (backfilled from manifest on load).
    #[serde(default)]
    pub category: String,

    /// Version string (always present, "unknown" if undetected)
    #[serde(default = "default_version_unknown")]
    pub version: String,

    // ═══════════════════════════════════════════════════════════════
    // STATE (common to all modes)
    // ═══════════════════════════════════════════════════════════════
    /// Current operational status
    pub status: OfferingStatus,

    /// Health status (unified across modes)
    pub health: ServiceHealthStatus,

    /// Runtime-discovered capabilities (models, extensions, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_capabilities: Vec<SubCapability>,

    // ═══════════════════════════════════════════════════════════════
    // LOCATION (unified port handling)
    // ═══════════════════════════════════════════════════════════════
    /// Service network location
    pub location: OfferingLocation,

    // ═══════════════════════════════════════════════════════════════
    // MODE-SPECIFIC DATA (enum with associated data)
    // ═══════════════════════════════════════════════════════════════
    /// Mode-specific configuration and state
    pub mode_data: OfferingModeData,

    // ═══════════════════════════════════════════════════════════════
    // TIMESTAMPS
    // ═══════════════════════════════════════════════════════════════
    /// When this offering was first registered/detected/announced
    pub registered_at: chrono::DateTime<chrono::Utc>,

    /// When this offering was last updated (status change, health change, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,

    // ═══════════════════════════════════════════════════════════════
    // ORCHESTRATION (ORCH-0001)
    // ═══════════════════════════════════════════════════════════════
    /// Orchestration state for multi-instance coordination.
    /// `None` when orchestration is not active for this offering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestration: Option<OrchestrationState>,
}

fn default_version_unknown() -> String {
    "unknown".to_string()
}

/// Unified offering status (expanded from ServiceStatus)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfferingStatus {
    /// Being installed/pulled (managed only)
    Installing,
    /// Running and operational
    Running,
    /// Stopped or not running
    Stopped,
    /// In maintenance mode
    Maintenance,
    /// Running but degraded
    Degraded,
    /// Status cannot be determined
    Unknown,
}

impl std::fmt::Display for OfferingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Installing => write!(f, "installing"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Maintenance => write!(f, "maintenance"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl From<ServiceStatus> for OfferingStatus {
    fn from(status: ServiceStatus) -> Self {
        match status {
            ServiceStatus::Installing => Self::Installing,
            ServiceStatus::Running => Self::Running,
            ServiceStatus::Stopped => Self::Stopped,
            ServiceStatus::Maintenance => Self::Maintenance,
            ServiceStatus::Degraded => Self::Degraded,
            ServiceStatus::Unknown => Self::Unknown,
        }
    }
}

impl From<OfferingStatus> for ServiceStatus {
    fn from(status: OfferingStatus) -> Self {
        match status {
            OfferingStatus::Installing => Self::Installing,
            OfferingStatus::Running => Self::Running,
            OfferingStatus::Stopped => Self::Stopped,
            OfferingStatus::Maintenance => Self::Maintenance,
            OfferingStatus::Degraded => Self::Degraded,
            OfferingStatus::Unknown => Self::Unknown,
        }
    }
}

/// Unified network location (replaces Ports + ServiceLocation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferingLocation {
    /// Host address (localhost for managed, configurable for adopted/borrowed)
    #[serde(default = "default_localhost")]
    pub host: String,

    /// Primary service port
    pub port: u16,

    /// Protocol hint (http, tcp, mongodb, postgres, etc.)
    #[serde(default = "default_protocol")]
    pub protocol: String,

    /// Optional agnostic port (managed containers only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agnostic_port: Option<u16>,

    /// Named port map: port_name → actual_host_port (PORT-0001).
    /// Only populated when at least one port was remapped from manifest defaults.
    /// Empty map = all ports match manifest defaults.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub port_map: std::collections::HashMap<String, u16>,
}

fn default_localhost() -> String {
    "localhost".to_string()
}

fn default_protocol() -> String {
    "http".to_string()
}

impl OfferingLocation {}

/// Mode-specific data as enum variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum OfferingModeData {
    /// Container managed by Moss
    Managed(ManagedData),

    /// Native service adopted by Moss
    Adopted(AdoptedData),

    /// External service announced by Moss
    Borrowed(BorrowedData),
}

impl OfferingModeData {
    /// Get the offering mode
    pub fn mode(&self) -> OfferingMode {
        match self {
            Self::Managed(_) => OfferingMode::Managed,
            Self::Adopted(_) => OfferingMode::Adopted,
            Self::Borrowed(_) => OfferingMode::Borrowed,
        }
    }
}

/// A configuration overlay applied to a managed service by an external actor.
///
/// Config patches allow orchestrators, admins, or other tools to modify
/// a container's runtime configuration (command, env vars, mounts) without
/// touching the manifest. Patches are persisted and survive restarts and
/// nourish operations.
///
/// Each patch has an **owner** — the actor who applied it. Ownership enables
/// conflict detection (two owners cannot set the same singular field),
/// targeted removal, and visibility ("customized by X").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPatch {
    /// Who owns this patch (e.g., "mongodb-orchestrator", "admin").
    pub owner: String,

    /// Human-readable description of why this patch exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// When this patch was applied or last updated.
    pub applied_at: chrono::DateTime<chrono::Utc>,

    /// Container command override (replaces image default CMD).
    /// Singular field — only one owner may set this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,

    /// Additional environment variables (KEY → VALUE).
    /// Additive — multiple owners may contribute, but same key = conflict.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,

    /// Additional volume mounts (host_path, container_path).
    /// Additive — duplicate container_path = conflict.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<(String, String)>,

    /// Config file content keyed by container path.
    /// e.g., "/etc/mongod.conf" → "replication:\n  replSetName: zen-garden\n"
    /// Written to the host config directory and applied via restart or signal.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, String>,
}

/// Data specific to managed (container) offerings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManagedData {
    /// Container resources (CPU, memory usage)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ContainerResources>,

    /// Job ID for tracking installation progress
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,

    /// Cached post-installation guidance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<OfferingGuidance>,

    /// Configuration patches applied by external actors (orchestrators, admins).
    /// Composed with the manifest template at every container lifecycle event
    /// to produce the effective container configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_patches: Vec<ConfigPatch>,
}

/// Data specific to adopted (native) offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdoptedData {
    /// Control level (Full/Monitor/Announce)
    #[serde(default)]
    pub control_level: AdoptedControlLevel,

    /// Start command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_command: Option<String>,

    /// Stop command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,

    /// Restart command (if control_level allows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_command: Option<String>,

    /// Health check URL for HTTP health probes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_check_url: Option<String>,

    /// Cached post-adoption guidance (if available)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub guidance: Option<OfferingGuidance>,

    /// Container name if adopted from a container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,

    /// When the service was detected
    pub detected_at: chrono::DateTime<chrono::Utc>,
}

/// Data specific to borrowed (external) offerings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorrowedData {
    /// Health check method (Http/Tcp/None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_method: Option<HealthMethod>,

    /// Key to retrieve credentials from secrets store
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials_key: Option<String>,

    /// Connection string template
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_template: Option<String>,

    /// When this service was announced
    pub announced_at: chrono::DateTime<chrono::Utc>,
}

// ── Offering helper methods ─────────────────────────────────────────

impl Offering {
    /// Get the offering mode
    pub fn mode(&self) -> OfferingMode {
        self.mode_data.mode()
    }

    /// Check if this is a managed container
    pub fn is_managed(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Managed(_))
    }

    /// Check if this is an adopted service
    pub fn is_adopted(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Adopted(_))
    }

    /// Check if this is a borrowed service
    pub fn is_borrowed(&self) -> bool {
        matches!(self.mode_data, OfferingModeData::Borrowed(_))
    }

    /// Get managed-specific data (if managed)
    pub fn managed_data(&self) -> Option<&ManagedData> {
        match &self.mode_data {
            OfferingModeData::Managed(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable managed-specific data (if managed)
    pub fn managed_data_mut(&mut self) -> Option<&mut ManagedData> {
        match &mut self.mode_data {
            OfferingModeData::Managed(data) => Some(data),
            _ => None,
        }
    }

    /// Get adopted-specific data (if adopted)
    pub fn adopted_data(&self) -> Option<&AdoptedData> {
        match &self.mode_data {
            OfferingModeData::Adopted(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable adopted-specific data (if adopted)
    pub fn adopted_data_mut(&mut self) -> Option<&mut AdoptedData> {
        match &mut self.mode_data {
            OfferingModeData::Adopted(data) => Some(data),
            _ => None,
        }
    }

    /// Get borrowed-specific data (if borrowed)
    pub fn borrowed_data(&self) -> Option<&BorrowedData> {
        match &self.mode_data {
            OfferingModeData::Borrowed(data) => Some(data),
            _ => None,
        }
    }

    /// Get mutable borrowed-specific data (if borrowed)
    pub fn borrowed_data_mut(&mut self) -> Option<&mut BorrowedData> {
        match &mut self.mode_data {
            OfferingModeData::Borrowed(data) => Some(data),
            _ => None,
        }
    }

    /// Update the timestamp
    pub fn touch(&mut self) {
        self.updated_at = Some(chrono::Utc::now());
    }
}

// ── Offering Guidance Types ─────────────────────────────────────────

/// Offering guidance - post-installation notes displayed on the stone portrait
///
/// Guidance is markdown content with YAML frontmatter that provides
/// helpful information to users after an offering is installed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OfferingGuidance {
    /// Raw markdown content (without frontmatter)
    pub content: String,

    /// Variables that have been substituted (for reference)
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub variables: std::collections::HashMap<String, String>,
}

/// Guidance frontmatter metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuidanceFrontmatter {
    /// Schema version
    #[serde(default = "default_version")]
    pub version: String,

    /// When to show the guidance (default: post_install)
    #[serde(default)]
    pub trigger: GuidanceTrigger,
}

fn default_version() -> String {
    "1".to_string()
}

impl Default for GuidanceFrontmatter {
    fn default() -> Self {
        Self {
            version: default_version(),
            trigger: GuidanceTrigger::default(),
        }
    }
}

/// When guidance should be displayed
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceTrigger {
    /// Show after installation (default)
    #[default]
    PostInstall,
    /// Always show while offering is running
    Always,
}
