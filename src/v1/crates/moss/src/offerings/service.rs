//! The application service: sequences domain commands against runtime
//! worlds (OFFERINGS.md §4). This is the ONLY place that knows both the
//! registry and the runtimes; HTTP handlers delegate here, O2's reconcile
//! loop will call these same commands.

use super::compile;
use super::facts::Factsheet;
use super::model::{Location, ManagedData, ModeData, Offering, Status, WorkloadSpec};
use super::manifest::Catalog;
use super::registry::Registry;
use super::runtime::{RuntimeRegistry, RuntimeError};
use std::collections::HashMap;
use std::sync::Arc;

/// Why an offering command refused.
#[derive(Debug)]
pub enum CommandError {
    /// The named offering isn't on this stone.
    NotFound(String),
    /// It is, but this command doesn't apply to it.
    Conflict(String),
    /// The bound world isn't available on this host.
    WorldUnavailable(String),
    /// The world tried and failed.
    Runtime(RuntimeError),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(n) => write!(f, "'{n}' is not planted here"),
            Self::Conflict(m) => write!(f, "{m}"),
            Self::WorldUnavailable(e) => write!(f, "{e}"),
            Self::Runtime(e) => write!(f, "{e}"),
        }
    }
}

/// Counts for posture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Counts {
    pub active: usize,
    pub candidates: usize,
}

/// The offering application service. Clone freely.
pub struct OfferingService {
    registry: Arc<Registry>,
    worlds: Arc<RuntimeRegistry>,
    default_world: String,
    /// The embedded catalog this stone can place from.
    pub catalog: Arc<Catalog>,
    /// The stone's facts census — compile reads a generation snapshot.
    pub facts: Arc<Factsheet>,
    /// Per-offering convergence failure counters (converge.rs drives them).
    failures: Arc<parking_lot::Mutex<HashMap<String, u32>>>,
}

impl OfferingService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<Registry>,
        worlds: Arc<RuntimeRegistry>,
        default_world: String,
        catalog: Arc<Catalog>,
        facts: Arc<Factsheet>,
    ) -> Self {
        Self {
            registry,
            worlds,
            default_world,
            catalog,
            facts,
            failures: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    fn volumes_root(&self) -> Result<std::path::PathBuf, CommandError> {
        let dir = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|h| std::path::PathBuf::from(h).join(".zen-garden").join("volumes"))
            .ok_or_else(|| CommandError::Conflict("no home directory known".into()))?;
        Ok(dir)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub(crate) fn bump_failure(&self, offering_id: &str) -> u32 {
        let mut map = self.failures.lock();
        let e = map.entry(offering_id.to_string()).or_insert(0);
        *e += 1;
        *e
    }

    pub(crate) fn clear_failure(&self, offering_id: &str) {
        self.failures.lock().remove(offering_id);
    }

    pub(crate) fn mark_degraded(&self, offering_id: &str) {
        self.registry.mark_status(offering_id, Status::Degraded);
    }

    pub fn counts(&self) -> Counts {
        Counts {
            active: self.registry.snapshot().len(),
            candidates: self.registry.candidate_count(),
        }
    }

    pub fn available_worlds(&self) -> Vec<&'static str> {
        self.worlds.kinds()
    }

    /// Plant an offering by name (OFFERINGS.md §5). Catalog manifests
    /// compile against current facts — compatibility decides, decisions are
    /// logged into the stored plan. Ad-hoc images place directly when no
    /// catalog entry exists.
    pub async fn offer(
        &self,
        name: &str,
        image: Option<String>,
        named_ports: std::collections::HashMap<String, u16>,
        category: Option<String>,
        requested_world: Option<&str>,
        inputs: &std::collections::BTreeMap<String, String>,
    ) -> Result<Offering, CommandError> {
        if self.registry.get_by_name(name).is_some() {
            return Err(CommandError::Conflict(format!("'{name}' is already planted")));
        }
        let kind = requested_world.unwrap_or(&self.default_world).to_string();
        let rt = self.worlds.by_kind(&kind).map_err(CommandError::WorldUnavailable)?;

        // Catalog path: manifest is truth; compile decides.
        if let Some(m) = self.catalog.get(name) {
            if m.managed.is_none() {
                return Err(CommandError::Conflict(format!(
                    "'{name}' declares no managed placement"
                )));
            }
            let facts_gen = self.facts.snapshot();
            let volumes_root = self.volumes_root()?;
            let plan = compile::compile(m, &facts_gen, inputs, &volumes_root).map_err(|e| match e {
                super::compile::CompileError::Denied { because, suggest } => {
                    CommandError::Conflict(format!(
                        "compatibility denied: {because}{}",
                        suggest.as_deref().map(|s| format!(" — {s}")).unwrap_or_default()
                    ))
                }
                other => CommandError::Runtime(RuntimeError::Failed(other.to_string())),
            })?;
            let placement =
                rt.place(name, &plan.workload).await.map_err(CommandError::Runtime)?;
            let now = chrono::Utc::now();
            let plan_value = serde_json::to_value(&plan)
                .map_err(|e| CommandError::Conflict(format!("plan encode: {e}")))?;
            let offering = Offering {
                offering_id: uuid::Uuid::now_v7().to_string(),
                name: m.name.clone(),
                offering: m.name.clone(),
                category: category.unwrap_or_else(|| m.category.clone()),
                status: Status::Running,
                location: Location {
                    host: "localhost".into(),
                    port: placement.named_host_ports.values().copied().next().unwrap_or(0),
                    protocol: "http".into(),
                },
                mode_data: ModeData::Managed(ManagedData {
                    runtime_kind: kind,
                    spec: plan.workload.clone(),
                    port_map: placement.named_host_ports,
                    plan: Some(plan_value),
                }),
                registered_at: now,
                updated_at: now,
            };
            self.registry.register(offering.clone());
            return Ok(offering);
        }

        // Ad-hoc path: a raw image with no catalog behind it.
        let Some(image) = image else {
            return Err(CommandError::NotFound(format!(
                "no catalog entry for '{name}' and no image given"
            )));
        };
        let spec = WorkloadSpec {
            image: image.clone(),
            named_ports: named_ports.clone(),
            ..Default::default()
        };
        let placement = rt.place(name, &spec).await.map_err(CommandError::Runtime)?;
        let now = chrono::Utc::now();
        let offering = Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: name.to_string(),
            offering: name.to_string(),
            category: category.unwrap_or_else(|| "misc".into()),
            status: Status::Running,
            location: Location {
                host: "localhost".into(),
                port: placement.named_host_ports.values().copied().next().unwrap_or(0),
                protocol: "http".into(),
            },
            mode_data: ModeData::Managed(ManagedData {
                runtime_kind: kind,
                spec,
                port_map: placement.named_host_ports,
                plan: None,
            }),
            registered_at: now,
            updated_at: now,
        };
        self.registry.register(offering.clone());
        Ok(offering)
    }

    /// The full placed record — plan attached (§5.3).
    pub fn placed(&self, name: &str) -> Option<Offering> {
        self.registry.get_by_name(name)
    }

    /// How many catalog offerings this stone could place today.
    pub fn catalog_size(&self) -> usize {
        self.catalog.len()
    }

    /// Rest: stopped, and reconcile will keep it so (§3.2).
    pub async fn rest(&self, name: &str) -> Result<Offering, CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        rt.stop(name).await.map_err(CommandError::Runtime)?;
        self.registry.mark_status(&offering.offering_id, Status::Stopped);
        self.registry.get_by_name(name).ok_or_else(|| CommandError::NotFound(name.into()))
    }

    /// Wake: running again — resurrecting the workload from its stored spec
    /// if reality lost it behind our backs (PoC wake parity).
    pub async fn wake(&self, name: &str) -> Result<Offering, CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        let managed = offering.managed().ok_or_else(|| {
            CommandError::Conflict(format!("'{name}' is not managed"))
        })?;

        match rt.observe(name).await {
            None => {
                tracing::warn!(offering = %name, "workload missing - resurrecting from stored spec");
                rt.place(name, &managed.spec).await.map_err(CommandError::Runtime)?;
            }
            Some(observed) if !observed.running => {
                rt.start(name).await.map_err(CommandError::Runtime)?;
            }
            Some(_) => {} // already running; idempotent wake
        }

        self.registry.mark_status(&offering.offering_id, Status::Running);

        // Port honesty: ephemeral reassignment without a ledger yet (O2) —
        // observe and refresh rather than lie about stale mappings.
        let mut updated = offering;
        updated.status = Status::Running; // the stale clone must not undo the mark above
        if let ModeData::Managed(m) = &mut updated.mode_data
            && let Some(observed) = rt.observe(name).await
            && observed.named_host_ports != m.port_map
            && !observed.named_host_ports.is_empty()
        {
            m.port_map = observed.named_host_ports;
            updated.location.port = m.port_map.values().copied().next().unwrap_or(0);
            updated.updated_at = chrono::Utc::now();
            self.registry.replace(updated.clone());
            tracing::info!(offering = %name, "host ports remapped after wake");
        }
        self.registry.get_by_name(name).ok_or_else(|| CommandError::NotFound(name.into()))
    }

    /// Uproot: remove the workload and forget the offering. Managed only —
    /// adopted release / borrowed return arrive with their modes (O3).
    pub async fn uproot(&self, name: &str) -> Result<(), CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        rt.remove(name).await.map_err(CommandError::Runtime)?;
        self.registry.remove(&offering.offering_id);
        Ok(())
    }

    fn managed(&self, name: &str) -> Result<Offering, CommandError> {
        let o = self.registry.get_by_name(name).ok_or_else(|| CommandError::NotFound(name.into()))?;
        if o.managed().is_none() {
            return Err(CommandError::Conflict(format!("'{name}' is not a managed offering")));
        }
        Ok(o)
    }

    pub(crate) fn world_for(&self, offering: &Offering) -> Result<Arc<dyn super::runtime::Runtime>, CommandError> {
        let kind = offering
            .managed()
            .map(|m| m.runtime_kind.as_str())
            .unwrap_or(self.default_world.as_str());
        if kind.is_empty() {
            return self.worlds.by_kind(&self.default_world).map_err(CommandError::WorldUnavailable);
        }
        self.worlds.by_kind(kind).map_err(CommandError::WorldUnavailable)
    }
}

