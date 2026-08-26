// O2's chirp-composition and reconcile consume the flagged surface
// (OFFERINGS.md §5). Trim allows as wiring lands.
#![allow(dead_code)]
#![allow(clippy::large_enum_variant)] // Managed carries the full spec by design (resurrection)

//! The offering model — domain vocabulary for placed work
//! (OFFERINGS.md §1). Pure types: no I/O, no runtime knowledge. The
//! `WorkloadSpec` lives here because "what should run" is domain language;
//! adapters consume it.

use garden_glossary::offering as vocab;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How this offering came to be here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Managed,
    Adopted,
    Borrowed,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Managed => vocab::MANAGED,
            Self::Adopted => vocab::ADOPTED,
            Self::Borrowed => vocab::BORROWED,
        }
    }
}

/// Lifecycle position. Wire strings match the PoC byte-for-byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Installing,
    Running,
    Stopped,
    Cordoned,
    Maintenance,
    Degraded,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installing => vocab::INSTALLING,
            Self::Running => vocab::RUNNING,
            Self::Stopped => vocab::STOPPED,
            Self::Cordoned => vocab::CORDONED,
            Self::Maintenance => vocab::MAINTENANCE,
            Self::Degraded => vocab::DEGRADED,
            Self::Unknown => vocab::UNKNOWN,
        }
    }

    /// Adapter-native states arrive as wire strings; unknowns stay honest.
    pub fn parse_or_unknown(s: &str) -> Self {
        match s {
            vocab::INSTALLING => Self::Installing,
            vocab::RUNNING => Self::Running,
            vocab::STOPPED => Self::Stopped,
            vocab::CORDONED => Self::Cordoned,
            vocab::MAINTENANCE => Self::Maintenance,
            vocab::DEGRADED => Self::Degraded,
            _ => Self::Unknown,
        }
    }
}

/// What should run: the domain's description of desired execution — v1's
/// generalization of the PoC's ContainerSpec (poc docker/spec.rs:18-42).
/// Adapters translate this into their own world's mechanics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadSpec {
    /// OCI image reference.
    pub image: String,
    /// Named ports: name → container port. Host mapping is the adapter's
    /// craft; the result comes back as a PORT-0001 map.
    #[serde(default)]
    pub named_ports: HashMap<String, u16>,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Files injected into the workload (container path → content).
    #[serde(default)]
    pub config_files: HashMap<String, String>,
    /// HTTP path probed for health; None = no probe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_health_path: Option<String>,
    /// Restart policy hint: "no" | "unless-stopped" | "always".
    #[serde(default = "default_restart")]
    pub restart: String,
    /// Rare-but-real passthroughs (cap_add, shm_size, sysctls, ulimits...)
    /// consumed by adapters that understand them. Opaque to the domain.
    #[serde(default)]
    pub advanced: serde_json::Value,
    /// Remembered host-port bindings — converge/wake inject these from the
    /// record's port_map so re-placement PRESERVES ports (PORT-0001 as
    /// placement constraint). Fresh placements leave this empty.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub preferred_ports: HashMap<String, u16>,
    /// Materialized config files staged inside the offering directory and
    /// mounted into the workload. Written by the adapter pre-start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configs: Vec<ConfigMount>,
}

/// Where a volume mount sits on both sides of the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    pub host_path: String,
    pub container_path: String,
}

/// A materialized config file: content staged at `host_path` (inside the
/// offering directory), read by the workload at `container_path`. The
/// adapter writes the file and mounts it before start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigMount {
    pub host_path: String,
    pub container_path: String,
    pub content: String,
}

fn default_restart() -> String {
    "unless-stopped".into()
}

/// Where the offering answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    #[serde(default = "default_host")]
    pub host: String,
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_host() -> String {
    "localhost".into()
}

fn default_protocol() -> String {
    "http".into()
}

/// Adoption settings: how the stone found it and how much it may do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptedData {
    /// full | monitor | announce (glossary::offering::control).
    #[serde(default = "default_control")]
    pub control_level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_path: Option<String>,
}

fn default_control() -> String {
    vocab::control::MONITOR.into()
}

/// A pointer to work living elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorrowedData {
    pub connection_url: String,
    #[serde(default = "default_health_method")]
    pub health_method: String,
}

fn default_health_method() -> String {
    "http".into()
}

/// Mode-specific payload, tagged on the wire (poc parity: `mode` field).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ModeData {
    Managed(ManagedData),
    Adopted(AdoptedData),
    Borrowed(BorrowedData),
}

/// Managed-mode specifics: WHICH world runs it, WHAT it asked for, and the
/// remembered host-port truth (PORT-0001). The stored spec is complete —
/// resurrection needs no original request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedData {
    /// Binding is per-offering and permanent (OFFERINGS.md §4).
    #[serde(default)]
    pub runtime_kind: String,
    /// The full desired-execution description.
    pub spec: WorkloadSpec,
    /// Actual host ports by name, as last observed.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub port_map: HashMap<String, u16>,
    /// The compiled PlacementPlan (decisions + hash), stored so `explain`
    /// and drift-detection read the same document reality was built from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<serde_json::Value>,
}

/// The offering: one named unit of work on this stone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offering {
    pub offering_id: String,
    /// Fully-qualified name: `mongodb`, or `ollama::adopted`.
    pub name: String,
    /// Catalog template it came from ("mongodb"); adopted/borrowed use the
    /// detected/given base name.
    pub offering: String,
    pub category: String,
    pub status: Status,
    pub location: Location,
    #[serde(flatten)]
    pub mode_data: ModeData,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Offering {
    pub fn mode(&self) -> Mode {
        match self.mode_data {
            ModeData::Managed(_) => Mode::Managed,
            ModeData::Adopted(_) => Mode::Adopted,
            ModeData::Borrowed(_) => Mode::Borrowed,
        }
    }

    pub fn managed(&self) -> Option<&ManagedData> {
        match &self.mode_data {
            ModeData::Managed(m) => Some(m),
            _ => None,
        }
    }

    /// Wire shape for chirps (contract::chirp::ServiceEntry), PORT-0001 map
    /// included only where remapped (R0.5).
    pub fn service_entry(&self) -> garden_contract::chirp::ServiceEntry {
        let ports = match &self.mode_data {
            ModeData::Managed(m) => m.port_map.clone(),
            _ => Default::default(),
        };
        garden_contract::chirp::ServiceEntry {
            offering_id: self.offering_id.clone(),
            name: self.name.clone(),
            offering: self.offering.clone(),
            category: self.category.clone(),
            status: self.status.as_str().into(),
            role: None,
            ports,
        }
    }
}
