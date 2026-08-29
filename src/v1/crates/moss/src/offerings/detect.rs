//! The detection domain (OFFERINGS.md §1 adopted mode, §2 ghost law,
//! L25). Catalog detection rules × live container facts → adopted
//! offerings, on the converge sweep clock.
//!
//! THE TRUST RULE — adoption observes, it never operates. This module
//! mints records, confirms candidates, and refreshes statuses. It never
//! places, starts, stops, or removes anything: an adopted workload's
//! lifecycle stays the host's. It dies → the record reports stopped; it
//! returns → the record reports running.

use crate::offerings::manifest::AdoptIntent;
use crate::offerings::model::{AdoptedData, Location, ModeData, Offering, Status};
use crate::offerings::runtime::ContainerFact;
use crate::offerings::service::OfferingService;

/// What one sweep did — every entry is a fact the operator may hear about.
#[derive(Debug, Default, Clone)]
pub struct DetectionReport {
    /// Stems recognized for the first time (minted as candidates, then
    /// confirmed by the same sweep's facts).
    pub minted: Vec<String>,
    /// Previously unseen candidates whose workload came back (ghost law:
    /// the record re-enters the room when its container does).
    pub confirmed: Vec<String>,
    /// Active adopted records whose status moved this sweep.
    pub observed: Vec<String>,
}

impl DetectionReport {
    pub fn is_empty(&self) -> bool {
        self.minted.is_empty() && self.confirmed.is_empty() && self.observed.is_empty()
    }
}

/// One detection sweep. Observe-only by law; safe to run on every tick.
pub async fn detect_once(service: &OfferingService) -> DetectionReport {
    let facts = service.container_facts().await;
    let mut report = DetectionReport::default();
    mint(service, &facts, &mut report);
    confirm(service, &facts, &mut report);
    observe(service, &facts, &mut report);
    report
}

/// Pass 1 — recognize: catalog rules × RUNNING facts. A matching stem
/// with no home here is minted as an adopted record (register lands it
/// in candidates; the confirm pass below returns it the same sweep —
/// the workload is standing right there).
fn mint(service: &OfferingService, facts: &[ContainerFact], report: &mut DetectionReport) {
    for manifest in service.catalog.names() {
        let Some(manifest) = service.catalog.get(&manifest) else { continue };
        let Some(adopt) = &manifest.adopted else { continue };
        let stem = manifest.name.as_str();
        // R1.1: one concept, one home. A stem the garden already knows —
        // planted, adopted, or still confirming — is never re-minted.
        if service.registry().stem_claimed(stem) {
            continue;
        }
        let Some(fact) = facts
            .iter()
            .find(|f| f.running() && pattern_matches(adopt, f))
        else {
            continue;
        };
        let Ok(fqn) = garden_glossary::fqn::canonicalize(&format!("{stem}::adopted")) else {
            continue;
        };
        let now = chrono::Utc::now();
        let offering = Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: fqn.clone(),
            offering: stem.to_string(),
            category: manifest.category.clone(),
            status: fact.status(),
            location: Location {
                host: "localhost".into(),
                port: fact.host_ports.first().copied().unwrap_or(0),
                protocol: "http".into(),
            },
            sub_capabilities: Default::default(),
            mode_data: ModeData::Adopted(AdoptedData {
                control_level: garden_glossary::offering::control::MONITOR.into(),
                start_command: None,
                stop_command: None,
                health_path: None,
                container_name: fact.name.clone(),
            }),
            registered_at: now,
            updated_at: now,
        };
        tracing::info!(
            offering = %fqn,
            container = %fact.name,
            image = %fact.image,
            "offering detected on the host - adopted (observe-only)"
        );
        service.registry().register(offering);
        report.minted.push(fqn);
    }
}

/// Do this rule's patterns accept this fact? Declared patterns must all
/// match; an absent pattern constrains nothing (validation guarantees at
/// least one is declared).
fn pattern_matches(adopt: &AdoptIntent, fact: &ContainerFact) -> bool {
    let ok = |pattern: &Option<String>, subject: &str| match pattern {
        Some(p) => regex::Regex::new(p).is_ok_and(|re| re.is_match(subject)),
        None => true,
    };
    ok(&adopt.container_name_pattern, &fact.name)
        && ok(&adopt.image_pattern, &fact.image)
}

/// Pass 2 — confirm (ghost law, OFFERINGS.md §2): a candidate whose
/// remembered container exists again — running or merely stopped-but-
/// present — re-enters the room carrying the observed status.
fn confirm(service: &OfferingService, facts: &[ContainerFact], report: &mut DetectionReport) {
    for candidate in service.registry().candidates_snapshot() {
        let Some(adopted) = candidate.adopted() else { continue };
        if adopted.container_name.is_empty() {
            continue;
        }
        let Some(fact) = facts
            .iter()
            .find(|f| f.name == adopted.container_name)
        else {
            continue;
        };
        if service
            .registry()
            .promote(&candidate.offering_id, fact.status())
            .is_some()
        {
            tracing::info!(offering = %candidate.name, status = fact.status().as_str(),
                "adopted offering confirmed by detection");
            report.confirmed.push(candidate.name.clone());
        }
    }
}

/// Pass 3 — observe: active adopted records track reality and nothing
/// else. Missing container → stopped. Back again → running. Never heal,
/// never restart, never remove (THE TRUST RULE).
fn observe(service: &OfferingService, facts: &[ContainerFact], report: &mut DetectionReport) {
    for offering in service.registry().snapshot() {
        let Some(adopted) = offering.adopted() else { continue };
        if adopted.container_name.is_empty() {
            continue;
        }
        let status = facts
            .iter()
            .find(|f| f.name == adopted.container_name)
            .map(|f| f.status())
            .unwrap_or(Status::Stopped);
        if status != offering.status
            && service.registry().mark_status(&offering.offering_id, status)
        {
            tracing::info!(offering = %offering.name, status = status.as_str(),
                "adopted workload moved - recorded, not operated");
            report.observed.push(format!("{} → {}", offering.name, status.as_str()));
        }
    }
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::directory::OfferingsRoot;
    use crate::offerings::facts::Factsheet;
    use crate::offerings::manifest::Catalog;
    use crate::offerings::model::WorkloadSpec;
    use crate::offerings::ports::Pool;
    use crate::offerings::registry::{MemorySnapshotStore, Registry, Snapshot, SnapshotStore};
    use crate::offerings::runtime::{
        Observed, Placement, PlacedRef, Runtime, RuntimeError, RuntimeRegistry,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const OLLAMA: &str = "\
kind: software
name: ollama
category: ai
description: Local LLM runtime
adopted:
  container_name_pattern: '^ollama(-.+)?$'
  image_pattern: '^ollama/ollama:'
";

    /// A world the tests script: it answers `list_running` from a shared
    /// cell and RECORDS every operating call. The record must stay empty —
    /// that is the trust rule, tested.
    struct ScriptedWorld {
        facts: Arc<parking_lot::Mutex<Vec<ContainerFact>>>,
        operated: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Runtime for ScriptedWorld {
        fn kind(&self) -> &'static str {
            "scripted"
        }

        async fn place(&self, _name: &str, _spec: &WorkloadSpec) -> Result<Placement, RuntimeError> {
            self.operated.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Unsupported("the detection domain never places"))
        }

        async fn start(&self, _name: &str) -> Result<(), RuntimeError> {
            self.operated.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Unsupported("the detection domain never starts"))
        }

        async fn stop(&self, _name: &str) -> Result<(), RuntimeError> {
            self.operated.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Unsupported("the detection domain never stops"))
        }

        async fn remove(&self, _name: &str) -> Result<(), RuntimeError> {
            self.operated.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeError::Unsupported("the detection domain never removes"))
        }

        async fn observe(&self, _name: &str) -> Option<Observed> {
            None
        }

        async fn list(&self) -> Vec<PlacedRef> {
            Vec::new()
        }

        async fn list_running(&self) -> Vec<ContainerFact> {
            self.facts.lock().clone()
        }
    }

    struct Rig {
        service: OfferingService,
        facts: Arc<parking_lot::Mutex<Vec<ContainerFact>>>,
        operated: Arc<AtomicUsize>,
        _root: std::path::PathBuf,
    }

    fn rig(catalog_docs: &'static [(&'static str, &'static str)]) -> Rig {
        let catalog = Catalog::embedded(catalog_docs.iter().copied()).unwrap();
        let facts = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let operated = Arc::new(AtomicUsize::new(0));
        let world = Arc::new(ScriptedWorld {
            facts: Arc::clone(&facts),
            operated: Arc::clone(&operated),
        });
        let root = std::env::temp_dir()
            .join(format!("moss-detect-{}-{}", std::process::id(), uuid::Uuid::now_v7()));
        let service = OfferingService::new(
            Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default()))),
            Arc::new(RuntimeRegistry::build(vec![world])),
            "scripted".into(),
            Arc::new(catalog),
            Arc::new(Factsheet::empty()),
            OfferingsRoot::new(root.clone()),
            Pool::default(),
            None,
        );
        Rig { service, facts, operated, _root: root }
    }

    fn ollama_running() -> ContainerFact {
        ContainerFact {
            name: "ollama".into(),
            image: "ollama/ollama:latest".into(),
            state: "running".into(),
            host_ports: vec![11434],
        }
    }

    /// The one-minute skeptic demo, in-process: hand-run container →
    /// adopted and visible; kill → stopped, record kept; restart →
    /// running. The world's operating methods are NEVER called.
    #[tokio::test]
    async fn hand_run_container_is_adopted_then_honest_through_death_and_return() {
        let rig = rig(&[("ollama", OLLAMA)]);
        rig.facts.lock().push(ollama_running());

        // Sweep 1: minted, confirmed, running — all in one sweep.
        let report = detect_once(&rig.service).await;
        assert_eq!(report.minted, vec!["ollama::adopted".to_string()]);
        assert_eq!(report.confirmed, vec!["ollama::adopted".to_string()]);
        let adopted = rig.service.placed("ollama::adopted").unwrap();
        assert_eq!(adopted.status, Status::Running);
        assert_eq!(adopted.adopted().unwrap().container_name, "ollama");
        assert_eq!(adopted.category, "ai");

        // Sweep 2: the container is killed. The record reports stopped —
        // it does NOT vanish, and nothing is restarted.
        rig.facts.lock().clear();
        let report = detect_once(&rig.service).await;
        assert_eq!(report.observed, vec!["ollama::adopted → stopped".to_string()]);
        let adopted = rig.service.placed("ollama::adopted").unwrap();
        assert_eq!(adopted.status, Status::Stopped);

        // Sweep 3: it comes back by its owner's hand. Running again.
        rig.facts.lock().push(ollama_running());
        let report = detect_once(&rig.service).await;
        assert_eq!(report.observed, vec!["ollama::adopted → running".to_string()]);
        assert_eq!(rig.service.placed("ollama::adopted").unwrap().status, Status::Running);

        assert_eq!(
            rig.operated.load(Ordering::SeqCst),
            0,
            "adoption observes, it never operates"
        );
    }

    /// A matching container whose stem already has a home here is nobody's
    /// discovery (R1.1): the garden's own ollama stays the only ollama.
    #[tokio::test]
    async fn claimed_stems_are_never_re_minted() {
        let rig = rig(&[("ollama", OLLAMA)]);
        rig.facts.lock().push(ollama_running());

        let first = detect_once(&rig.service).await;
        assert_eq!(first.minted.len(), 1);

        // A second ollama container (same stem) appears: no new identity.
        rig.facts
            .lock()
            .push(ContainerFact {
                name: "ollama-2".into(),
                image: "ollama/ollama:latest".into(),
                state: "running".into(),
                host_ports: vec![],
            });
        let second = detect_once(&rig.service).await;
        assert!(second.minted.is_empty(), "stem already claimed");
        assert_eq!(rig.service.registry().snapshot().len(), 1);
    }

    /// A workload that never matches stays invisible; a candidate whose
    /// container is gone waits as a ghost — invisible until its container
    /// exists again (OFFERINGS.md §2, boot rehydration path).
    #[tokio::test]
    async fn ghosts_wait_for_detection_and_confirm_on_presence() {
        // Simulate a restart: a persisted snapshot carries an adopted
        // record in the ACTIVE pool. Registry::new must split it into
        // candidates (ghost prevention) — it must not haunt the room.
        let store = MemorySnapshotStore::default();
        let now = chrono::Utc::now();
        let ghost = Offering {
            offering_id: "ghost-1".into(),
            name: "ollama::adopted".into(),
            offering: "ollama".into(),
            category: "ai".into(),
            status: Status::Running,
            location: Location { host: "localhost".into(), port: 0, protocol: "http".into() },
            sub_capabilities: Default::default(),
            mode_data: ModeData::Adopted(AdoptedData {
                control_level: "monitor".into(),
                start_command: None,
                stop_command: None,
                health_path: None,
                container_name: "ollama".into(),
            }),
            registered_at: now,
            updated_at: now,
        };
        store.save(&Snapshot { active: vec![ghost], candidates: vec![] });
        let registry = Arc::new(Registry::new(Arc::new(store)));
        assert_eq!(registry.snapshot().len(), 0, "ghost invisible at boot");
        assert_eq!(registry.candidate_count(), 1);

        // Its container is gone: the sweep confirms nothing.
        let catalog = Catalog::embedded([("ollama", OLLAMA)]).unwrap();
        let facts = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let world = Arc::new(ScriptedWorld {
            facts: Arc::clone(&facts),
            operated: Arc::new(AtomicUsize::new(0)),
        });
        let root = std::env::temp_dir().join(format!("moss-ghost-{}", uuid::Uuid::now_v7()));
        let service = OfferingService::new(
            Arc::clone(&registry),
            Arc::new(RuntimeRegistry::build(vec![world])),
            "scripted".into(),
            Arc::new(catalog),
            Arc::new(Factsheet::empty()),
            OfferingsRoot::new(root),
            Pool::default(),
            None,
        );
        let report = detect_once(&service).await;
        assert!(report.is_empty());
        assert_eq!(service.registry().snapshot().len(), 0, "still a ghost");

        // The container exists again — even stopped: presence confirms,
        // and the record tells the truth about the state it found.
        facts.lock().push(ContainerFact {
            name: "ollama".into(),
            image: "ollama/ollama:latest".into(),
            state: "exited".into(),
            host_ports: vec![],
        });
        let report = detect_once(&service).await;
        assert_eq!(report.confirmed, vec!["ollama::adopted".to_string()]);
        let confirmed = service.placed("ollama::adopted").unwrap();
        assert_eq!(confirmed.status, Status::Stopped, "confirmed carrying OBSERVED status");
    }

    /// A rule with only an image pattern (no name rule) still detects.
    #[tokio::test]
    async fn image_only_rules_detect() {
        const DOC: &str = "\
kind: software
name: ollama
category: ai
description: Local LLM runtime
adopted:
  image_pattern: '^ollama/ollama:'
";
        let rig = rig(&[("ollama", DOC)]);
        rig.facts.lock().push(ContainerFact {
            name: "someone-elses-llm".into(),
            image: "ollama/ollama:0.5".into(),
            state: "running".into(),
            host_ports: vec![],
        });
        let report = detect_once(&rig.service).await;
        assert_eq!(report.minted, vec!["ollama::adopted".to_string()]);
        assert_eq!(
            rig.service.placed("ollama::adopted").unwrap().adopted().unwrap().container_name,
            "someone-elses-llm"
        );
    }

    /// Non-running containers are not discoveries: detection recognizes
    /// what LIVES on the host; dead containers are nobody's offering.
    #[tokio::test]
    async fn dead_containers_are_not_discoveries() {
        let rig = rig(&[("ollama", OLLAMA)]);
        rig.facts.lock().push(ContainerFact {
            name: "ollama".into(),
            image: "ollama/ollama:latest".into(),
            state: "exited".into(),
            host_ports: vec![],
        });
        let report = detect_once(&rig.service).await;
        assert!(report.is_empty());
        assert!(rig.service.placed("ollama::adopted").is_none());
    }
}
