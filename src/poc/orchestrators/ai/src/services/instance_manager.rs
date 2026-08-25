//! Instance pool primitives — shared toolkit for adapters with
//! local instance pools (ORCH-0030 §4, revised).
//!
//! **Scope: primitives only.** This module provides the mechanical
//! pieces every adapter needs: a health-aware registry, per-instance
//! concurrency limits (semaphores), atomic queue-depth counters, and
//! an RAII `Selection` bundle that releases every resource atomically
//! on drop.
//!
//! **Out of scope: selection policy.** Scoring and ranking are
//! adapter-owned, because every adapter has a different definition of
//! "best instance":
//!
//! - Ollama ranks (model, instance) pairs using its capability matrix,
//!   per-model benchmark verdicts, warmth (model loaded in VRAM),
//!   parameter size for quality, and demand-based reservation.
//! - ComfyUI ranks instances by which ones have the requested skill's
//!   checkpoint + LoRAs already cached, plus VRAM headroom.
//! - Cloud adapters have no instance pools at all and never use this
//!   module.
//!
//! The shared library therefore exposes a **toolkit** of primitives
//! (not a `select()` method). Adapters compose:
//!
//! 1. `InstancePool::<I>` to hold their instances keyed by id
//! 2. `ManagedInstance<I>` for per-instance permits + depth counters
//! 3. `HealthFilter` to eliminate unhealthy instances from candidacy
//! 4. `Selection<I>` RAII bundle combining (instance, claim guard,
//!    semaphore permit, depth guard)
//!
//! Adapters write their own `select()` on top, making whatever
//! decisions make sense for their domain. The pool primitives ensure
//! the mechanical bits (release-on-drop, no double-counting) are
//! correct by construction.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, TryAcquireError};

use crate::domain::resources::{ClaimGuard, ComputeStack, StoneName};

// ── Identity ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Health ────────────────────────────────────────────────────

/// Health as observed by the adapter. Values correspond directly to
/// the standalone Ollama orchestrator's `InstanceHealth`:
/// - `Profiling`: discovery probe in progress; not routable yet.
/// - `Healthy`: responding normally; routable.
/// - `Unhealthy`: unreachable or erroring; **removed** from
///   candidacy (not deprioritized).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Profiling,
    Healthy,
    Unhealthy,
}

impl Health {
    pub fn is_routable(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

// ── Capacity ──────────────────────────────────────────────────

/// Per-instance capacity hints the adapter knows about.
///
/// This is a *shape* adapters can reuse for common fields; adapters
/// with richer state (Ollama's model matrix, ComfyUI's skill cache)
/// wrap this in their own struct.
#[derive(Debug, Clone, Serialize)]
pub struct Capacity {
    /// Maximum concurrent in-flight requests for this instance.
    pub max_concurrent: u32,
    /// Typical VRAM footprint in MB. `None` → adapter can't estimate;
    /// claims against this instance's stone will be unsized
    /// (exclusive). Adapters that know their workload should always
    /// provide an estimate (even a conservative one like
    /// `total_vram_mb`).
    pub typical_vram_mb: Option<u64>,
    /// Compute stack this instance requires. Used by the Resources
    /// domain to filter claims against device capabilities.
    pub required_stack: ComputeStack,
}

// ── Instance runtime trait ────────────────────────────────────

/// Adapter-specific runtime handle. Adapters implement this on their
/// concrete instance type; the pool holds trait objects (via
/// `Arc<I: InstanceRuntime>`) and exposes them back to the adapter's
/// selector via `snapshot()`.
#[async_trait]
pub trait InstanceRuntime: Send + Sync + 'static {
    fn id(&self) -> &InstanceId;
    fn stone(&self) -> &StoneName;
    fn capacity(&self) -> &Capacity;
    fn health(&self) -> Health;
}

// ── Managed instance ──────────────────────────────────────────

/// An instance tracked by the pool. Wraps the adapter's runtime
/// handle with a concurrency semaphore and an atomic queue-depth
/// counter that the RAII `Selection` bundle decrements on drop.
pub struct ManagedInstance<I: InstanceRuntime> {
    pub runtime: Arc<I>,
    pub semaphore: Arc<Semaphore>,
    pub queue_depth: Arc<AtomicU32>,
}

impl<I: InstanceRuntime> ManagedInstance<I> {
    /// Current number of in-flight + queued permits not yet released.
    /// This is the sum of live `Selection` bundles for this instance.
    pub fn current_depth(&self) -> u32 {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// How many permits are *immediately available* (i.e., could be
    /// acquired without blocking). Adapters use this to pick idle
    /// instances before busy ones in their ranking functions.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

// ── RAII selection bundle ─────────────────────────────────────

/// The canonical result of a selection: the chosen instance plus
/// every resource that must be released when the work completes.
///
/// `Selection` holds four things:
/// 1. The adapter's runtime handle (`Arc<I>`) — for the caller to
///    dispatch against.
/// 2. A `ClaimGuard` against the Resources domain — released on
///    drop via the Resources domain's claim lifecycle.
/// 3. An `OwnedSemaphorePermit` — returns one concurrency slot to
///    the instance on drop.
/// 4. A `DepthGuard` — decrements the queue-depth counter on drop.
///
/// Dropping the `Selection` releases all four. Adapters that want
/// synchronous release semantics can call `ClaimGuard::release_now`
/// on `sel.claim` before drop, but the default drop path handles
/// the normal case.
pub struct Selection<I: InstanceRuntime> {
    pub instance: Arc<I>,
    pub claim: ClaimGuard,
    pub _permit: OwnedSemaphorePermit,
    pub _depth_guard: DepthGuard,
}

impl<I: InstanceRuntime> std::fmt::Debug for Selection<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Selection")
            .field("instance", self.instance.id())
            .field("claim", &self.claim)
            .finish()
    }
}

/// Decrements an instance's queue depth counter on drop. Paired
/// with the increment that happens when a `Selection` is built.
pub struct DepthGuard {
    counter: Arc<AtomicU32>,
}

impl DepthGuard {
    pub(crate) fn new(counter: Arc<AtomicU32>) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

// ── Acquisition errors ────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AcquireError {
    #[error("instance `{0}` not found in pool")]
    NotFound(String),
    #[error("instance `{0}` is not healthy")]
    NotHealthy(String),
    #[error("instance `{0}` is saturated (no permits available)")]
    Saturated(String),
}

// ── The pool ──────────────────────────────────────────────────

/// A pool of managed instances. Pure registry + primitives; **no
/// scoring or selection policy**. Adapters iterate over `snapshot()`
/// to build their candidate set, rank it with their own logic, and
/// call `try_acquire()` on the winner.
pub struct InstancePool<I: InstanceRuntime> {
    instances: RwLock<HashMap<InstanceId, ManagedInstance<I>>>,
    adapter_name: String,
}

impl<I: InstanceRuntime> InstancePool<I> {
    pub fn new(adapter_name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            instances: RwLock::new(HashMap::new()),
            adapter_name: adapter_name.into(),
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Register a new instance. If one with the same id already
    /// exists, it is replaced (including its permit count and
    /// depth counter — the caller should ensure no live `Selection`
    /// bundles reference the old instance).
    pub async fn register(&self, runtime: Arc<I>) {
        let id = runtime.id().clone();
        let max = runtime.capacity().max_concurrent.max(1) as usize;
        let semaphore = Arc::new(Semaphore::new(max));
        let queue_depth = Arc::new(AtomicU32::new(0));
        let mut state = self.instances.write().await;
        state.insert(
            id,
            ManagedInstance {
                runtime,
                semaphore,
                queue_depth,
            },
        );
    }

    /// Remove an instance from the pool. In-flight `Selection`s that
    /// reference it are unaffected — they still hold their own
    /// permit and depth counter clones — but no new acquisitions
    /// will land on it.
    pub async fn unregister(&self, id: &InstanceId) {
        let mut state = self.instances.write().await;
        state.remove(id);
    }

    pub async fn len(&self) -> usize {
        self.instances.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.instances.read().await.is_empty()
    }

    /// Snapshot the pool's current instances for ranking.
    ///
    /// Returns cloned `Arc<I>` handles plus the semaphore and depth
    /// counter needed to acquire a permit. Adapters walk this list,
    /// filter by health (or any other criteria), score the survivors,
    /// and call `try_acquire_instance()` on their chosen winner.
    pub async fn snapshot(&self) -> Vec<PoolEntry<I>> {
        let state = self.instances.read().await;
        state
            .iter()
            .map(|(id, m)| PoolEntry {
                id: id.clone(),
                runtime: m.runtime.clone(),
                semaphore: m.semaphore.clone(),
                queue_depth: m.queue_depth.clone(),
            })
            .collect()
    }

    /// Snapshot filtered to healthy instances only. Common case for
    /// most adapters.
    pub async fn healthy_snapshot(&self) -> Vec<PoolEntry<I>> {
        self.snapshot()
            .await
            .into_iter()
            .filter(|e| e.runtime.health().is_routable())
            .collect()
    }
}

/// A snapshot of one pool entry returned from
/// [`InstancePool::snapshot`]. Adapters hold these while ranking
/// and then call [`PoolEntry::try_acquire`] on the winner to
/// produce a [`Selection`].
pub struct PoolEntry<I: InstanceRuntime> {
    pub id: InstanceId,
    pub runtime: Arc<I>,
    pub semaphore: Arc<Semaphore>,
    pub queue_depth: Arc<AtomicU32>,
}

impl<I: InstanceRuntime> PoolEntry<I> {
    /// Try to acquire a concurrency permit on this instance. Does
    /// not block. Returns `AcquireError::Saturated` if no permits
    /// are available.
    ///
    /// The caller is responsible for placing a `ClaimGuard` against
    /// the Resources domain and assembling the full `Selection`
    /// bundle. The pool only handles the semaphore + depth guard
    /// halves because those are mechanical; the claim half requires
    /// adapter-specific knowledge (which device, which VRAM size,
    /// which compute stack).
    pub fn try_acquire(&self) -> Result<AcquiredSlot<I>, AcquireError> {
        if !self.runtime.health().is_routable() {
            return Err(AcquireError::NotHealthy(self.id.as_str().to_string()));
        }
        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(TryAcquireError::NoPermits) => {
                return Err(AcquireError::Saturated(self.id.as_str().to_string()))
            }
            Err(TryAcquireError::Closed) => {
                return Err(AcquireError::NotHealthy(self.id.as_str().to_string()))
            }
        };
        let depth_guard = DepthGuard::new(self.queue_depth.clone());
        Ok(AcquiredSlot {
            runtime: self.runtime.clone(),
            permit,
            depth_guard,
        })
    }

    /// Current in-flight count for this instance.
    pub fn current_depth(&self) -> u32 {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// Permits currently available for immediate acquisition.
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// Intermediate result of [`PoolEntry::try_acquire`]: the permit and
/// depth guard are held, but no resource claim has been placed yet.
/// The adapter completes the `Selection` by attaching its own
/// `ClaimGuard` via [`AcquiredSlot::into_selection`].
pub struct AcquiredSlot<I: InstanceRuntime> {
    pub runtime: Arc<I>,
    permit: OwnedSemaphorePermit,
    depth_guard: DepthGuard,
}

impl<I: InstanceRuntime> std::fmt::Debug for AcquiredSlot<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcquiredSlot")
            .field("instance", self.runtime.id())
            .finish()
    }
}

impl<I: InstanceRuntime> AcquiredSlot<I> {
    /// Combine the pool-side slot with a resource `ClaimGuard`
    /// produced by the adapter's call to `Resources::claim` to
    /// yield the full `Selection` RAII bundle.
    pub fn into_selection(self, claim: ClaimGuard) -> Selection<I> {
        Selection {
            instance: self.runtime,
            claim,
            _permit: self.permit,
            _depth_guard: self.depth_guard,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    // Synthetic runtime for exercising the primitives in isolation.
    // No network, no adapters, no real hardware.
    struct TestRuntime {
        id: InstanceId,
        stone: StoneName,
        capacity: Capacity,
        health: AtomicBool, // true=healthy, false=unhealthy
    }

    #[async_trait]
    impl InstanceRuntime for TestRuntime {
        fn id(&self) -> &InstanceId {
            &self.id
        }
        fn stone(&self) -> &StoneName {
            &self.stone
        }
        fn capacity(&self) -> &Capacity {
            &self.capacity
        }
        fn health(&self) -> Health {
            if self.health.load(Ordering::Relaxed) {
                Health::Healthy
            } else {
                Health::Unhealthy
            }
        }
    }

    fn runtime(id: &str, stone: &str, max: u32) -> Arc<TestRuntime> {
        Arc::new(TestRuntime {
            id: InstanceId::new(id),
            stone: StoneName::new(stone),
            capacity: Capacity {
                max_concurrent: max,
                typical_vram_mb: Some(1024),
                required_stack: ComputeStack::Cuda,
            },
            health: AtomicBool::new(true),
        })
    }

    #[tokio::test]
    async fn empty_pool_is_empty() {
        let pool: Arc<InstancePool<TestRuntime>> = InstancePool::new("test");
        assert!(pool.is_empty().await);
        assert_eq!(pool.len().await, 0);
        assert!(pool.snapshot().await.is_empty());
        assert!(pool.healthy_snapshot().await.is_empty());
    }

    #[tokio::test]
    async fn register_adds_instance_to_snapshot() {
        let pool: Arc<InstancePool<TestRuntime>> = InstancePool::new("test");
        pool.register(runtime("inst-a", "stone-a", 4)).await;
        pool.register(runtime("inst-b", "stone-b", 4)).await;
        assert_eq!(pool.len().await, 2);
        let snap = pool.snapshot().await;
        assert_eq!(snap.len(), 2);
        let ids: Vec<&str> = snap.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"inst-a"));
        assert!(ids.contains(&"inst-b"));
    }

    #[tokio::test]
    async fn healthy_snapshot_filters_unhealthy() {
        let pool: Arc<InstancePool<TestRuntime>> = InstancePool::new("test");
        let good = runtime("inst-good", "stone-a", 4);
        let bad = runtime("inst-bad", "stone-b", 4);
        bad.health.store(false, Ordering::Relaxed);
        pool.register(good).await;
        pool.register(bad).await;
        let all = pool.snapshot().await;
        assert_eq!(all.len(), 2);
        let healthy = pool.healthy_snapshot().await;
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].id.as_str(), "inst-good");
    }

    #[tokio::test]
    async fn try_acquire_respects_max_concurrent() {
        let pool: Arc<InstancePool<TestRuntime>> = InstancePool::new("test");
        pool.register(runtime("inst", "stone", 2)).await;
        let snap = pool.snapshot().await;
        let entry = &snap[0];

        let s1 = entry.try_acquire().expect("first permit");
        let s2 = entry.try_acquire().expect("second permit");
        let err = entry.try_acquire().unwrap_err();
        assert!(matches!(err, AcquireError::Saturated(_)));
        assert_eq!(entry.current_depth(), 2);
        assert_eq!(entry.available_permits(), 0);

        // Release one and re-check
        drop(s1);
        // Depth guard drop is synchronous (no tokio::spawn)
        assert_eq!(entry.current_depth(), 1);
        assert_eq!(entry.available_permits(), 1);
        let _s3 = entry.try_acquire().expect("recovered permit");

        drop(s2);
    }

    #[tokio::test]
    async fn try_acquire_rejects_unhealthy() {
        let pool: Arc<InstancePool<TestRuntime>> = InstancePool::new("test");
        let bad = runtime("inst-bad", "stone-b", 4);
        bad.health.store(false, Ordering::Relaxed);
        pool.register(bad).await;
        let snap = pool.snapshot().await;
        let err = snap[0].try_acquire().unwrap_err();
        assert!(matches!(err, AcquireError::NotHealthy(_)));
    }

    #[tokio::test]
    async fn depth_guard_decrements_on_drop() {
        let counter = Arc::new(AtomicU32::new(0));
        {
            let _g1 = DepthGuard::new(counter.clone());
            assert_eq!(counter.load(Ordering::Relaxed), 1);
            {
                let _g2 = DepthGuard::new(counter.clone());
                assert_eq!(counter.load(Ordering::Relaxed), 2);
            }
            assert_eq!(counter.load(Ordering::Relaxed), 1);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    /// Two synthetic adapters (distinct pools) run concurrently
    /// against overlapping stones — prove that independent pools
    /// don't share state and both can acquire in parallel.
    #[tokio::test]
    async fn two_pools_operate_independently() {
        let pool_a: Arc<InstancePool<TestRuntime>> = InstancePool::new("adapter-a");
        let pool_b: Arc<InstancePool<TestRuntime>> = InstancePool::new("adapter-b");

        pool_a.register(runtime("a-1", "stone-1", 2)).await;
        pool_b.register(runtime("b-1", "stone-1", 2)).await;
        pool_b.register(runtime("b-2", "stone-2", 2)).await;

        // Saturate pool A's one instance
        let snap_a = pool_a.snapshot().await;
        let _a1 = snap_a[0].try_acquire().unwrap();
        let _a2 = snap_a[0].try_acquire().unwrap();
        assert!(matches!(
            snap_a[0].try_acquire(),
            Err(AcquireError::Saturated(_))
        ));

        // Pool B is unaffected — different pool, different semaphores
        let snap_b = pool_b.snapshot().await;
        assert_eq!(snap_b.len(), 2);
        for entry in &snap_b {
            let _ = entry.try_acquire().expect("pool B unaffected by pool A");
        }
    }

    /// Parallel fanout: two adapters each processing two different
    /// requests concurrently. Proves the pool primitives don't
    /// introduce cross-adapter contention.
    #[tokio::test]
    async fn parallel_fanout_across_two_adapters() {
        let pool_a: Arc<InstancePool<TestRuntime>> = InstancePool::new("adapter-a");
        let pool_b: Arc<InstancePool<TestRuntime>> = InstancePool::new("adapter-b");

        pool_a.register(runtime("a-1", "stone-1", 4)).await;
        pool_b.register(runtime("b-1", "stone-1", 4)).await;

        // Four concurrent acquisitions — two per pool
        let pa = pool_a.clone();
        let pb = pool_b.clone();
        let h1 = tokio::spawn(async move {
            let snap = pa.snapshot().await;
            let slot = snap[0].try_acquire().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(slot);
            "adapter-a request 1"
        });
        let pa = pool_a.clone();
        let h2 = tokio::spawn(async move {
            let snap = pa.snapshot().await;
            let slot = snap[0].try_acquire().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(slot);
            "adapter-a request 2"
        });
        let pb2 = pb.clone();
        let h3 = tokio::spawn(async move {
            let snap = pb2.snapshot().await;
            let slot = snap[0].try_acquire().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(slot);
            "adapter-b request 1"
        });
        let h4 = tokio::spawn(async move {
            let snap = pb.snapshot().await;
            let slot = snap[0].try_acquire().unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            drop(slot);
            "adapter-b request 2"
        });

        let results = tokio::try_join!(h1, h2, h3, h4).expect("all tasks succeed");
        assert_eq!(results.0, "adapter-a request 1");
        assert_eq!(results.1, "adapter-a request 2");
        assert_eq!(results.2, "adapter-b request 1");
        assert_eq!(results.3, "adapter-b request 2");
    }
}
