//! Instance Manager — shared component for adapters with local
//! instance pools (ORCH-0030 §4).
//!
//! This is the "airline" half of the airport/airline split: each
//! adapter that has multiple instances (ComfyUI, Ollama, Whisper,
//! Infinity, etc.) wraps an `InstanceManager<I>` to handle selection,
//! health gating, queue depth, and resource claim coordination.
//!
//! The dispatcher picks the *provider* (the airport); the adapter's
//! Instance Manager picks the *instance* (which plane). This is the
//! load-bearing layering that makes shared-GPU coordination work.
//!
//! ## Default scheduling policy
//!
//! Least-loaded with pressure penalty: instances are ranked by
//!
//! ```text
//! score = queue_depth * QUEUE_WEIGHT + stone_pressure * PRESSURE_WEIGHT
//! ```
//!
//! where `stone_pressure` is read from [`Resources::pressure`]. The
//! lowest-scoring instance wins; ties are broken by instance id
//! (deterministic).
//!
//! Pluggable [`SchedulingPolicy`] is the extension point for priority,
//! deadline, or affinity scheduling in future commits.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};

use crate::domain::resources::{
    ClaimError, ClaimGuard, ClaimHolder, ClaimKind, ComputeStack, Resources, ResourceRequest,
    StoneName,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Healthy,
    Degraded,
    Offline,
}

// ── Capacity ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Capacity {
    /// Maximum concurrent in-flight requests this instance can handle.
    pub max_concurrent: u32,
    /// Estimated VRAM footprint when running a typical workload.
    /// `None` → adapter doesn't know; selection falls back to
    /// unsized claims (which degrade to exclusive).
    pub typical_vram_mb: Option<u64>,
    /// The compute stack this instance requires (CUDA for ComfyUI,
    /// CUDA/ROCm/Metal for Ollama, etc.).
    pub required_stack: ComputeStack,
}

// ── Instance trait ────────────────────────────────────────────

/// Adapter-specific runtime handle. Adapters implement this for
/// their concrete instance type; the Instance Manager owns the
/// selection logic on top of the trait.
#[async_trait]
pub trait InstanceRuntime: Send + Sync + 'static {
    fn id(&self) -> &InstanceId;
    fn stone(&self) -> &StoneName;
    fn capacity(&self) -> &Capacity;
    fn health(&self) -> Health;
}

// ── Managed instance ──────────────────────────────────────────

pub struct ManagedInstance<I: InstanceRuntime> {
    pub runtime: Arc<I>,
    pub semaphore: Arc<Semaphore>,
    pub queue_depth: Arc<std::sync::atomic::AtomicU32>,
}

impl<I: InstanceRuntime> ManagedInstance<I> {
    pub fn current_queue_depth(&self) -> u32 {
        self.queue_depth.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ── Selection result ──────────────────────────────────────────

pub struct Selection<I: InstanceRuntime> {
    pub instance: Arc<I>,
    /// The resource claim placed on the instance's stone. Must be
    /// held for the duration of the work; dropping releases it.
    pub claim: ClaimGuard,
    /// Permit held against the instance's concurrency semaphore.
    /// Dropped on completion.
    pub _permit: tokio::sync::OwnedSemaphorePermit,
    /// Queue depth tracker decrement guard. Dropped on completion.
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

/// RAII helper that decrements an instance's queue depth on drop.
pub struct DepthGuard {
    counter: Arc<std::sync::atomic::AtomicU32>,
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

// ── Errors ────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SelectError {
    #[error("no healthy instances available")]
    NoHealthyInstances,
    #[error("all instances are saturated")]
    AllInstancesSaturated,
    #[error("resource claim failed: {0}")]
    ResourceClaimFailed(#[from] ClaimError),
    #[error("no instance can satisfy compute stack requirement")]
    NoMatchingComputeStack,
}

// ── Scheduling policy ─────────────────────────────────────────

const QUEUE_WEIGHT: f64 = 100.0;
const PRESSURE_WEIGHT: f64 = 0.001; // pressure is in MB; needs scaling

/// Score one instance against current load + pressure. Lower score
/// is better. The default implementation is least-loaded with a
/// VRAM-pressure penalty.
pub trait SchedulingPolicy: Send + Sync + 'static {
    fn score(&self, queue_depth: u32, stone_pressure_mb: u64) -> f64 {
        (queue_depth as f64) * QUEUE_WEIGHT + (stone_pressure_mb as f64) * PRESSURE_WEIGHT
    }
}

pub struct LeastLoadedWithPressure;
impl SchedulingPolicy for LeastLoadedWithPressure {}

// ── Instance Manager ──────────────────────────────────────────

pub struct InstanceManager<I: InstanceRuntime> {
    instances: RwLock<HashMap<InstanceId, ManagedInstance<I>>>,
    resources: Arc<Resources>,
    policy: Box<dyn SchedulingPolicy>,
    adapter_name: String,
}

impl<I: InstanceRuntime> InstanceManager<I> {
    pub fn new(adapter_name: impl Into<String>, resources: Arc<Resources>) -> Arc<Self> {
        Arc::new(Self {
            instances: RwLock::new(HashMap::new()),
            resources,
            policy: Box::new(LeastLoadedWithPressure),
            adapter_name: adapter_name.into(),
        })
    }

    pub fn with_policy(
        adapter_name: impl Into<String>,
        resources: Arc<Resources>,
        policy: Box<dyn SchedulingPolicy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            instances: RwLock::new(HashMap::new()),
            resources,
            policy,
            adapter_name: adapter_name.into(),
        })
    }

    /// Add a new instance to the pool.
    pub async fn register(&self, runtime: Arc<I>) {
        let id = runtime.id().clone();
        let max = runtime.capacity().max_concurrent.max(1);
        let semaphore = Arc::new(Semaphore::new(max as usize));
        let queue_depth = Arc::new(std::sync::atomic::AtomicU32::new(0));
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

    /// Remove an instance from the pool.
    pub async fn unregister(&self, id: &InstanceId) {
        let mut state = self.instances.write().await;
        state.remove(id);
    }

    /// Total number of registered instances (including unhealthy).
    pub async fn len(&self) -> usize {
        let state = self.instances.read().await;
        state.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Select the best instance for a request and place a resource
    /// claim against the chosen instance's stone.
    ///
    /// Returns a [`Selection`] holding:
    /// - the chosen instance
    /// - a [`ClaimGuard`] that releases the resource on drop
    /// - a semaphore permit that returns capacity on drop
    /// - a [`DepthGuard`] that decrements queue depth on drop
    ///
    /// The caller dispatches against the selection's instance and
    /// drops the selection on completion (success or failure).
    pub async fn select(&self, _action: Option<&str>) -> Result<Selection<I>, SelectError> {
        // 1. Filter to healthy instances
        let candidates: Vec<(InstanceId, Arc<I>, Arc<Semaphore>, Arc<std::sync::atomic::AtomicU32>)> = {
            let state = self.instances.read().await;
            if state.is_empty() {
                return Err(SelectError::NoHealthyInstances);
            }
            state
                .iter()
                .filter(|(_, m)| matches!(m.runtime.health(), Health::Healthy))
                .map(|(id, m)| {
                    (
                        id.clone(),
                        m.runtime.clone(),
                        m.semaphore.clone(),
                        m.queue_depth.clone(),
                    )
                })
                .collect()
        };

        if candidates.is_empty() {
            return Err(SelectError::NoHealthyInstances);
        }

        // 2. Score each candidate using the scheduling policy
        let mut scored: Vec<(f64, InstanceId, Arc<I>, Arc<Semaphore>, Arc<std::sync::atomic::AtomicU32>)> =
            Vec::with_capacity(candidates.len());
        for (id, runtime, sem, depth_ctr) in candidates {
            let depth = depth_ctr.load(std::sync::atomic::Ordering::Relaxed);
            let pressure = self
                .resources
                .pressure(runtime.stone())
                .await
                .map(|p| {
                    p.gpus
                        .iter()
                        .map(|g| g.committed_mb)
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let score = self.policy.score(depth, pressure);
            scored.push((score, id, runtime, sem, depth_ctr));
        }
        scored.sort_by(|a, b| {
            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.as_str().cmp(b.1.as_str()))
        });

        // 3. Try to acquire a semaphore permit + place a claim, in
        //    score order. The first instance that successfully
        //    claims wins.
        for (_score, id, runtime, sem, depth_ctr) in scored {
            // Try to acquire a permit without blocking
            let permit = match sem.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => continue, // saturated, try next
            };

            // Increment queue depth
            depth_ctr.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let depth_guard = DepthGuard {
                counter: depth_ctr.clone(),
            };

            // Place a resource claim
            let stone = runtime.stone().clone();
            let cap = runtime.capacity();
            let request = ResourceRequest::Gpu {
                stone: stone.clone(),
                device: 0, // first device; future commit picks the best device
                vram_mb: cap.typical_vram_mb,
                required_stack: cap.required_stack,
            };
            let claim = match self
                .resources
                .claim(
                    ClaimHolder::new(self.adapter_name.clone(), id.as_str()),
                    request,
                    ClaimKind::Hard,
                )
                .await
            {
                Ok(g) => g,
                Err(ClaimError::UnknownStone(_)) | Err(ClaimError::UnknownDevice { .. }) => {
                    // Stone topology not yet hydrated for this
                    // instance; the claim is best-effort. Proceed
                    // without a guard so the instance still gets
                    // dispatched. The Resources domain will track
                    // it once topology lands.
                    drop(permit);
                    drop(depth_guard);
                    continue;
                }
                Err(e) => {
                    // Real conflict (capacity exhausted, exclusive
                    // hold, wrong compute stack). Try the next
                    // instance.
                    drop(permit);
                    drop(depth_guard);
                    tracing::debug!(
                        instance = %id,
                        error = %e,
                        "instance manager: claim rejected, trying next"
                    );
                    continue;
                }
            };

            return Ok(Selection {
                instance: runtime,
                claim,
                _permit: permit,
                _depth_guard: depth_guard,
            });
        }

        Err(SelectError::AllInstancesSaturated)
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::EventBus;
    use crate::domain::resources::{StoneTopology, TopologyGpu, GpuVendor};

    struct TestRuntime {
        id: InstanceId,
        stone: StoneName,
        capacity: Capacity,
        health: Health,
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
            self.health
        }
    }

    fn instance(id: &str, stone: &str, vram: u64, health: Health) -> Arc<TestRuntime> {
        Arc::new(TestRuntime {
            id: InstanceId::new(id),
            stone: StoneName::new(stone),
            capacity: Capacity {
                max_concurrent: 4,
                typical_vram_mb: Some(vram),
                required_stack: ComputeStack::Cuda,
            },
            health,
        })
    }

    async fn setup_with_stones(
        stones: &[(&str, GpuVendor, Vec<ComputeStack>, u64)],
    ) -> (Arc<Resources>, Arc<InstanceManager<TestRuntime>>) {
        let bus = EventBus::new();
        let resources = Resources::new(bus);
        for (name, vendor, stack, vram) in stones {
            resources
                .update_topology(
                    StoneName::new(*name),
                    StoneTopology {
                        gpus: vec![TopologyGpu {
                            index: 0,
                            name: format!("{:?}", vendor),
                            vendor: *vendor,
                            compute_stack: stack.clone(),
                            total_vram_mb: Some(*vram),
                        }],
                        memory_total_mb: Some(32768),
                    },
                )
                .await;
        }
        let mgr = InstanceManager::new("test", resources.clone());
        (resources, mgr)
    }

    #[tokio::test]
    async fn empty_pool_returns_no_healthy() {
        let (_, mgr) = setup_with_stones(&[]).await;
        let err = mgr.select(None).await.unwrap_err();
        assert!(matches!(err, SelectError::NoHealthyInstances));
    }

    #[tokio::test]
    async fn picks_only_healthy_instance() {
        let (_, mgr) = setup_with_stones(&[
            ("stone-a", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
            ("stone-b", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
        ])
        .await;
        mgr.register(instance("inst-a", "stone-a", 6144, Health::Offline)).await;
        mgr.register(instance("inst-b", "stone-b", 6144, Health::Healthy)).await;
        let sel = mgr.select(None).await.unwrap();
        assert_eq!(sel.instance.id().as_str(), "inst-b");
    }

    #[tokio::test]
    async fn picks_least_loaded_when_pressure_equal() {
        let (_, mgr) = setup_with_stones(&[
            ("stone-a", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
            ("stone-b", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
        ])
        .await;
        let inst_a = instance("inst-a", "stone-a", 6144, Health::Healthy);
        let inst_b = instance("inst-b", "stone-b", 6144, Health::Healthy);
        mgr.register(inst_a).await;
        mgr.register(inst_b).await;

        // First selection — both are tied at depth=0, ties broken
        // by id alphabetically → picks inst-a
        let sel1 = mgr.select(None).await.unwrap();
        assert_eq!(sel1.instance.id().as_str(), "inst-a");

        // Second selection — inst-a now has depth=1, inst-b has 0
        let sel2 = mgr.select(None).await.unwrap();
        assert_eq!(sel2.instance.id().as_str(), "inst-b");
    }

    #[tokio::test]
    async fn cuda_request_skips_amd_only_stone() {
        let (_, mgr) = setup_with_stones(&[
            ("stone-cuda", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
            ("stone-rocm", GpuVendor::Amd, vec![ComputeStack::Rocm], 16384),
        ])
        .await;
        // Both healthy
        mgr.register(instance("inst-cuda", "stone-cuda", 6144, Health::Healthy)).await;
        mgr.register(instance("inst-rocm", "stone-rocm", 6144, Health::Healthy)).await;

        // Selection requires CUDA. The AMD instance's claim will be
        // rejected by Resources, so the manager picks the CUDA one.
        let sel = mgr.select(None).await.unwrap();
        assert_eq!(sel.instance.id().as_str(), "inst-cuda");
    }

    #[tokio::test]
    async fn saturation_returns_all_saturated() {
        let (_, mgr) = setup_with_stones(&[
            ("stone-a", GpuVendor::Nvidia, vec![ComputeStack::Cuda], 24576),
        ])
        .await;
        // One instance, max_concurrent=4
        mgr.register(instance("inst-a", "stone-a", 1024, Health::Healthy)).await;

        // Drain all 4 permits
        let s1 = mgr.select(None).await.unwrap();
        let s2 = mgr.select(None).await.unwrap();
        let s3 = mgr.select(None).await.unwrap();
        let s4 = mgr.select(None).await.unwrap();

        // The 5th attempt should fail
        let err = mgr.select(None).await.unwrap_err();
        assert!(matches!(err, SelectError::AllInstancesSaturated));

        // Drop one and verify recovery
        drop(s1);
        // Selection requires the resource claim release to land,
        // which happens via tokio::spawn on Drop. Yield to give it
        // a tick.
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _s5 = mgr.select(None).await.unwrap();

        drop(s2);
        drop(s3);
        drop(s4);
    }
}
