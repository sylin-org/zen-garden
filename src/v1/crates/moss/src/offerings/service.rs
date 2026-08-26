//! The application service: sequences domain commands against runtime
//! worlds (OFFERINGS.md §4). This is the ONLY place that knows both the
//! registry and the runtimes; HTTP handlers delegate here, O2's reconcile
//! loop will call these same commands.

use super::compile;
use super::events::EventLog;
use super::facts::Factsheet;
use super::manifest::Catalog;
use super::model::{Location, ManagedData, ModeData, Offering, PortAllocation, Status, WorkloadSpec};
use super::ports::{self, Claim, Pool};
use super::registry::Registry;
use super::runtime::{RuntimeRegistry, RuntimeError};
use super::directory::OfferingsRoot;
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
    /// Where offering directories live (rehydration contract, OFFERINGS.md).
    dirs_root: OfferingsRoot,
    /// The stone's service pool for address allocation (ADR-0002 ruling 1).
    pool: Pool,
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
        dirs_root: OfferingsRoot,
        pool: Pool,
    ) -> Self {
        Self {
            registry,
            worlds,
            default_world,
            catalog,
            facts,
            dirs_root,
            pool,
            failures: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        }
    }

    /// The address ledger (ADR-0002): every managed offering's held port,
    /// from its allocations — or, transitionally before directory migration,
    /// from whatever residence it last recorded. Rest counts; offline is
    /// not free.
    fn ledger(&self) -> Vec<Claim> {
        self.registry
            .snapshot()
            .iter()
            .filter_map(|o| {
                let m = o.managed()?;
                let homes = if m.spec.allocations.is_empty() {
                    // Transitional: derive claims from recorded residences.
                    m.port_map
                        .values()
                        .map(|port| (*port, super::ports::Tier::Flexible))
                        .collect::<Vec<_>>()
                } else {
                    m.spec
                        .allocations
                        .values()
                        .map(|a| (a.home, a.tier))
                        .collect::<Vec<_>>()
                };
                Some(
                    homes
                        .into_iter()
                        .map(|(port, _tier)| Claim::new(&o.name, port)),
                )
            })
            .flatten()
            .collect()
    }

    /// Append to an offering's audit ledger; failures warn but never block.
    fn audit(&self, name: &str, kind: &str, details: serde_json::Value) {
        let log = EventLog::for_dir(&self.dirs_root.base, name);
        if let Err(e) = log.append(kind, details) {
            tracing::warn!(offering = %name, error = %e, "audit append failed");
        }
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

    pub(crate) fn audit_healed(&self, name: &str) {
        self.audit(name, "Healed", serde_json::json!({}));
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
            // One machine-truth parse (OFFERINGS.md §5.1): a catalog-named
            // offering's image comes from its manifest. Explicit overrides
            // would fork deployed reality from compiled decisions.
            if image.is_some() {
                return Err(CommandError::Conflict(format!(
                    "'{name}' is a catalog offering; its manifest defines the image and no explicit image may be supplied"
                )));
            }
            if m.managed.is_none() {
                return Err(CommandError::Conflict(format!(
                    "'{name}' declares no managed placement"
                )));
            }
            let facts_gen = self.facts.snapshot();
            let dir = self.dirs_root.dir_for(name);
            let claims = self.ledger();
            let plan = compile::compile(m, &facts_gen, inputs, &dir, &claims, self.pool).map_err(|e| match e {
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
                // Provenance is the manifest's (§5.1 machine-truth): a
                // client-supplied category must not rewrite catalog identity.
                category: m.category.clone(),
                status: Status::Running,
                location: Location {
                    host: "localhost".into(),
                    port: placement.named_host_ports.values().copied().next().unwrap_or(0),
                    protocol: "http".into(),
                },
                mode_data: ModeData::Managed(ManagedData {
                    runtime_kind: kind.clone(),
                    spec: plan.workload.clone(),
                    port_map: placement.named_host_ports,
                    plan: Some(plan_value),
                }),                registered_at: now,
                updated_at: now,
            };
            self.registry.register(offering.clone());
            self.audit(name, "Placed", serde_json::json!({ "world": kind, "catalog": true }));
            return Ok(offering);
        }


        // Ad-hoc path: a raw image with no catalog behind it.
        let Some(image) = image else {
            return Err(CommandError::NotFound(format!(
                "no catalog entry for '{name}' and no image given"
            )));
        };
        // ADR-0002: ad-hoc offerings are citizens too — their roles draw
        // stable homes from the same ledger the catalog path uses.
        let mut intents = std::collections::BTreeMap::new();
        for role in named_ports.keys() {
            intents.insert(
                role.clone(),
                ports::Intent {
                    tier: ports::Tier::Flexible,
                    home: None,
                },
            );
        }
        let claims = self.ledger();
        let homes = ports::allocate(&intents, &claims, self.pool).map_err(|e| match e {
            ports::AllocError::ClaimConflict { port, holder } => CommandError::Conflict(format!(
                "host port {port} is held by garden member '{holder}'"
            )),
            other => CommandError::Conflict(format!("address allocation refused: {other}")),
        })?;
        let spec = WorkloadSpec {
            image: image.clone(),
            named_ports: named_ports.clone(),
            allocations: homes
                .iter()
                .map(|(role, home)| {
                    (
                        role.clone(),
                        PortAllocation {
                            home: *home,
                            tier: ports::Tier::Flexible,
                        },
                    )
                })
                .collect(),
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
                runtime_kind: kind.clone(),
                spec,
                port_map: placement.named_host_ports,
                plan: None,
            }),
            registered_at: now,
            updated_at: now,
        };
        self.registry.register(offering.clone());
        self.audit(name, "Placed", serde_json::json!({ "world": kind, "catalog": false }));
        Ok(offering)
    }

    /// How many catalog offerings this stone could place today.
    pub fn catalog_size(&self) -> usize {
        self.catalog.len()
    }

    /// The full placed record — plan attached (§5.3).
    pub fn placed(&self, name: &str) -> Option<Offering> {
        self.registry.get_by_name(name)
    }

    /// Rest: stopped, and reconcile will keep it so (§3.2).
    pub async fn rest(&self, name: &str) -> Result<Offering, CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        rt.stop(name).await.map_err(CommandError::Runtime)?;
        self.registry.mark_status(&offering.offering_id, Status::Stopped);
        self.audit(name, "Stopped", serde_json::json!({ "reason": "rest" }));
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
                // The stored spec already carries the ledgered allocations
                // (ADR-0002): identity rides along; residence is chosen at
                // the create edge (squatters relocate, homes remembered).
                let spec = managed.spec.clone();
                rt.place(name, &spec).await.map_err(CommandError::Runtime)?;
                self.audit(name, "Resurrected", serde_json::json!({}));
            }
            Some(observed) if !observed.running => {
                rt.start(name).await.map_err(CommandError::Runtime)?;
            }
            Some(_) => {} // already running; idempotent wake
        }

        self.registry.mark_status(&offering.offering_id, Status::Running);
        self.audit(name, "Started", serde_json::json!({}));

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
        self.audit(name, "Uprooted", serde_json::json!({}));
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

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::manifest::Catalog;
    use crate::offerings::registry::MemorySnapshotStore;
    use crate::offerings::runtime::{NullRuntime, RuntimeRegistry};

    const REDIS: &str = "\
kind: software
name: redis
category: data
description: In-memory cache
managed:
  world: oci
  image: redis:7-alpine
  ports: { default: 6379 }
";

    fn service_with(catalog: Catalog) -> (OfferingService, std::path::PathBuf) {
        let root = std::env::temp_dir()
            .join(format!("moss-service-{}-{}", std::process::id(), uuid::Uuid::now_v7()));
        let service = OfferingService::new(
            Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default()))),
            Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)])),
            "null".into(),
            Arc::new(catalog),
            Arc::new(Factsheet::empty()),
            OfferingsRoot::new(root.clone()),
            super::ports::Pool::default(),
        );
        (service, root)
    }

    fn inputs() -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::new()
    }

    /// §5.1 single machine-truth: a catalog-named offering's workload comes
    /// from its manifest; an explicit image may not fork deployed reality
    /// from compiled decisions.
    #[tokio::test]
    async fn catalog_named_plants_reject_explicit_image() {
        let catalog = Catalog::embedded([("redis", REDIS)]).unwrap();
        let (service, _root) = service_with(catalog);

        let err = service
            .offer("redis", Some("redis:9".into()), HashMap::new(), None, None, &inputs())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("catalog") && msg.contains("manifest"),
            "refusal should name the contract, got: {msg}"
        );
    }

    /// The rejection is specific: WITHOUT an explicit image the same plant
    /// proceeds past the surface into compile/world work (here: the null
    /// world refusing placement — not the catalog contract firing).
    #[tokio::test]
    async fn catalog_plants_without_image_reach_the_world() {
        let catalog = Catalog::embedded([("redis", REDIS)]).unwrap();
        let (service, _root) = service_with(catalog);

        let err = service
            .offer("redis", None, HashMap::new(), None, None, &inputs())
            .await
            .unwrap_err();
        // The null world's own refusal — proof the plant passed the surface.
        assert_eq!(err.to_string(), "runtime unsupported here: the null world places nothing");
    }
}

