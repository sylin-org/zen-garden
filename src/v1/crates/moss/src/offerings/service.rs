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
    /// The request itself is malformed — it never had a chance.
    BadRequest(String),
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
            Self::BadRequest(m) => write!(f, "{m}"),
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
    pub dirs_root: OfferingsRoot,
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
    /// Replant (ADR-0005 §6): bring a restored incarnation to life.
    /// The offering arrives from a verified checkpoint's directory -
    /// SAME FQN, SAME stored spec, SAME connection strings. Place runs
    /// from the stored spec (no catalog needed: resurrection needs no
    /// original request), allocations ride their ledgered homes, and the
    /// audit chain opens with Replanted{predecessor, final_hash}.
    pub async fn replant(
        &self,
        mut offering: Offering,
        final_hash: &str,
    ) -> Result<Offering, CommandError> {
        let fqn = offering.name.clone();
        if self.placed(&fqn).is_some() {
            return Err(CommandError::Conflict(format!(
                "'{fqn}' is already incarnate here - replant restores the dead, not the doubled"
            )));
        }
        let ModeData::Managed(managed) = &offering.mode_data else {
            return Err(CommandError::Conflict(format!(
                "'{fqn}' is not a managed offering - only managed work replants"
            )));
        };
        let kind = if managed.runtime_kind.is_empty() {
            self.default_world.clone()
        } else {
            managed.runtime_kind.clone()
        };
        let rt = self.worlds.by_kind(&kind).map_err(CommandError::Conflict)?;
        let placement = rt
            .place(&fqn, &managed.spec)
            .await
            .map_err(CommandError::Runtime)?;

        let now = chrono::Utc::now();
        offering.status = Status::Running;
        offering.location = Location {
            host: "localhost".into(),
            port: placement.named_host_ports.values().copied().next().unwrap_or(0),
            protocol: "http".into(),
        };
        if let ModeData::Managed(m) = &mut offering.mode_data {
            m.port_map = placement.named_host_ports;
        }
        offering.updated_at = now;
        self.registry.register(offering.clone());
        self.audit(
            &fqn,
            "Replanted",
            serde_json::json!({
                "predecessor_offering_id": offering.offering_id,
                "final_hash": final_hash,
                "world": kind,
            }),
        );
        tracing::info!(offering = %fqn, world = %kind, "replanted from its checkpoint");
        Ok(offering)
    }

    /// Placed managed offerings with a TRUSTED declared will (ADR-0005):
    /// the scheduler's capture set. Untrusted offerings are surfaced
    /// honestly on their faces, never silently tarred here.
    pub fn capture_targets(&self) -> Vec<(Offering, super::capture::CapturePolicy)> {
        let mut out = Vec::new();
        for o in self.registry.snapshot() {
            if o.managed().is_none() {
                continue;
            }
            if let Some(manifest) = self.catalog.get(&o.offering)
                && let Some(policy) = &manifest.capture
            {
                out.push((o, policy.clone()));
            }
        }
        out
    }

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

    /// Every placed offering, sorted by name (the collection face's data).
    pub fn snapshot(&self) -> Vec<Offering> {
        self.registry.snapshot()
    }

    /// Subscribe to OfferingChanged (L18) - the pulse surface's offering leg.
    pub fn events(&self) -> tokio::sync::broadcast::Receiver<super::registry::OfferingChanged> {
        self.registry.events()
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

    /// Plant an offering by name (OFFERINGS.md §5). Names arrive as
    /// MONIKERS (`ollama`) or explicit FQNs (`ollama::prod`); the grammar
    /// (glossary::fqn) canonicalizes both to machine truth. Instances of a
    /// stem share its catalog manifest while holding their own identity,
    /// directory, decisions, and ledgered addresses.
    pub async fn offer(
        &self,
        name: &str,
        image: Option<String>,
        named_ports: std::collections::HashMap<String, u16>,
        category: Option<String>,
        requested_world: Option<&str>,
        inputs: &std::collections::BTreeMap<String, String>,
    ) -> Result<Offering, CommandError> {
        let fqn = garden_glossary::fqn::canonicalize(name)
            .map_err(|e| CommandError::Conflict(e.to_string()))?;
        if self.registry.get_by_name(&fqn).is_some() {
            return Err(CommandError::Conflict(format!("'{fqn}' is already planted")));
        }
        let kind = requested_world.unwrap_or(&self.default_world).to_string();
        let rt = self.worlds.by_kind(&kind).map_err(CommandError::WorldUnavailable)?;

        // Catalog path: manifest is truth; compile decides. Every instance
        // inherits its STEM's manifest; only stems exist in the catalog.
        let stem = garden_glossary::fqn::stem_of(&fqn);
        if let Some(m) = self.catalog.get(&stem) {
            // One machine-truth parse (OFFERINGS.md §5.1): a catalog-named
            // offering's image comes from its manifest. Explicit overrides
            // would fork deployed reality from compiled decisions.
            if image.is_some() {
                return Err(CommandError::Conflict(format!(
                    "'{fqn}' is a catalog offering; its manifest defines the image and no explicit image may be supplied"
                )));
            }
            if m.managed.is_none() {
                return Err(CommandError::Conflict(format!(
                    "'{fqn}' declares no managed placement"
                )));
            }
            let facts_gen = self.facts.snapshot();
            let dir = self.dirs_root.dir_for(&fqn);
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
                rt.place(&fqn, &plan.workload).await.map_err(CommandError::Runtime)?;
            let now = chrono::Utc::now();
            let plan_value = serde_json::to_value(&plan)
                .map_err(|e| CommandError::Conflict(format!("plan encode: {e}")))?;
            let offering = Offering {
                offering_id: uuid::Uuid::now_v7().to_string(),
                // Identity is the FQN; provenance is the stem. Both
                // `memcached` and `memcached::prod`.offering == "memcached".
                name: fqn.clone(),
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
                }),
                registered_at: now,
                updated_at: now,
            };
            self.registry.register(offering.clone());
            self.audit(&fqn, "Placed", serde_json::json!({ "world": kind, "catalog": true }));
            return Ok(offering);
        }


        // Ad-hoc path: a raw image with no catalog behind it.
        let Some(image) = image else {
            return Err(CommandError::NotFound(format!(
                "no catalog entry for '{stem}' and no image given"
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
        let placement = rt.place(&fqn, &spec).await.map_err(CommandError::Runtime)?;
        let now = chrono::Utc::now();
        let offering = Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: fqn.clone(),
            // Ad-hoc offerings are their own stem (no catalog behind them).
            offering: stem,
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
        self.audit(&fqn, "Placed", serde_json::json!({ "world": kind, "catalog": false }));
        Ok(offering)
    }

    /// How many catalog offerings this stone could place today.
    pub fn catalog_size(&self) -> usize {
        self.catalog.len()
    }

    /// The full placed record — plan attached (§5.3). Monikers resolve to
    /// their FQN (`memcached` finds `memcached::default`).
    pub fn placed(&self, name: &str) -> Option<Offering> {
        let fqn = garden_glossary::fqn::canonicalize(name).ok()?;
        self.registry.get_by_name(&fqn)
    }

    /// Follow an offering's logs through its bound world. `None` when
    /// the offering is not placed here or its world cannot stream logs
    /// (the null world opts out at its own seam).
    /// The nourish check (J3): refresh the offering's image reference and
    /// say whether the container would now run something different.
    pub async fn update_check(
        &self,
        name: &str,
    ) -> Result<super::runtime::ImageRefresh, CommandError> {
        let offering = self.placed(name).ok_or_else(|| {
            CommandError::NotFound(format!("no offering '{name}' is planted here"))
        })?;
        let managed = offering
            .managed()
            .ok_or_else(|| CommandError::Conflict(format!("'{}' is not managed - updates are for garden-placed work", offering.name)))?;
        let world = self.world_for(&offering)?;
        match world.refresh_image(&managed.spec.image).await {
            Some(Ok(r)) => Ok(r),
            Some(Err(e)) => Err(CommandError::Runtime(e)),
            None => Err(CommandError::WorldUnavailable(format!(
                "'{}' grows in a world that cannot check updates",
                offering.name
            ))),
        }
    }

    /// The nourish apply (J3): pull the newer image, rebuild the
    /// container from the stored spec (volumes persist — data never
    /// moves), and if the new container will not run, revert to the
    /// pre-pull image ID. Never the watchtower story: nothing applies
    /// unless this face is asked.
    pub async fn update_offering(
        &self,
        name: &str,
    ) -> Result<super::runtime::ImageRefresh, CommandError> {
        let offering = self.placed(name).ok_or_else(|| {
            CommandError::NotFound(format!("no offering '{name}' is planted here"))
        })?;
        let managed = offering
            .managed()
            .ok_or_else(|| CommandError::Conflict(format!("'{}' is not managed - updates are for garden-placed work", offering.name)))?;
        let world = self.world_for(&offering)?;
        let Some(Ok(refresh)) = world.refresh_image(&managed.spec.image).await else {
            return Err(CommandError::WorldUnavailable(format!(
                "'{}' grows in a world that cannot check updates",
                offering.name
            )));
        };
        if !refresh.changed {
            return Ok(refresh); // already the newest: nothing to do
        }
        // Rebuild from the stored spec: remove the container (the image
        // tag now resolves to the NEW id), place again, demand running.
        world
            .remove(name)
            .await
            .map_err(CommandError::Runtime)?;
        match world.place(name, &managed.spec).await {
            Ok(_) => Ok(refresh),
            Err(_) => {
                // The new image will not run: revert to the pre-pull ID.
                let mut reverted = managed.spec.clone();
                reverted.image = refresh.id.clone();
                world
                    .place(name, &reverted)
                    .await
                    .map_err(CommandError::Runtime)?;
                Err(CommandError::Runtime(RuntimeError::Failed(format!(
                    "update placed but failed to run; reverted to the pre-pull image ({})",
                    refresh.id
                ))))
            }
        }
    }

    /// Follow an offering's logs through its bound world. `None` when
    /// the offering is not placed here or its world cannot stream logs.
    pub fn logs_stream(
        &self,
        name: &str,
        tail: Option<u64>,
        timestamps: bool,
    ) -> Option<super::runtime::LogStream> {
        let offering = self.placed(name)?;
        let managed = offering.managed()?;
        let rt = self.worlds.by_kind(&managed.runtime_kind).ok()?;
        rt.logs_stream(&offering.name, tail, timestamps)
    }

    /// Rest: stopped, and reconcile will keep it so (§3.2).
    pub async fn rest(&self, name: &str) -> Result<Offering, CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        let fqn = offering.name.clone();
        rt.stop(&fqn).await.map_err(CommandError::Runtime)?;
        self.registry.mark_status(&offering.offering_id, Status::Stopped);
        self.audit(&fqn, "Stopped", serde_json::json!({ "reason": "rest" }));
        self.registry.get_by_name(&fqn).ok_or(CommandError::NotFound(fqn))
    }

    /// Wake: running again — resurrecting the workload from its stored spec
    /// if reality lost it behind our backs (PoC wake parity).
    pub async fn wake(&self, name: &str) -> Result<Offering, CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        let fqn = offering.name.clone();
        let managed = offering.managed().ok_or_else(|| {
            CommandError::Conflict(format!("'{fqn}' is not managed"))
        })?;

        match rt.observe(&fqn).await {
            None => {
                tracing::warn!(offering = %fqn, "workload missing - resurrecting from stored spec");
                // The stored spec already carries the ledgered allocations
                // (ADR-0002): identity rides along; residence is chosen at
                // the create edge (squatters relocate, homes remembered).
                let spec = managed.spec.clone();
                rt.place(&fqn, &spec).await.map_err(CommandError::Runtime)?;
                self.audit(&fqn, "Resurrected", serde_json::json!({}));
            }
            Some(observed) if !observed.running => {
                rt.start(&fqn).await.map_err(CommandError::Runtime)?;
            }
            Some(_) => {} // already running; idempotent wake
        }

        self.registry.mark_status(&offering.offering_id, Status::Running);
        self.audit(&fqn, "Started", serde_json::json!({}));

        // Port honesty: observe and refresh rather than lie about stale
        // mappings. Relocation under pressure is recorded, never denied.
        let mut updated = offering;
        updated.status = Status::Running; // the stale clone must not undo the mark above
        if let ModeData::Managed(m) = &mut updated.mode_data
            && let Some(observed) = rt.observe(&fqn).await
            && observed.named_host_ports != m.port_map
            && !observed.named_host_ports.is_empty()
        {
            m.port_map = observed.named_host_ports;
            updated.location.port = m.port_map.values().copied().next().unwrap_or(0);
            updated.updated_at = chrono::Utc::now();
            self.registry.replace(updated.clone());
            tracing::info!(offering = %fqn, "host ports remapped after wake");
        }
        self.registry.get_by_name(&fqn).ok_or(CommandError::NotFound(fqn))
    }

    /// Uproot: remove the workload and forget the offering. Managed only —
    /// adopted release / borrowed return arrive with their modes (O3).
    pub async fn uproot(&self, name: &str) -> Result<(), CommandError> {
        let offering = self.managed(name)?;
        let rt = self.world_for(&offering)?;
        let fqn = offering.name.clone();
        rt.remove(&fqn).await.map_err(CommandError::Runtime)?;
        self.registry.remove(&offering.offering_id);
        self.audit(&fqn, "Uprooted", serde_json::json!({}));
        Ok(())
    }

    /// Managed-only lookup. Monikers canonicalize here (`uproot memcached`
    /// uproots `memcached::default`); off-grammar names refuse with hints.
    fn managed(&self, name: &str) -> Result<Offering, CommandError> {
        let fqn = garden_glossary::fqn::canonicalize(name)
            .map_err(|e| CommandError::Conflict(e.to_string()))?;
        let o = self
            .registry
            .get_by_name(&fqn)
            .ok_or(CommandError::NotFound(fqn.clone()))?;
        if o.managed().is_none() {
            return Err(CommandError::Conflict(format!("'{fqn}' is not a managed offering")));
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
    use crate::offerings::runtime::{NullRuntime, Observed, Placement, PlacedRef, Runtime, RuntimeError, RuntimeRegistry};

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

    /// A place-anywhere fake: binds every role exactly at its ledgered home.
    /// Lets sequencing tests run real offer flows without Docker.
    struct RecordingRuntime;

    #[async_trait::async_trait]
    impl Runtime for RecordingRuntime {
        fn kind(&self) -> &'static str {
            "rec"
        }

        async fn place(
            &self,
            _name: &str,
            spec: &WorkloadSpec,
        ) -> Result<Placement, RuntimeError> {
            let mut named = HashMap::new();
            for (role, alloc) in &spec.allocations {
                named.insert(role.clone(), alloc.home);
            }
            Ok(Placement { named_host_ports: named })
        }

        async fn start(&self, _name: &str) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn stop(&self, _name: &str) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn remove(&self, _name: &str) -> Result<(), RuntimeError> {
            Ok(())
        }

        async fn observe(&self, _name: &str) -> Option<Observed> {
            Some(Observed {
                running: true,
                named_host_ports: HashMap::new(),
            })
        }

        async fn list(&self) -> Vec<PlacedRef> {
            Vec::new()
        }
    }

    fn service_with(catalog: Catalog) -> (OfferingService, std::path::PathBuf) {
        let root = std::env::temp_dir()
            .join(format!("moss-service-{}-{}", std::process::id(), uuid::Uuid::now_v7()));
        let service = OfferingService::new(
            Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default()))),
            Arc::new(RuntimeRegistry::build(vec![
                Arc::new(NullRuntime),
                Arc::new(RecordingRuntime),
            ])),
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

    /// Multi-instance hosts (namespace law): two installations of one stem
    /// coexist under FQN identities, each with its OWN ledgered address.
    /// The second draws :7301 regardless of the first's status — claims
    /// decide, not sockets. The bare moniker plants ::default.
    #[tokio::test]
    async fn named_installations_share_a_stem_but_not_addresses() {
        let catalog = Catalog::embedded([("redis", REDIS)]).unwrap();
        let (service, _root) = service_with(catalog);

        let first = service
            .offer("redis", None, HashMap::new(), None, Some("rec"), &inputs())
            .await
            .unwrap();
        let second = service
            .offer("redis::prod", None, HashMap::new(), None, Some("rec"), &inputs())
            .await
            .unwrap();

        // Distinct identities over shared provenance.
        assert_eq!(first.name, "redis::default");
        assert_eq!(second.name, "redis::prod");
        assert_eq!(second.offering, "redis", "provenance stays the stem");
        assert_ne!(first.offering_id, second.offering_id);

        // And the point of it all: ledger-first addresses, ascending.
        let m1 = first.managed().unwrap();
        let m2 = second.managed().unwrap();
        assert_eq!(m1.spec.allocations["default"].home, 7300);
        assert_eq!(m2.spec.allocations["default"].home, 7301);
        assert_eq!(m2.port_map["default"], 7301);
    }

    /// Surfaces speak moniker; machine truth is the FQN. `rest redis` and
    /// `explain redis` must find `redis::default`.
    #[tokio::test]
    async fn moniker_arguments_resolve_to_the_default_instance() {
        let catalog = Catalog::embedded([("redis", REDIS)]).unwrap();
        let (service, _root) = service_with(catalog);

        service
            .offer("redis", None, HashMap::new(), None, Some("rec"), &inputs())
            .await
            .unwrap();

        assert!(
            service.placed("redis").is_some(),
            "moniker lookup finds the default instance"
        );
        assert_eq!(service.rest("redis").await.unwrap().name, "redis::default");
        assert!(service.wake("redis").await.is_ok());
        // Double-plant by alias spelling collides — same identity.
        let err = service
            .offer("redis::default", None, HashMap::new(), None, Some("rec"), &inputs())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already planted"));
    }

    /// Off-grammar names refuse loudly — image-tag shapes get the '::'
    /// hint instead of a confusing catalog miss.
    #[tokio::test]
    async fn single_colon_names_refuse_with_the_namespace_hint() {
        let catalog = Catalog::embedded([("redis", REDIS)]).unwrap();
        let (service, _root) = service_with(catalog);

        let err = service
            .offer("redis:7-alpine", None, HashMap::new(), None, Some("rec"), &inputs())
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("redis::7-alpine") && msg.contains("':'"), "{msg}");
    }
}

