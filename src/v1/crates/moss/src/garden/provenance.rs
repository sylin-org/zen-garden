//! Provenance (ADR-0015): where an offering comes from and whether it
//! can come HERE — the manifests, the versions, the decisions, spoken
//! BEFORE anything is placed.
//!
//! Two verbs, one decision path:
//! - [`Provenance::plan_install`] — the dry twin: locate the manifest,
//!   compile the placement decisions against this stone's facts and
//!   ledger, and answer *can it, and why / why not*. Nothing is placed.
//! - [`Provenance::install`] — run the SAME plan as a job: resolve,
//!   place, start — with the steps and their progress visible on the
//!   pulse for the job's whole life. The result is the placed offering.

use super::compile::{self, Decision, PlacementPlan};
use super::model::{ManagedData, ModeData, Offering, PortAllocation, Status, WorkloadSpec};
use super::service::{CommandError, OfferingService};
use crate::jobs::JobTracker;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The answer to "can this come here, and what would it look like".
#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub fqn: String,
    pub stem: String,
    /// True when placement would proceed.
    pub can: bool,
    /// When `can`: the decision trail (why THIS shape). When not: the
    /// refusals, plainly worded.
    pub because: Vec<String>,
    /// The image that would run (manifest-defined for catalog offerings).
    pub image: Option<String>,
    /// The compiled placement — present only when `can`.
    pub compiled: Option<PlacementPlan>,
}

/// The catalog's mouth: plan and install offerings by name.
pub struct Provenance<'a> {
    garden: &'a OfferingService,
}

impl<'a> Provenance<'a> {
    pub fn new(garden: &'a OfferingService) -> Self {
        Self { garden }
    }

    /// The dry twin. Locates the manifest, compiles the placement
    /// against this stone's facts and ledger, and reports the verdict —
    /// NO placement, NO pull, NO registry writes. Same decision path
    /// [`Self::install`] will run.
    pub fn plan_install(
        &self,
        name: &str,
        image: Option<String>,
        inputs: &BTreeMap<String, String>,
    ) -> Result<InstallPlan, CommandError> {
        let fqn = garden_glossary::fqn::canonicalize(name)
            .map_err(|e| CommandError::Conflict(e.to_string()))?;
        let stem = garden_glossary::fqn::stem_of(&fqn).to_string();

        if self.garden.placed(&fqn).is_some() {
            return Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![format!("'{fqn}' is already planted here")],
                image: None,
                compiled: None,
            });
        }

        let Some(m) = self.garden.catalog.get(&stem) else {
            // Ad-hoc: an image with no catalog behind it can still come.
            return Ok(InstallPlan {
                can: image.is_some(),
                because: vec![if image.is_some() {
                    "no catalog entry - ad-hoc image placement".to_string()
                } else {
                    format!("no catalog entry for '{stem}' and no image given")
                }],
                image,
                compiled: None,
                fqn: fqn.clone(),
                stem: stem.clone(),
            });
        };

        if m.managed.is_none() {
            return Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![format!("'{stem}' declares no managed placement")],
                image: None,
                compiled: None,
            });
        }
        if image.is_some() {
            return Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![format!(
                    "'{fqn}' is a catalog offering; its manifest defines the image and no explicit image may be supplied"
                )],
                image: None,
                compiled: None,
            });
        }

        let facts_gen = self.garden.facts.snapshot();
        let dir = self.garden.dirs_root.dir_for(&fqn);
        let claims = self.garden.ledger();
        match compile::compile(m, &facts_gen, inputs, &dir, &claims, self.garden.pool) {
            Ok(plan) => {
                let because = plan
                    .decisions
                    .iter()
                    .map(|d: &Decision| {
                        format!(
                            "{}: {} ({})",
                            d.rule,
                            d.chose,
                            if d.because.is_empty() { "declared" } else { d.because.as_str() }
                        )
                    })
                    .collect();
                Ok(InstallPlan {
                    fqn: fqn.clone(),
                    stem: stem.clone(),
                    can: true,
                    because,
                    image: Some(plan.workload.image.clone()),
                    compiled: Some(plan),
                })
            }
            Err(super::compile::CompileError::Denied { because, suggest }) => Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![format!(
                    "compatibility denied: {because}{}",
                    suggest.as_deref().map(|s| format!(" — {s}")).unwrap_or_default()
                )],
                image: None,
                compiled: None,
            }),
            Err(other) => Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![other.to_string()],
                image: None,
                compiled: None,
            }),
        }
    }

    /// Install: the plan, EXECUTED. Resolve → decide → place → start,
    /// as a job whose steps ride the pulse. `jobs = None` runs the same
    /// pipeline silently (service wrappers, tests).
    #[allow(clippy::too_many_arguments)]
    pub async fn install(
        &self,
        name: &str,
        image: Option<String>,
        named_ports: std::collections::HashMap<String, u16>,
        category: Option<String>,
        requested_world: Option<&str>,
        inputs: &BTreeMap<String, String>,
        jobs: Option<&JobTracker>,
    ) -> Result<(Offering, Option<String>), CommandError> {
        let job = jobs.map(|j| j.start(crate::jobs::kind::INSTALL, name));
        let say = |jobs: Option<&JobTracker>, job: &Option<String>, line: String| {
            if let (Some(j), Some(id)) = (jobs, job) {
                j.progress(id, line);
            }
        };
        match self
            .install_inner(
                name,
                image,
                named_ports,
                category,
                requested_world,
                inputs,
                jobs,
                &job,
            )
            .await
        {
            Ok(offering) => {
                if let (Some(j), Some(id)) = (jobs, &job) {
                    j.complete(id, serde_json::json!({ "fqn": offering.name }));
                }
                Ok((offering, job))
            }
            Err(e) => {
                if let (Some(j), Some(id)) = (jobs, &job) {
                    j.fail(id, &e.to_string());
                }
                Err(e)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn install_inner(
        &self,
        name: &str,
        image: Option<String>,
        named_ports: std::collections::HashMap<String, u16>,
        category: Option<String>,
        requested_world: Option<&str>,
        inputs: &BTreeMap<String, String>,
        jobs: Option<&JobTracker>,
        job: &Option<String>,
    ) -> Result<Offering, CommandError> {
        let plan = self.plan_install(name, image.clone(), inputs)?;
        if !plan.can {
            return Err(CommandError::Conflict(plan.because.join("; ")));
        }
        if let (Some(j), Some(id)) = (jobs, job) {
            j.progress(id, "decided — placing".to_string());
        }

        let kind = requested_world.unwrap_or(&self.garden.default_world).to_string();
        let rt = self.garden.runtime_for_kind(&kind)?;

        // Catalog offerings are placed FROM THE COMPILED PLAN; ad-hoc
        // ones draw flexible homes from the same ledger first.
        let (workload, plan_value, stem, category) = if let Some(compiled) = &plan.compiled {
            let plan_value = serde_json::to_value(compiled)
                .map_err(|e| CommandError::Conflict(format!("plan encode: {e}")))?;
            let category = self
                .garden
                .catalog
                .get(&plan.stem)
                .map(|m| m.category.clone())
                .unwrap_or_else(|| "misc".into());
            (
                compiled.workload.clone(),
                Some(plan_value),
                plan.stem.clone(),
                category,
            )
        } else {
            let Some(image) = &image else {
                return Err(CommandError::NotFound(format!(
                    "no catalog entry for '{}' and no image given",
                    plan.stem
                )));
            };
            let mut intents = BTreeMap::new();
            for role in named_ports.keys() {
                intents.insert(
                    role.clone(),
                    super::ports::Intent { tier: super::ports::Tier::Flexible, home: None },
                );
            }
            let claims = self.garden.ledger();
            let homes = super::ports::allocate(&intents, &claims, self.garden.pool)
                .map_err(|e| match e {
                    super::ports::AllocError::ClaimConflict { port, holder } => {
                        CommandError::Conflict(format!(
                            "host port {port} is held by garden member '{holder}'"
                        ))
                    }
                    other => CommandError::Conflict(format!(
                        "address allocation refused: {other}"
                    )),
                })?;
            let spec = WorkloadSpec {
                image: image.clone(),
                named_ports: named_ports.clone(),
                allocations: homes
                    .iter()
                    .map(|(role, home)| {
                        (
                            role.clone(),
                            PortAllocation { home: *home, tier: super::ports::Tier::Flexible },
                        )
                    })
                    .collect(),
                ..Default::default()
            };
            (spec, None, plan.stem.clone(), category.unwrap_or_else(|| "misc".into()))
        };

        let placement = rt
            .place(&plan.fqn, &workload)
            .await
            .map_err(CommandError::Runtime)?;
        let now = chrono::Utc::now();
        let offering = Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: plan.fqn.clone(),
            offering: stem,
            category,
            status: Status::Running,
            location: super::model::Location {
                host: "localhost".into(),
                port: placement.named_host_ports.values().copied().next().unwrap_or(0),
                protocol: "http".into(),
            },
            sub_capabilities: Default::default(),
            mode_data: ModeData::Managed(ManagedData {
                runtime_kind: kind.clone(),
                spec: workload,
                port_map: placement.named_host_ports,
                plan: plan_value,
            }),
            registered_at: now,
            updated_at: now,
        };
        self.garden.register_placed(offering.clone(), &kind);
        Ok(offering)
    }
}

impl OfferingService {
    /// The catalog's mouth: plan and install offerings by name.
    pub fn provenance(&self) -> Provenance<'_> {
        Provenance::new(self)
    }

    /// The placement persistence: the registry hears the incarnation and
    /// the chain opens with Placed (Provenance decides — the service
    /// owns what is remembered).
    pub(crate) fn register_placed(&self, offering: Offering, world_kind: &str) {
        self.registry.register(offering.clone());
        self.audit(
            &offering.name,
            super::events::audit_kind::PLACED,
            serde_json::json!({ "world": world_kind, "catalog": self.catalog.get(&offering.offering).is_some() }),
        );
    }

    /// A runtime by world kind, for contexts that place directly.
    pub(crate) fn runtime_for_kind(
        &self,
        kind: &str,
    ) -> Result<Arc<dyn super::runtime::Runtime>, CommandError> {
        self.worlds
            .by_kind(kind)
            .map_err(CommandError::WorldUnavailable)
    }
}
