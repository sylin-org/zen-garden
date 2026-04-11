//! Offerings aggregate — the DDD root that owns active and candidate pools.
//!
//! The aggregate enforces the persist + emit invariant on every mutation by
//! funneling all writes through private lock-scoped blocks followed by a
//! shared `finalize` step. There is no public accessor for a raw `.write()`
//! handle on the inner state — mutation is impossible except through the
//! methods on this type.
//!
//! **Read access** is currently provided in three shapes:
//! 1. `read()` — back-compat shim returning `ActiveGuard`, which derefs to
//!    `&Vec<Offering>`. This keeps the 82 existing `.read().await` call
//!    sites compiling while they migrate opportunistically.
//! 2. `snapshot()`, `find_by_id()`, `find_by_name()`, `candidates_snapshot()`
//!    — typed query methods preferred for new code.
//! 3. `with_active()`, `with_candidates()` — scoped closures for hot paths
//!    that want to iterate without cloning.
//!
//! See [ARCH-0016](../../../../../docs/decisions/ARCH-0016-offerings-aggregate-domain.md)
//! for full rationale.

use super::event::{ChangeKind, OfferingsChanged};
use super::guard::{ActiveGuard, CandidatesGuard};
use super::store::OfferingStore;
use garden_common::{Offering, OfferingStatus, ServiceHealthStatus};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Internal state of the aggregate — active pool and adopted-candidates pool.
///
/// Kept behind a single `RwLock` on `Offerings` so that cross-collection
/// moves (promote / demote) are atomic.
pub(super) struct OfferingsState {
    pub(super) active: Vec<Offering>,
    pub(super) candidates: Vec<Offering>,
}

impl OfferingsState {
    fn snapshot_all(&self) -> Vec<Offering> {
        let mut all = Vec::with_capacity(self.active.len() + self.candidates.len());
        all.extend(self.active.iter().cloned());
        all.extend(self.candidates.iter().cloned());
        all
    }
}

/// The Offerings aggregate — owns active + candidates, persists, emits events.
pub struct Offerings {
    state: RwLock<OfferingsState>,
    store: Arc<dyn OfferingStore>,
    changes: broadcast::Sender<OfferingsChanged>,
}

impl Offerings {
    /// Construct an aggregate from a pre-loaded offering set.
    ///
    /// Called at bootstrap after `FileOfferingStore::load()` returns the
    /// persisted set. The loader splits by `is_adopted()` — adopted offerings
    /// go to candidates (pending detection), the rest go to active.
    pub fn new(
        active: Vec<Offering>,
        candidates: Vec<Offering>,
        store: Arc<dyn OfferingStore>,
    ) -> Self {
        let (changes, _) = broadcast::channel(garden_common::constants::channels::OFFERINGS_EVENT);
        Self {
            state: RwLock::new(OfferingsState { active, candidates }),
            store,
            changes,
        }
    }

    /// Split a merged offering set into (active, candidates) by adoption mode.
    ///
    /// Adopted offerings start as candidates and must pass detection before
    /// being promoted to active. Everything else (managed, borrowed, and
    /// detection-confirmed adopted that wasn't just reloaded) goes to active.
    pub fn split_loaded(all: Vec<Offering>) -> (Vec<Offering>, Vec<Offering>) {
        let mut active = Vec::new();
        let mut candidates = Vec::new();
        for offering in all {
            if offering.is_adopted() {
                candidates.push(offering);
            } else {
                active.push(offering);
            }
        }
        (active, candidates)
    }

    // ========================================================================
    // Event subscription
    // ========================================================================

    /// Subscribe to aggregate-level mutation events.
    ///
    /// Subscribers receive an `OfferingsChanged` every time a mutation
    /// through this aggregate completes successfully. Lagged receivers
    /// should reconcile by calling `snapshot()` and rebuilding their
    /// projection rather than breaking the stream.
    pub fn changes(&self) -> broadcast::Receiver<OfferingsChanged> {
        self.changes.subscribe()
    }

    // ========================================================================
    // Read API — snapshots and queries
    // ========================================================================

    /// Back-compat read guard for the active pool.
    ///
    /// Derefs to `&Vec<Offering>` so existing call sites that do
    /// `state.offerings.read().await.iter()...` compile unchanged.
    /// New code should prefer `snapshot()`, `find_by_id()`, or `with_active()`.
    pub async fn read(&self) -> ActiveGuard<'_> {
        ActiveGuard {
            inner: self.state.read().await,
        }
    }

    /// Read guard for the adopted-candidates pool.
    pub async fn read_candidates(&self) -> CandidatesGuard<'_> {
        CandidatesGuard {
            inner: self.state.read().await,
        }
    }

    /// Clone of the active offerings pool.
    pub async fn snapshot(&self) -> Vec<Offering> {
        self.state.read().await.active.clone()
    }

    /// Clone of the adopted-candidates pool.
    pub async fn candidates_snapshot(&self) -> Vec<Offering> {
        self.state.read().await.candidates.clone()
    }

    /// Find an active offering by `offering_id`.
    pub async fn find_by_id(&self, offering_id: &str) -> Option<Offering> {
        self.state
            .read()
            .await
            .active
            .iter()
            .find(|o| o.offering_id == offering_id)
            .cloned()
    }

    /// Find an active offering by FQN.
    pub async fn find_by_name(&self, name: &str) -> Option<Offering> {
        self.state
            .read()
            .await
            .active
            .iter()
            .find(|o| o.name.fqn_eq(name))
            .cloned()
    }

    /// Scoped borrow of the active pool. The closure runs inside the read
    /// lock and must not await — use `snapshot()` if you need to escape the
    /// lock scope.
    pub async fn with_active<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Offering]) -> R,
    {
        let st = self.state.read().await;
        f(&st.active)
    }

    /// Scoped borrow of the adopted-candidates pool.
    pub async fn with_candidates<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Offering]) -> R,
    {
        let st = self.state.read().await;
        f(&st.candidates)
    }

    /// Number of active offerings.
    pub async fn count_active(&self) -> usize {
        self.state.read().await.active.len()
    }

    // ========================================================================
    // Mutation API — the only way to change state
    // ========================================================================

    /// Insert or update an offering in the active pool.
    ///
    /// Dedup guard: matches by `offering_id` first, then by FQN.
    pub async fn upsert(&self, mut offering: Offering) {
        offering.touch();
        let id = offering.offering_id.clone();

        let all = {
            let mut st = self.state.write().await;
            if let Some(pos) = st
                .active
                .iter()
                .position(|o| o.offering_id == offering.offering_id)
            {
                st.active[pos] = offering;
            } else if let Some(pos) = st.active.iter().position(|o| o.name == offering.name) {
                tracing::info!(
                    name = %offering.name,
                    old_id = %st.active[pos].offering_id,
                    new_id = %offering.offering_id,
                    "upsert: FQN already exists, updating in place"
                );
                st.active[pos] = offering;
            } else {
                st.active.push(offering);
            }
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Upserted, vec![id]).await;
    }

    /// Remove an offering from the active pool by ID.
    pub async fn remove(&self, offering_id: &str) -> bool {
        let all = {
            let mut st = self.state.write().await;
            let before = st.active.len();
            st.active.retain(|o| o.offering_id != offering_id);
            if st.active.len() == before {
                return false;
            }
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Removed, vec![offering_id.to_string()])
            .await;
        true
    }

    /// Remove an offering from the active pool by FQN.
    pub async fn remove_by_name(&self, name: &str) -> bool {
        let (all, affected) = {
            let mut st = self.state.write().await;
            let affected: Vec<String> = st
                .active
                .iter()
                .filter(|o| o.name.fqn_eq(name))
                .map(|o| o.offering_id.clone())
                .collect();
            if affected.is_empty() {
                return false;
            }
            st.active.retain(|o| !o.name.fqn_eq(name));
            (st.snapshot_all(), affected)
        };

        self.finalize(all, ChangeKind::Removed, affected).await;
        true
    }

    /// Update an adopted-candidate offering in place by ID.
    ///
    /// Used by auto-adoption to fix metadata (e.g., port) on a candidate
    /// before promoting it. Returns whether the closure reported a change.
    pub async fn update_candidate<F>(&self, offering_id: &str, mutator: F) -> bool
    where
        F: FnOnce(&mut Offering) -> bool,
    {
        let all = {
            let mut st = self.state.write().await;
            let changed = st
                .candidates
                .iter_mut()
                .find(|o| o.offering_id == offering_id)
                .map(mutator)
                .unwrap_or(false);
            if !changed {
                return false;
            }
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Updated, vec![offering_id.to_string()])
            .await;
        true
    }

    /// Update an offering in place by ID. Returns whether the closure reported
    /// a change (and persistence + emit therefore ran).
    pub async fn update<F>(&self, offering_id: &str, mutator: F) -> bool
    where
        F: FnOnce(&mut Offering) -> bool,
    {
        let all = {
            let mut st = self.state.write().await;
            let changed = st
                .active
                .iter_mut()
                .find(|o| o.offering_id == offering_id)
                .map(mutator)
                .unwrap_or(false);
            if !changed {
                return false;
            }
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Updated, vec![offering_id.to_string()])
            .await;
        true
    }

    /// Update an offering in place by FQN.
    pub async fn update_by_name<F>(&self, name: &str, mutator: F) -> bool
    where
        F: FnOnce(&mut Offering) -> bool,
    {
        let (all, offering_id) = {
            let mut st = self.state.write().await;
            let Some(o) = st.active.iter_mut().find(|o| o.name.fqn_eq(name)) else {
                return false;
            };
            let id = o.offering_id.clone();
            if !mutator(o) {
                return false;
            }
            (st.snapshot_all(), id)
        };

        self.finalize(all, ChangeKind::Updated, vec![offering_id])
            .await;
        true
    }

    /// Batch-update the active pool via a closure over the whole vec.
    /// Returns the count of offerings changed as reported by the closure.
    pub async fn update_batch<F>(&self, mutator: F) -> usize
    where
        F: FnOnce(&mut Vec<Offering>) -> usize,
    {
        let (count, all) = {
            let mut st = self.state.write().await;
            let count = mutator(&mut st.active);
            if count == 0 {
                return 0;
            }
            (count, st.snapshot_all())
        };

        self.finalize(all, ChangeKind::BatchUpdated, Vec::new())
            .await;
        count
    }

    /// Replace the entire active pool with a new set.
    pub async fn replace_active(&self, new_active: Vec<Offering>) {
        let affected: Vec<String> = new_active.iter().map(|o| o.offering_id.clone()).collect();
        let all = {
            let mut st = self.state.write().await;
            st.active = new_active;
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Replaced, affected).await;
    }

    /// Coalesce duplicate active offerings by FQN, keeping the most recent.
    /// Returns the number of duplicates removed.
    pub async fn coalesce_duplicates(&self) -> usize {
        let (removed, all) = {
            let mut st = self.state.write().await;
            let before = st.active.len();

            let mut best: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (i, o) in st.active.iter().enumerate() {
                let key = o.name.to_string();
                let dominated = best.get(&key).is_some_and(|&prev| {
                    let prev_ts = st.active[prev]
                        .updated_at
                        .unwrap_or(st.active[prev].registered_at);
                    let cur_ts = o.updated_at.unwrap_or(o.registered_at);
                    cur_ts <= prev_ts
                });
                if !dominated {
                    best.insert(key, i);
                }
            }

            let keep: std::collections::HashSet<usize> = best.into_values().collect();
            let mut idx = 0usize;
            st.active.retain(|_| {
                let k = keep.contains(&idx);
                idx += 1;
                k
            });

            let removed = before - st.active.len();
            if removed == 0 {
                return 0;
            }
            tracing::warn!(removed, "Coalesced duplicate offerings by FQN");
            (removed, st.snapshot_all())
        };

        self.finalize(all, ChangeKind::Coalesced, Vec::new()).await;
        removed
    }

    /// Promote an adopted candidate to the active pool.
    ///
    /// This is the fix for the bug described in ARCH-0016: the old
    /// `AppState::promote_adopted` bypassed the mutation gateway and the
    /// tool registry projection never saw the adopted offering.
    pub async fn promote(&self, offering_id: &str) -> bool {
        let all = {
            let mut st = self.state.write().await;
            let Some(idx) = st
                .candidates
                .iter()
                .position(|o| o.offering_id == offering_id)
            else {
                return false;
            };

            let mut offering = st.candidates.remove(idx);
            offering.status = OfferingStatus::Running;
            offering.health = ServiceHealthStatus::Healthy;
            let name = offering.offering.clone();
            st.active.push(offering);
            tracing::info!(offering = %name, "Promoted adopted candidate to active pool");
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Promoted, vec![offering_id.to_string()])
            .await;
        true
    }

    /// Demote an adopted offering back to candidates.
    ///
    /// Fix for the symmetric bug to `promote` — demotion also has to persist
    /// and publish so the tool registry drops the offering immediately.
    pub async fn demote(&self, offering_id: &str) -> bool {
        let all = {
            let mut st = self.state.write().await;
            let Some(idx) = st
                .active
                .iter()
                .position(|o| o.offering_id == offering_id && o.is_adopted())
            else {
                return false;
            };

            let mut offering = st.active.remove(idx);
            offering.status = OfferingStatus::Stopped;
            offering.health = ServiceHealthStatus::Offline;
            let name = offering.offering.clone();
            st.candidates.push(offering);
            tracing::info!(offering = %name, "Demoted adopted offering back to candidates");
            st.snapshot_all()
        };

        self.finalize(all, ChangeKind::Demoted, vec![offering_id.to_string()])
            .await;
        true
    }

    // ========================================================================
    // Finalize — persist + emit (private, called after every mutation)
    // ========================================================================

    /// Persist the full merged set and emit an `OfferingsChanged` event.
    ///
    /// Called by every mutation method after the write lock has been released.
    /// A persistence failure is logged but does not suppress the event — the
    /// in-memory state already reflects the change, and the projection task
    /// must still see it to keep consumers coherent.
    async fn finalize(&self, all: Vec<Offering>, kind: ChangeKind, affected: Vec<String>) {
        if let Err(e) = self.store.save(&all).await {
            tracing::error!(
                kind = ?kind,
                error = ?e,
                "Failed to persist offerings after mutation",
            );
        }

        let event = OfferingsChanged::new(kind, affected);
        // Send errors are ignored: broadcast::send returns Err only when
        // there are no receivers, which is fine — the projection task may
        // not have spawned yet at boot.
        let _ = self.changes.send(event);
    }
}
