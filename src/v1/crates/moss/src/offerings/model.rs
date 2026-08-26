// O0 lands the full model; chirp-composition and reconcile consume the
// flagged surface in O1/O2 (OFFERINGS.md §5). Trim allows as wiring lands.
#![allow(dead_code)]

//! The offering model — an agnostic representation of placed work
//! (OFFERINGS.md §1). Modes carry mode-specific data; the registry knows
//! modes, runtimes know containers (§4).

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

    pub fn parse(s: &str) -> Self {
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
    /// What to run if control_level allows; absent = watch-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,
    /// HTTP path probed for liveness (e.g. "/").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_path: Option<String>,
}

fn default_control() -> String {
    vocab::control::MONITOR.into()
}

/// A pointer to work living elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorrowedData {
    /// Connection URL as given (`http://host:port`).
    pub connection_url: String,
    /// Health probe method: http | tcp | none.
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

/// Managed-mode specifics. O1 fills deployment details via the runtime;
/// the registry stores only what outlives processes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedData {
    /// Actual host ports by name — remembered across redeploys (PORT-0001).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub port_map: HashMap<String, u16>,
    /// Container-side ports by name — lets wake re-derive the host mapping
    /// when the runtime reassigns ephemeral ports.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub container_ports: HashMap<String, u16>,
    /// The image this offering was placed from — enough for wake to
    /// resurrect a vanished workload without the original request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Volumes root for this offering's data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_root: Option<String>,
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
