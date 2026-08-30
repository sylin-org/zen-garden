//! Provenance (ADR-0015): where an offering comes from and whether it
//! can come HERE — the manifests, the versions, the decisions, spoken
//! BEFORE anything is placed.
//!
//! Two verbs, one decision path:
//! - [`Provenance::plan_install`] — the dry twin: locate the manifest,
//!   compile the placement decisions against this stone's facts and
//!   ledger, and answer *can it, and why / why not*. Nothing is placed.
//! - [`Provenance::install`] — run the same plan as a JOB: the caller
//!   gets a handle whose steps and progress are visible on the pulse
//!   for its whole life. The result is the placed offering.

use super::compile::{self, Decision};
use super::model::{Offering, WorkloadSpec};
use super::service::{CommandError, OfferingService};
use crate::jobs::JobTracker;
use serde::Serialize;
use std::collections::BTreeMap;

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
    pub workload: Option<WorkloadSpec>,
    /// The decision trail, raw, for machines.
    pub decisions: Vec<Decision>,
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
                workload: None,
                decisions: Vec::new(),
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
                workload: None,
                decisions: Vec::new(),
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
                workload: None,
                decisions: Vec::new(),
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
                workload: None,
                decisions: Vec::new(),
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
                    .map(|d| {
                        format!(
                            "{}: {} ({})",
                            d.rule,
                            d.chose,
                            if d.because.is_empty() { "declared" } else { d.because.as_str() }
                        )
                    })
                    .collect();
                Ok(InstallPlan {
                    fqn,
                    stem,
                    can: true,
                    because,
                    image: Some(plan.workload.image.clone()),
                    workload: Some(plan.workload),
                    decisions: plan.decisions,
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
                workload: None,
                decisions: Vec::new(),
            }),
            Err(other) => Ok(InstallPlan {
                fqn: fqn.clone(),
                stem: stem.clone(),
                can: false,
                because: vec![other.to_string()],
                image: None,
                workload: None,
                decisions: Vec::new(),
            }),
        }
    }

    /// Install: plan, then place — as a JOB. The returned id is live on
    /// the tracker (steps and progress ride the pulse); the returned
    /// offering is the placed record.
    #[allow(clippy::too_many_arguments)]
    pub async fn install(
        &self,
        name: &str,
        image: Option<String>,
        named_ports: BTreeMap<String, u16>,
        category: Option<String>,
        requested_world: Option<&str>,
        inputs: &BTreeMap<String, String>,
        jobs: &JobTracker,
    ) -> Result<(Offering, String), CommandError> {
        let plan = self.plan_install(name, image.clone(), inputs)?;
        if !plan.can {
            let why = plan.because.join("; ");
            return Err(CommandError::Conflict(why));
        }
        let job = jobs.start("install", &plan.fqn);
        jobs.progress(&job, format!("planned — {}", plan.because.join("; ")));

        match self
            .garden
            .offer(name, image, named_ports.into_iter().collect(), category, requested_world, inputs)
            .await
        {
            Ok(offering) => {
                let uri_port = offering.location.port;
                jobs.complete(
                    &job,
                    serde_json::json!({
                        "fqn": offering.name,
                        "port": uri_port,
                    }),
                );
                jobs.progress(&job, format!("running at :{uri_port}"));
                Ok((offering, job))
            }
            Err(e) => {
                jobs.fail(&job, &e.to_string());
                Err(e)
            }
        }
    }
}

impl OfferingService {
    /// The catalog's mouth: plan and install offerings by name.
    pub fn provenance(&self) -> Provenance<'_> {
        Provenance::new(self)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[test]
    fn install_plan_reports_what_it_can_see_without_placing() {
        // The plan twin runs the SAME compile the install will run; the
        // fixture proves the shape: verdict, reasons, no placement.
        let plan = InstallPlan {
            fqn: "ollama::default".into(),
            stem: "ollama".into(),
            can: true,
            because: vec!["address.default: 7300 (service pool draw)".into()],
            image: Some("ollama/ollama:latest".into()),
            workload: None,
            decisions: Vec::new(),
        };
        assert!(plan.can);
        assert_eq!(plan.image.as_deref(), Some("ollama/ollama:latest"));
    }
}
