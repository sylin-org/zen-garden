// Event fan-out and OfferingChanged fields light up with O1's lifecycle
// commands; the registry shape is pinned now by tests.
#![allow(dead_code)]

//! The registry: one hot map of this stone's offerings (OFFERINGS.md §2).
//!
//! Active pool + candidates pool behind one lock; every mutation funnels
//! persist → broadcast. Adopted offerings load into *candidates* — invisible
//! until their detector confirms them again (ghost prevention, poc
//! run.rs:806-858). Readers snapshot; nobody polls anyone (L18/L22).

use super::model::{Mode, ModeData, Offering};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Something about an offering changed. The domain event for the garden.
#[derive(Debug, Clone)]
pub struct OfferingChanged {
    pub offering_id: String,
    pub name: String,
    /// The full post-change snapshot when present; None = removed.
    pub offering: Option<Offering>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    active: Vec<Offering>,
    candidates: Vec<Offering>,
}

/// One stone's offerings. Clone freely; all clones share state.
pub struct Registry {
    inner: Arc<parking_lot::RwLock<Inner>>,
    store_path: Arc<std::path::PathBuf>,
    events_tx: broadcast::Sender<OfferingChanged>,
}

struct Inner {
    active: HashMap<String, Offering>,
    candidates: Vec<Offering>,
}

impl Registry {
    /// Load from `path`, splitting adopted into candidates (ghost
    /// prevention); missing file = empty garden.
    pub fn load(path: std::path::PathBuf) -> Self {
        let (active, candidates) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<RegistryFile>(&bytes) {
                Ok(file) => (file.active, file.candidates),
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "offerings file unreadable; starting empty");
                    (Vec::new(), Vec::new())
                }
            },
            Err(_) => (Vec::new(), Vec::new()),
        };
        let events_tx = broadcast::channel(256).0;
        let registry = Self {
            inner: Arc::new(parking_lot::RwLock::new(Inner {
                active: active.into_iter().map(|o| (o.offering_id.clone(), o)).collect(),
                candidates,
            })),
            store_path: Arc::new(path),
            events_tx,
        };
        registry.persist();
        registry
    }

    pub fn events(&self) -> broadcast::Receiver<OfferingChanged> {
        self.events_tx.subscribe()
    }

    pub fn get(&self, offering_id: &str) -> Option<Offering> {
        self.inner.read().active.get(offering_id).cloned()
    }

    pub fn get_by_name(&self, name: &str) -> Option<Offering> {
        self.inner.read().active.values().find(|o| o.name == name).cloned()
    }

    /// Move an active offering to a new status (rest/wake/degrade...).
    pub fn set_status(&self, offering_id: &str, status: super::model::Status) -> bool {
        let event = {
            let mut inner = self.inner.write();
            let Some(o) = inner.active.get_mut(offering_id) else { return false };
            if o.status == status {
                return true;
            }
            o.status = status;
            o.updated_at = chrono::Utc::now();
            Some(OfferingChanged {
                offering_id: o.offering_id.clone(),
                name: o.name.clone(),
                offering: Some(o.clone()),
            })
        };
        if let Some(event) = &event {
            self.persist();
            let _ = self.events_tx.send(event.clone());
        }
        event.is_some()
    }

    /// Snapshot of the active pool, sorted by name.
    pub fn snapshot(&self) -> Vec<Offering> {
        let mut all: Vec<Offering> = self.inner.read().active.values().cloned().collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }

    pub fn candidate_count(&self) -> usize {
        self.inner.read().candidates.len()
    }

    /// Insert or replace an ACTIVE offering by id. Adopted offerings enter
    /// as candidates instead — use [`promote`] once detection confirms them.
    pub fn upsert(&self, mut offering: Offering) {
        if matches!(offering.mode_data, ModeData::Adopted(_)) && !self.is_known(&offering.offering_id) {
            self.upsert_candidate(offering);
            return;
        }
        offering.updated_at = chrono::Utc::now();
        let event = {
            let mut inner = self.inner.write();
            let name = offering.name.clone();
            let id = offering.offering_id.clone();
            inner.active.insert(id.clone(), offering.clone());
            OfferingChanged { offering_id: id, name, offering: Some(offering) }
        };
        self.persist();
        let _ = self.events_tx.send(event);
    }

    /// Awaiting confirmation: stored but invisible to chirps and reconcile.
    pub fn upsert_candidate(&self, offering: Offering) {
        let mut inner = self.inner.write();
        inner.candidates.retain(|c| c.offering_id != offering.offering_id);
        inner.candidates.push(offering);
        // Candidates are memory-only until promoted (poc aggregate.rs:406+).
    }

    /// Detection confirmed: move a candidate into the active pool, Running.
    pub fn promote(&self, offering_id: &str) -> Option<Offering> {
        let event = {
            let mut inner = self.inner.write();
            let idx = inner.candidates.iter().position(|c| c.offering_id == offering_id)?;
            let mut o = inner.candidates.remove(idx);
            o.status = super::model::Status::Running;
            o.updated_at = chrono::Utc::now();
            let event = OfferingChanged {
                offering_id: o.offering_id.clone(),
                name: o.name.clone(),
                offering: Some(o.clone()),
            };
            inner.active.insert(o.offering_id.clone(), o);
            event
        };
        self.persist();
        let _ = self.events_tx.send(event);
        self.get(offering_id)
    }

    /// Detection went silent: demote back to candidates (ghost prevention).
    pub fn demote(&self, offering_id: &str) -> bool {
        let moved = {
            let mut inner = self.inner.write();
            if let Some(mut o) = inner.active.remove(offering_id) {
                o.status = super::model::Status::Stopped;
                o.updated_at = chrono::Utc::now();
                inner.candidates.push(o);
                true
            } else {
                false
            }
        };
        if moved {
            self.persist();
            if let Some(o) = self.get(offering_id) {
                let _ = self.events_tx.send(OfferingChanged {
                    offering_id: o.offering_id.clone(),
                    name: o.name.clone(),
                    offering: None,
                });
            }
        }
        moved
    }

    pub fn remove(&self, offering_id: &str) -> bool {
        let event = {
            let mut inner = self.inner.write();
            if let Some(o) = inner.active.remove(offering_id) {
                Some(OfferingChanged { offering_id: o.offering_id.clone(), name: o.name.clone(), offering: None })
            } else {
                None
            }
        };
        if let Some(event) = &event {
            self.persist();
            let _ = self.events_tx.send(event.clone());
        }
        event.is_some()
    }

    fn is_known(&self, offering_id: &str) -> bool {
        let inner = self.inner.read();
        inner.active.contains_key(offering_id)
            || inner.candidates.iter().any(|c| c.offering_id == offering_id)
    }

    fn persist(&self) {
        let file = {
            let inner = self.inner.read();
            RegistryFile {
                active: inner.active.values().cloned().collect(),
                candidates: inner.candidates.clone(),
            }
        };
        match serde_json::to_vec_pretty(&file) {
            Ok(bytes) => {
                let tmp = self.store_path.with_extension("json.tmp");
                if let Err(e) = std::fs::write(&tmp, &bytes)
                    .and_then(|_| std::fs::rename(&tmp, &*self.store_path))
                {
                    tracing::warn!(error = %e, "offerings persist failed");
                }
            }
            Err(e) => tracing::warn!(error = %e, "offerings encode failed"),
        }
    }
}

/// Which pool an offering landed in after load — test/inspection helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    Active,
    Candidate,
}

impl Registry {
    pub fn pool_of(&self, offering_id: &str) -> Option<Pool> {
        let inner = self.inner.read();
        if inner.active.contains_key(offering_id) {
            Some(Pool::Active)
        } else if inner.candidates.iter().any(|c| c.offering_id == offering_id) {
            Some(Pool::Candidate)
        } else {
            None
        }
    }

    /// Modes present in the active pool (posture/tests).
    pub fn modes_present(&self) -> Vec<Mode> {
        let mut modes: Vec<Mode> =
            self.inner.read().active.values().map(|o| o.mode()).collect();
        modes.sort_by_key(|m| m.as_str());
        modes.dedup();
        modes
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::offerings::model::{AdoptedData, Location, ManagedData, Status};

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "moss-registry-test-{}-{tag}.json",
            std::process::id()
        ))
    }

    fn managed(name: &str) -> Offering {
        Offering {
            offering_id: format!("id-{name}"),
            name: name.into(),
            offering: name.into(),
            category: "data".into(),
            status: Status::Running,
            location: Location { host: "localhost".into(), port: 27017, protocol: "http".into() },
            mode_data: ModeData::Managed(ManagedData::default()),
            registered_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn adopted(name: &str) -> Offering {
        Offering {
            offering_id: format!("id-{name}"),
            name: format!("{name}::adopted"),
            offering: name.into(),
            category: "ai".into(),
            status: Status::Stopped,
            location: Location { host: "localhost".into(), port: 11434, protocol: "http".into() },
            mode_data: ModeData::Adopted(AdoptedData {
                control_level: garden_glossary::offering::control::MONITOR.into(),
                start_command: None,
                stop_command: None,
                health_path: Some("/".into()),
            }),
            registered_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// OFFERINGS.md §2 ghost prevention: adopted loads to candidates,
    /// invisible until promoted; a hand-stopped service must not haunt.
    #[test]
    fn adopted_loads_as_candidate_and_promotes_on_detection() {
        let path = temp_path("ghost");
        let reg = Registry::load(path.clone());
        reg.upsert(adopted("ollama"));
        assert_eq!(reg.pool_of("id-ollama"), Some(Pool::Candidate));
        assert!(reg.snapshot().is_empty(), "candidates are invisible");

        let seen = reg.promote("id-ollama").unwrap();
        assert_eq!(seen.status, Status::Running);
        assert_eq!(reg.pool_of("id-ollama"), Some(Pool::Active));

        // Detection silence demotes; it does not delete.
        assert!(reg.demote("id-ollama"));
        assert_eq!(reg.pool_of("id-ollama"), Some(Pool::Candidate));

        // Persistence roundtrip keeps the split honest.
        let reloaded = Registry::load(path);
        assert_eq!(reloaded.pool_of("id-ollama"), Some(Pool::Candidate));
    }

    #[test]
    fn managed_upsert_persists_and_removes() {
        let path = temp_path("managed");
        let reg = Registry::load(path.clone());
        reg.upsert(managed("mongodb"));
        assert_eq!(reg.snapshot().len(), 1);
        assert_eq!(reg.modes_present(), vec![Mode::Managed]);

        let reloaded = Registry::load(path);
        assert_eq!(reloaded.snapshot()[0].name, "mongodb");
        assert!(reloaded.remove("id-mongodb"));
        assert!(Registry::load(temp_path("managed")).snapshot().is_empty());
    }
}
