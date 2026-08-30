// O2's chirp-composition and reconcile consume the flagged surface
// (OFFERINGS.md §5). Trim allows as wiring lands.
#![allow(dead_code)]
#![allow(clippy::large_enum_variant)] // Managed carries the full spec by design (resurrection)

//! The offering model — domain vocabulary for placed work
//! (OFFERINGS.md §1). Pure types: no I/O, no runtime knowledge. The
//! `WorkloadSpec` lives here because "what should run" is domain language;
//! adapters consume it.

use garden_glossary::offering as vocab;
use crate::garden::ports::Tier;
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
    /// The offering's address allocations riding to the world (ADR-0002):
    /// identity side of the address law. Adapters bind these homes
    /// explicitly at every create — never dynamic `""` bindings — and may
    /// relocate under protest (squatters) per [`Tier`]; the ledger truth
    /// stays fixed regardless of where reality answers today.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub allocations: HashMap<String, PortAllocation>,
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

/// An offering's ledgered address for one named port role (ADR-0002).
/// `home` is claimed for the offering's lifetime — rest and rehydration
/// keep it; only uproot releases it. `tier` records WHY that address is
/// its (manifest-declared requiredness).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortAllocation {
    pub home: u16,
    pub tier: Tier,
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
    /// The world's container name this adoption is bound to (L25:
    /// remembered binding). Status updates re-find the workload by it;
    /// without it the record could not tell its container from another's.
    #[serde(default)]
    pub container_name: String,
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
    /// Content this instance holds, by capability type, as last observed
    /// by the stone's capability sweep (offerings::capabilities). Empty
    /// until the offering's manifest declares capability types.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub sub_capabilities: HashMap<String, Vec<String>>,
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

    pub fn adopted(&self) -> Option<&AdoptedData> {
        match &self.mode_data {
            ModeData::Adopted(a) => Some(a),
            _ => None,
        }
    }

    /// Wire shape for chirps (contract::chirp::ServiceEntry), PORT-0001 map
    /// included only where remapped (R0.5). The canonical sectioned shape:
    /// identity (fqn/stem/category) + state + ports.
    pub fn service_entry(&self) -> garden_contract::chirp::ServiceEntry {
        let ports = match &self.mode_data {
            ModeData::Managed(m) => m.port_map.clone(),
            // Adopted work publishes its own port — the observed host
            // port IS its address (the connection promise reaches
            // adopted offerings too).
            _ => {
                if self.location.port > 0 {
                    HashMap::from([("default".to_string(), self.location.port)])
                } else {
                    Default::default()
                }
            }
        };
        garden_contract::chirp::ServiceEntry {
            offering_id: self.offering_id.clone(),
            name: self.name.clone(),
            stem: self.offering.clone(),
            category: self.category.clone(),
            state: garden_contract::chirp::ServiceState {
                status: self.status.as_str().into(),
                role: None,
                mode: Some(self.mode().as_str().into()),
            },
            ports,
            capabilities: self.sub_capabilities.clone(),
        }
    }
}

impl Offering {
    /// The Incarnation law (ADR-0015; ADR-0005 §6): FQN, identity, the
    /// declared will, the image, port ROLES and tiers, and volume NAMES
    /// travel with the offering. Host paths and port numbers are a
    /// stone's projection — recompiled here, on arrival. `dir` is the
    /// restored offering directory on THIS stone; `claims` is this
    /// stone's address ledger; `pool` its service pool.
    pub fn reincarnate_on(
        &mut self,
        dir: &super::directory::OfferingDir,
        claims: &[super::ports::Claim],
        pool: super::ports::Pool,
    ) -> Result<(), String> {
        let ModeData::Managed(managed) = &mut self.mode_data else {
            return Err("only managed work reincarnates".to_string());
        };
        // The stored spec speaks the DEAD stone's filesystem; the tail
        // segment splits on BOTH separators — the stored path may speak
        // a foreign OS's dialect (`C:\...` has no `/` on Linux).
        for v in &mut managed.spec.volumes {
            if let Some(name) = tail_segment(&v.host_path) {
                v.host_path = dir.volumes().join(name).to_string_lossy().into_owned();
            }
        }
        for c in &mut managed.spec.configs {
            if let Some(file) = tail_segment(&c.host_path) {
                c.host_path = dir.configs().join(file).to_string_lossy().into_owned();
            }
        }
        // Addresses are per-stone law (ADR-0002): the dead stone's
        // ledger died with it. Re-arbitrate the stored intents — a free
        // home is kept (the ledger-first promise), an occupied flexible
        // home redraws from the pool, a strict dispute refuses loudly.
        let mut intents = std::collections::BTreeMap::new();
        for (role, a) in &managed.spec.allocations {
            intents.insert(
                role.clone(),
                super::ports::Intent { tier: a.tier, home: Some(a.home) },
            );
        }
        let homes = super::ports::allocate(&intents, claims, pool)
            .map_err(|e| format!("replant address arbitration: {e}"))?;
        for (role, home) in &homes {
            if let Some(a) = managed.spec.allocations.get_mut(role) {
                a.home = *home;
            }
        }
        Ok(())
    }
}

impl Offering {
    /// REST: stopped, and convergence keeps it so. The world stops the
    /// workload; the entity records the truth of itself. Managed only.
    pub async fn rest(&mut self, rt: &dyn super::runtime::Runtime) -> Result<(), String> {
        self.require_managed("rest")?;
        rt.stop(&self.name)
            .await
            .map_err(|e| format!("rest: {e}"))?;
        self.status = Status::Stopped;
        Ok(())
    }

    /// WAKE: running again — resurrecting from the stored spec if
    /// reality lost the workload behind our back. Returns what actually
    /// happened, so the caller journals honestly.
    pub async fn wake(
        &mut self,
        rt: &dyn super::runtime::Runtime,
    ) -> Result<WakeOutcome, String> {
        let spec = {
            let managed = self.require_managed("wake")?;
            managed.spec.clone()
        };
        let outcome = match rt.observe(&self.name).await {
            None => {
                tracing::warn!(offering = %self.name, "workload missing - resurrecting from stored spec");
                rt.place(&self.name, &spec)
                    .await
                    .map_err(|e| format!("wake: {e}"))?;
                WakeOutcome::Resurrected
            }
            Some(observed) if !observed.running => {
                rt.start(&self.name)
                    .await
                    .map_err(|e| format!("wake: {e}"))?;
                WakeOutcome::Started
            }
            Some(_) => WakeOutcome::AlreadyRunning, // idempotent wake
        };
        self.status = Status::Running;
        Ok(outcome)
    }

    /// UPROOT: the workload is removed. Idempotent at the world's edge
    /// (a husk whose placement never landed is already gone). Managed
    /// only.
    pub async fn uproot(&mut self, rt: &dyn super::runtime::Runtime) -> Result<(), String> {
        self.require_managed("uproot")?;
        rt.remove(&self.name)
            .await
            .map_err(|e| format!("uproot: {e}"))
    }

    /// The verb gate: rest/wake/uproot apply to managed work only.
    fn require_managed(&self, verb: &str) -> Result<&ManagedData, String> {
        self.managed()
            .ok_or_else(|| format!("'{}' is not managed - {verb} applies to managed work", self.name))
    }
}

/// What a wake actually did — the caller journals the difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeOutcome {
    /// The workload was running; wake was a no-op on the world.
    AlreadyRunning,
    /// The workload existed but was stopped; it was started.
    Started,
    /// The workload was GONE; it was resurrected from the stored spec.
    Resurrected,
}

/// The last path segment under EITHER separator dialect. A stored
/// spec's host paths speak the stone that wrote them; a checkpoint
/// replanted across OS lines carries paths the local parser cannot
/// split.
fn tail_segment(path: &str) -> Option<&str> {
    path.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_segment_reads_both_path_dialects() {
        assert_eq!(tail_segment("/home/stone/.zen-garden/offerings/ntfy/default/volumes/ntfy-cache"), Some("ntfy-cache"));
        assert_eq!(tail_segment(r"C:\Users\onose\.zen-garden\offerings\ntfy\default\volumes\ntfy-cache"), Some("ntfy-cache"));
        assert_eq!(tail_segment("ntfy-cache"), Some("ntfy-cache"));
        assert_eq!(tail_segment(""), None);
    }
}
