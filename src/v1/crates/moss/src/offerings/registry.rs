// O2/O3 consume the dormant surface (events fan-out, promote/demote from
// detectors, posture counts) — OFFERINGS.md §5. Trim as wiring lands.
#![allow(dead_code)]

//! The registry: the Offering aggregate's home (OFFERINGS.md §2).
//!
//! Two pools — active and candidates — behind one lock; every mutation
//! funnels facts → persist (via the store port) → broadcast. Adopted
//! offerings enter as *candidates* and stay invisible until promoted
//! (ghost prevention, poc run.rs:806-858). Readers snapshot; nobody polls
//! anyone (L18/L22). No I/O lives here: persistence is a port.

use super::model::{ModeData, Offering, Status};
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

/// The aggregate's persisted shape.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub active: Vec<Offering>,
    pub candidates: Vec<Offering>,
}

/// Port: where the aggregate's memory survives restarts. Implementations:
/// the file store (adapters) and an in-memory fake in tests.
pub trait SnapshotStore: Send + Sync {
    fn load(&self) -> Option<Snapshot>;
    fn save(&self, snapshot: &Snapshot);
}

/// In-memory store — for tests and embedded uses; nothing survives the
/// process.
#[derive(Default)]
pub struct MemorySnapshotStore {
    cell: parking_lot::Mutex<Option<Snapshot>>,
}

impl SnapshotStore for MemorySnapshotStore {
    fn load(&self) -> Option<Snapshot> {
        self.cell.lock().clone()
    }

    fn save(&self, snapshot: &Snapshot) {
        *self.cell.lock() = Some(snapshot.clone());
    }
}

/// File-backed store: pretty JSON, atomic temp+rename (poc parity).
pub struct FileSnapshotStore {
    path: std::path::PathBuf,
}

impl FileSnapshotStore {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl SnapshotStore for FileSnapshotStore {
    fn load(&self) -> Option<Snapshot> {
        let bytes = std::fs::read(&self.path).ok()?;
        match serde_json::from_slice(&bytes) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(path = %self.path.display(), error = %e, "offerings file unreadable; starting empty");
                None
            }
        }
    }

    fn save(&self, snapshot: &Snapshot) {
        if let Some(parent) = self.path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(error = %e, "offerings dir create failed");
            return;
        }
        let tmp = self.path.with_extension("json.tmp");
        let write = serde_json::to_vec_pretty(snapshot)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(&tmp, bytes).map_err(|e| e.to_string()))
            .and_then(|_| std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string()));
        if let Err(e) = write {
            tracing::warn!(error = %e, "offerings persist failed");
        }
    }
}

struct Inner {
    active: HashMap<String, Offering>,
    candidates: Vec<Offering>,
}

/// One stone's offerings. Clone freely; all clones share state.
pub struct Registry {
    inner: Arc<parking_lot::RwLock<Inner>>,
    store: Arc<dyn SnapshotStore>,
    events_tx: broadcast::Sender<OfferingChanged>,
}

impl Registry {
    /// Build from a store, splitting adopted into candidates on load
    /// (ghost prevention).
    pub fn new(store: Arc<dyn SnapshotStore>) -> Self {
        let (active, candidates) = match store.load() {
            Some(s) => (s.active, s.candidates),
            None => (Vec::new(), Vec::new()),
        };
        let events_tx = broadcast::channel(256).0;
        Self {
            inner: Arc::new(parking_lot::RwLock::new(Inner {
                active: active.into_iter().map(|o| (o.offering_id.clone(), o)).collect(),
                candidates,
            })),
            store,
            events_tx,
        }
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

    /// Snapshot of the active pool, sorted by name.
    pub fn snapshot(&self) -> Vec<Offering> {
        let mut all: Vec<Offering> = self.inner.read().active.values().cloned().collect();
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    }

    pub fn candidate_count(&self) -> usize {
        self.inner.read().candidates.len()
    }

    /// Insert or replace an ACTIVE offering. Adopted offerings entering
    /// fresh are diverted to candidates — call [`promote`] once detection
    /// confirms them.
    pub fn register(&self, mut offering: Offering) {
        if matches!(offering.mode_data, ModeData::Adopted(_)) && !self.is_known(&offering.offering_id)
        {
            self.add_candidate(offering);
            return;
        }
        offering.updated_at = chrono::Utc::now();
        let event = {
            let mut inner = self.inner.write();
            let id = offering.offering_id.clone();
            let name = offering.name.clone();
            inner.active.insert(id.clone(), offering.clone());
            OfferingChanged { offering_id: id, name, offering: Some(offering) }
        };
        self.persist();
        let _ = self.events_tx.send(event);
    }

    /// Awaiting confirmation: stored but invisible to chirps and reconcile.
    pub fn add_candidate(&self, offering: Offering) {
        let mut inner = self.inner.write();
        inner.candidates.retain(|c| c.offering_id != offering.offering_id);
        inner.candidates.push(offering);
        // Candidates stay memory-only until promoted (poc aggregate.rs:406+);
        // they ride along inside the next full persist.
        self.persist();
    }

    /// Detection confirmed: promote a candidate into the active pool.
    pub fn promote(&self, offering_id: &str) -> Option<Offering> {
        let event = {
            let mut inner = self.inner.write();
            let idx =
                inner.candidates.iter().position(|c| c.offering_id == offering_id)?;
            let mut o = inner.candidates.remove(idx);
            o.status = Status::Running;
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
                o.status = Status::Stopped;
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
                    offering_id: o.offering_id,
                    name: o.name,
                    offering: None,
                });
            }
        }
        moved
    }

    pub fn remove(&self, offering_id: &str) -> bool {
        let event = {
            let mut inner = self.inner.write();
            inner.active.remove(offering_id).map(|o| OfferingChanged {
                offering_id: o.offering_id.clone(),
                name: o.name.clone(),
                offering: None,
            })
        };
        if let Some(event) = &event {
            self.persist();
            let _ = self.events_tx.send(event.clone());
        }
        event.is_some()
    }

    /// Move an active offering to a new status (rest/wake/degrade...).
    pub fn mark_status(&self, offering_id: &str, status: Status) -> bool {
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

    /// Replace an active offering wholesale (port-map refresh after wake).
    pub fn replace(&self, offering: Offering) {
        self.register(offering);
    }

    fn is_known(&self, offering_id: &str) -> bool {
        let inner = self.inner.read();
        inner.active.contains_key(offering_id)
            || inner.candidates.iter().any(|c| c.offering_id == offering_id)
    }

    fn persist(&self) {
        let snapshot = {
            let inner = self.inner.read();
            Snapshot {
                active: inner.active.values().cloned().collect(),
                candidates: inner.candidates.clone(),
            }
        };
        self.store.save(&snapshot);
    }
}
