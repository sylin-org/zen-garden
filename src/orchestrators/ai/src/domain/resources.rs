//! Resources domain — physical stone resources with claim accounting
//! (ORCH-0030 §2).
//!
//! The Resources domain is the single authority for physical hardware
//! contention in the garden. Adapters that dispatch GPU work place
//! claims against this domain before starting; the claims compose
//! when sizes are known and degrade to exclusive holds when they
//! aren't. The domain publishes pressure events on the unified
//! [`crate::domain::events::EventBus`] so dashboards and recommendation
//! engines can react to changing supply.
//!
//! ## Hybrid claim model
//!
//! Two axes of "known / unknown" compose into a four-quadrant rule:
//!
//! ```text
//! +----------------+----------------------+----------------------+
//! |                | total known          | total unknown        |
//! +----------------+----------------------+----------------------+
//! | sized claim    | shared accounting    | degrades to exclusive|
//! | unsized claim  | exclusive hold       | exclusive hold       |
//! +----------------+----------------------+----------------------+
//! ```
//!
//! - Sized + known → multiple adapters share the device (sum of
//!   `committed_mb` plus `headroom_mb` must not exceed `total_mb`).
//! - Unsized → device is exclusively held until released.
//! - Sized + unknown total → device behaves exclusively, but the
//!   claim is recorded for observability and the upgrade path is
//!   ready when topology eventually reports a total.
//!
//! Sized and unsized claims **never coexist** on the same device.
//!
//! ## Compute-stack capability filtering
//!
//! Each `GpuDevice` advertises its supported [`ComputeStack`]s
//! (CUDA, ROCm, Metal, OneAPI, Vulkan, Cpu). Each
//! [`ResourceRequest::Gpu`] carries a `required_stack`. The domain
//! rejects any claim where the device cannot satisfy the requirement
//! with [`ClaimError::UnsupportedComputeStack`]. This is the
//! load-bearing rule for heterogeneous gardens — ComfyUI requesting
//! CUDA on an AMD-only stone fails fast and the Instance Manager
//! routes to a different stone.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use garden_common::utils::ids::generate_guidv7;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::domain::events::EventBus;

// ── Identity ──────────────────────────────────────────────────

/// Stone identity. Mirrors the moss-side stone naming convention.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoneName(String);

impl StoneName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StoneName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Opaque claim identifier. Stable for the lifetime of one claim.
/// String-backed (GUIDv7) so we don't need a direct uuid dependency.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaimId(String);

impl ClaimId {
    pub fn generate() -> Self {
        Self(generate_guidv7())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ClaimId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who placed a claim. Used for liveness tracking and eviction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaimHolder {
    pub adapter: String,
    pub instance: String,
}

impl ClaimHolder {
    pub fn new(adapter: impl Into<String>, instance: impl Into<String>) -> Self {
        Self {
            adapter: adapter.into(),
            instance: instance.into(),
        }
    }
}

// ── Hardware capabilities ─────────────────────────────────────

/// GPU vendor enumeration. Hardware that doesn't fit a known vendor
/// is reported as `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,
    Unknown,
}

/// Compute stack supported by a device. A device may support
/// multiple stacks; e.g., an NVIDIA GPU supports CUDA and Vulkan,
/// an AMD GPU supports ROCm and Vulkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComputeStack {
    Cuda,
    Rocm,
    OneApi,
    Metal,
    Vulkan,
    Cpu,
}

// ── Device state ──────────────────────────────────────────────

/// Mode of a single device, derived from its current claim set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceMode {
    /// No active claims, or only sized claims on a known-total
    /// device. Sized claims compose; unsized claims are rejected.
    Shared,
    /// One unsized claim, or a sized claim on an unknown-total
    /// device. New claims (sized or not) are rejected.
    Exclusive,
    /// The device's total VRAM is not known (topology never
    /// reported it). Always behaves as exclusive — accounting is
    /// not safe without a total to subtract from.
    Opaque,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    pub vendor: GpuVendor,
    pub compute_stack: Vec<ComputeStack>,
    /// `None` → unknown total; forces opaque/exclusive mode.
    pub total_vram_mb: Option<u64>,
    pub headroom_mb: u64,
    pub committed_mb: u64,
    pub mode: DeviceMode,
}

impl GpuDevice {
    /// Compute available VRAM after headroom and committed claims.
    /// Returns `None` if total is unknown.
    pub fn available_mb(&self) -> Option<u64> {
        let total = self.total_vram_mb?;
        Some(total.saturating_sub(self.committed_mb).saturating_sub(self.headroom_mb))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryResource {
    pub total_mb: Option<u64>,
    pub committed_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoneResources {
    pub name: StoneName,
    pub gpus: Vec<GpuDevice>,
    pub memory: MemoryResource,
    /// Active claims, keyed by id.
    pub claims: HashMap<String, Claim>,
}

impl StoneResources {
    pub fn new(name: StoneName) -> Self {
        Self {
            name,
            gpus: Vec::new(),
            memory: MemoryResource {
                total_mb: None,
                committed_mb: 0,
            },
            claims: HashMap::new(),
        }
    }
}

// ── Claims ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimKind {
    /// In-flight work that consumes the resource right now.
    Hard,
    /// Reserved-for-queued-work; contributes to pressure but does
    /// not block hard claims from landing. Promoted to Hard when
    /// the queued work actually starts.
    Soft,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub holder: ClaimHolder,
    pub request: ResourceRequest,
    pub kind: ClaimKind,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceRequest {
    Gpu {
        stone: StoneName,
        device: u32,
        /// `None` → unsized (exclusive)
        vram_mb: Option<u64>,
        required_stack: ComputeStack,
    },
    Memory {
        stone: StoneName,
        mb: Option<u64>,
    },
}

impl ResourceRequest {
    pub fn stone(&self) -> &StoneName {
        match self {
            Self::Gpu { stone, .. } => stone,
            Self::Memory { stone, .. } => stone,
        }
    }
}

// ── Errors ────────────────────────────────────────────────────

#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ClaimError {
    #[error("insufficient VRAM on stone={stone} device={device}: requested={requested}MB, available={available}MB")]
    InsufficientVram {
        stone: StoneName,
        device: u32,
        requested: u64,
        available: u64,
    },
    #[error("insufficient memory on stone={stone}: requested={requested}MB, available={available}MB")]
    InsufficientMemory {
        stone: StoneName,
        requested: u64,
        available: u64,
    },
    #[error("device stone={stone} device={device} is exclusively held by {holder:?}")]
    DeviceExclusivelyHeld {
        stone: StoneName,
        device: u32,
        holder: ClaimHolder,
    },
    #[error("device stone={stone} device={device} does not support compute stack {required:?}; supported: {available:?}")]
    UnsupportedComputeStack {
        stone: StoneName,
        device: u32,
        required: ComputeStack,
        available: Vec<ComputeStack>,
    },
    #[error("cannot mix sized and unsized claims on stone={stone} device={device}")]
    SizeMismatchMode {
        stone: StoneName,
        device: u32,
        current_mode: DeviceMode,
    },
    #[error("unknown stone {0}")]
    UnknownStone(StoneName),
    #[error("unknown device {device} on stone={stone}")]
    UnknownDevice { stone: StoneName, device: u32 },
}

// ── Pressure snapshot ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PressureSnapshot {
    pub stone: StoneName,
    pub gpus: Vec<GpuPressure>,
    pub memory: MemoryPressure,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuPressure {
    pub device: u32,
    pub vendor: GpuVendor,
    pub mode: DeviceMode,
    pub total_mb: Option<u64>,
    pub committed_mb: u64,
    pub available_mb: Option<u64>,
    pub headroom_mb: u64,
    pub hard_claims: usize,
    pub soft_claims: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPressure {
    pub total_mb: Option<u64>,
    pub committed_mb: u64,
    pub available_mb: Option<u64>,
}

// ── Topology hydration shape ──────────────────────────────────

/// Hardware topology snapshot for a stone, fed in by garden
/// discovery (or test fixtures). The Resources domain reads totals
/// and capabilities from this; it never tries to discover hardware
/// itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoneTopology {
    pub gpus: Vec<TopologyGpu>,
    pub memory_total_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyGpu {
    pub index: u32,
    pub name: String,
    pub vendor: GpuVendor,
    pub compute_stack: Vec<ComputeStack>,
    pub total_vram_mb: Option<u64>,
}

// ── The Resources domain ──────────────────────────────────────

const DEFAULT_HEADROOM_MB: u64 = 512;

pub struct Resources {
    state: Mutex<ResourcesState>,
    events: Arc<EventBus>,
}

#[derive(Default)]
struct ResourcesState {
    stones: HashMap<StoneName, StoneResources>,
}

impl Resources {
    pub fn new(events: Arc<EventBus>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(ResourcesState::default()),
            events,
        })
    }

    /// Snapshot the full state of one stone.
    pub async fn snapshot(&self, stone: &StoneName) -> Option<StoneResources> {
        let state = self.state.lock().await;
        state.stones.get(stone).cloned()
    }

    /// Snapshot every known stone.
    pub async fn snapshot_all(&self) -> Vec<StoneResources> {
        let state = self.state.lock().await;
        state.stones.values().cloned().collect()
    }

    /// Compute the current pressure snapshot for a stone.
    pub async fn pressure(&self, stone: &StoneName) -> Option<PressureSnapshot> {
        let state = self.state.lock().await;
        let st = state.stones.get(stone)?;
        Some(build_pressure(st))
    }

    /// Update topology for a stone. Adds the stone if absent;
    /// updates per-device totals/capabilities if present. Existing
    /// claims survive — they may transition between modes if
    /// total_vram_mb went from `None` to `Some`.
    pub async fn update_topology(
        &self,
        stone: StoneName,
        topology: StoneTopology,
    ) {
        let mut state = self.state.lock().await;
        let entry = state
            .stones
            .entry(stone.clone())
            .or_insert_with(|| StoneResources::new(stone.clone()));

        // Update memory total
        entry.memory.total_mb = topology.memory_total_mb;

        // Build a map of existing committed_mb per device so we
        // preserve accounting across topology updates
        let existing_committed: HashMap<u32, u64> = entry
            .gpus
            .iter()
            .map(|g| (g.index, g.committed_mb))
            .collect();

        entry.gpus = topology
            .gpus
            .into_iter()
            .map(|tg| {
                let committed_mb = *existing_committed.get(&tg.index).unwrap_or(&0);
                let mode = derive_mode(tg.total_vram_mb, &entry.claims, tg.index);
                GpuDevice {
                    index: tg.index,
                    name: tg.name,
                    vendor: tg.vendor,
                    compute_stack: tg.compute_stack,
                    total_vram_mb: tg.total_vram_mb,
                    headroom_mb: DEFAULT_HEADROOM_MB,
                    committed_mb,
                    mode,
                }
            })
            .collect();

        let topic = format!("resources.stone.{}.topology.changed", stone.as_str());
        let payload = serde_json::json!({
            "stone": stone.as_str(),
            "gpus": entry.gpus.len(),
        });
        drop(state);
        self.events.publish(&topic, &payload).await;
    }

    /// Place a claim. Returns a [`ClaimGuard`] that releases the
    /// claim on drop.
    pub async fn claim(
        self: &Arc<Self>,
        holder: ClaimHolder,
        request: ResourceRequest,
        kind: ClaimKind,
    ) -> Result<ClaimGuard, ClaimError> {
        let stone_name = request.stone().clone();
        let (claim_id, topic) = {
            let mut state = self.state.lock().await;
            let stone = state
                .stones
                .get_mut(&stone_name)
                .ok_or_else(|| ClaimError::UnknownStone(stone_name.clone()))?;

            // Validate the request shape against current device state
            match &request {
                ResourceRequest::Gpu {
                    device,
                    vram_mb,
                    required_stack,
                    ..
                } => {
                    let dev_idx = *device;
                    let dev = stone
                        .gpus
                        .iter_mut()
                        .find(|g| g.index == dev_idx)
                        .ok_or_else(|| ClaimError::UnknownDevice {
                            stone: stone_name.clone(),
                            device: dev_idx,
                        })?;

                    // 1. Compute-stack capability filter (load-bearing)
                    if !dev.compute_stack.contains(required_stack) {
                        return Err(ClaimError::UnsupportedComputeStack {
                            stone: stone_name.clone(),
                            device: dev_idx,
                            required: *required_stack,
                            available: dev.compute_stack.clone(),
                        });
                    }

                    // Inspect existing claims on this device
                    let existing: Vec<&Claim> = stone
                        .claims
                        .values()
                        .filter(|c| match &c.request {
                            ResourceRequest::Gpu {
                                device: d,
                                vram_mb: _,
                                ..
                            } => *d == dev_idx,
                            _ => false,
                        })
                        .collect();

                    let any_unsized = existing.iter().any(|c| matches!(
                        &c.request,
                        ResourceRequest::Gpu { vram_mb: None, .. }
                    ));
                    let any_sized_hard = existing.iter().any(|c| {
                        matches!(c.kind, ClaimKind::Hard)
                            && matches!(
                                &c.request,
                                ResourceRequest::Gpu { vram_mb: Some(_), .. }
                            )
                    });

                    // 2. Mixing rule: sized + unsized never coexist
                    match (vram_mb, any_unsized, any_sized_hard) {
                        // Unsized claim against a device with any
                        // active claim → exclusive conflict
                        (None, true, _) | (None, _, true) => {
                            let holder = existing
                                .first()
                                .map(|c| c.holder.clone())
                                .unwrap_or_else(|| holder.clone());
                            return Err(ClaimError::DeviceExclusivelyHeld {
                                stone: stone_name.clone(),
                                device: dev_idx,
                                holder,
                            });
                        }
                        // Sized claim against a device with an
                        // unsized hold → exclusive conflict
                        (Some(_), true, _) => {
                            let holder = existing
                                .iter()
                                .find(|c| {
                                    matches!(
                                        &c.request,
                                        ResourceRequest::Gpu { vram_mb: None, .. }
                                    )
                                })
                                .map(|c| c.holder.clone())
                                .unwrap_or_else(|| holder.clone());
                            return Err(ClaimError::DeviceExclusivelyHeld {
                                stone: stone_name.clone(),
                                device: dev_idx,
                                holder,
                            });
                        }
                        _ => {}
                    }

                    // 3. Sized + known-total → accounting check
                    if let Some(req_mb) = vram_mb {
                        if let Some(total) = dev.total_vram_mb {
                            // Sum of *hard* sized claims that
                            // currently consume the GPU
                            let hard_committed: u64 = existing
                                .iter()
                                .filter(|c| matches!(c.kind, ClaimKind::Hard))
                                .filter_map(|c| match &c.request {
                                    ResourceRequest::Gpu {
                                        vram_mb: Some(mb), ..
                                    } => Some(*mb),
                                    _ => None,
                                })
                                .sum();
                            // Soft claims contribute to pressure
                            // but only block other soft claims —
                            // they don't reserve hard capacity.
                            let projected = hard_committed
                                .saturating_add(*req_mb)
                                .saturating_add(dev.headroom_mb);
                            if projected > total {
                                let available = total
                                    .saturating_sub(hard_committed)
                                    .saturating_sub(dev.headroom_mb);
                                return Err(ClaimError::InsufficientVram {
                                    stone: stone_name.clone(),
                                    device: dev_idx,
                                    requested: *req_mb,
                                    available,
                                });
                            }
                        } else {
                            // Sized + unknown total → degrade to
                            // exclusive: any other claim blocks us
                            if !existing.is_empty() {
                                let holder = existing
                                    .first()
                                    .map(|c| c.holder.clone())
                                    .unwrap_or_else(|| holder.clone());
                                return Err(ClaimError::DeviceExclusivelyHeld {
                                    stone: stone_name.clone(),
                                    device: dev_idx,
                                    holder,
                                });
                            }
                        }
                    }
                }
                ResourceRequest::Memory { mb, .. } => {
                    if let (Some(req_mb), Some(total)) = (mb, stone.memory.total_mb) {
                        let projected = stone.memory.committed_mb.saturating_add(*req_mb);
                        if projected > total {
                            return Err(ClaimError::InsufficientMemory {
                                stone: stone_name.clone(),
                                requested: *req_mb,
                                available: total.saturating_sub(stone.memory.committed_mb),
                            });
                        }
                    }
                }
            }

            // Admit the claim
            let claim_id = ClaimId::generate();
            let claim = Claim {
                id: claim_id.clone(),
                holder: holder.clone(),
                request: request.clone(),
                kind,
                granted_at: Utc::now(),
                expires_at: None,
            };
            stone.claims.insert(claim_id.as_str().to_string(), claim);

            // Recompute committed_mb and mode for the affected device
            recompute_device_state(stone);

            let topic = format!("resources.stone.{}.claim.granted", stone_name.as_str());
            (claim_id, topic)
        };

        let payload = serde_json::json!({
            "claim_id": claim_id.as_str(),
            "holder": holder,
            "request": request,
            "kind": kind,
        });
        self.events.publish(&topic, &payload).await;

        // Also publish an updated pressure snapshot for the affected device
        if let Some(snapshot) = self.pressure(&stone_name).await {
            let pressure_topic = format!("resources.stone.{}.pressure", stone_name.as_str());
            self.events.publish(&pressure_topic, &snapshot).await;
        }

        Ok(ClaimGuard {
            id: claim_id,
            stone: stone_name,
            resources: Some(Arc::clone(self)),
        })
    }

    /// Release a claim by id. Idempotent: releasing an unknown
    /// claim is a no-op.
    pub async fn release(self: &Arc<Self>, claim_id: &ClaimId, stone: &StoneName) {
        let removed = {
            let mut state = self.state.lock().await;
            let Some(stone_state) = state.stones.get_mut(stone) else {
                return;
            };
            let removed = stone_state.claims.remove(claim_id.as_str());
            if removed.is_some() {
                recompute_device_state(stone_state);
            }
            removed
        };
        if removed.is_some() {
            let topic = format!("resources.stone.{}.claim.released", stone.as_str());
            let payload = serde_json::json!({
                "claim_id": claim_id.as_str(),
            });
            self.events.publish(&topic, &payload).await;
            if let Some(snapshot) = self.pressure(stone).await {
                let pressure_topic = format!("resources.stone.{}.pressure", stone.as_str());
                self.events.publish(&pressure_topic, &snapshot).await;
            }
        }
    }

    /// Promote a soft claim to hard. Recomputes device state.
    pub async fn promote_soft_to_hard(
        self: &Arc<Self>,
        claim_id: &ClaimId,
        stone: &StoneName,
    ) -> Result<(), ClaimError> {
        let mut state = self.state.lock().await;
        let stone_state = state
            .stones
            .get_mut(stone)
            .ok_or_else(|| ClaimError::UnknownStone(stone.clone()))?;
        let Some(claim) = stone_state.claims.get_mut(claim_id.as_str()) else {
            return Ok(());
        };
        claim.kind = ClaimKind::Hard;
        recompute_device_state(stone_state);
        Ok(())
    }

    /// Evict every claim held by `(adapter, instance?)`. If
    /// `instance` is None, evict every instance under `adapter`.
    pub async fn evict_holder(self: &Arc<Self>, adapter: &str, instance: Option<&str>) {
        let mut affected_stones: Vec<StoneName> = Vec::new();
        let mut released_count = 0usize;
        {
            let mut state = self.state.lock().await;
            for (stone_name, stone_state) in state.stones.iter_mut() {
                let to_remove: Vec<String> = stone_state
                    .claims
                    .iter()
                    .filter(|(_, c)| {
                        c.holder.adapter == adapter
                            && instance.map(|i| c.holder.instance == i).unwrap_or(true)
                    })
                    .map(|(k, _)| k.clone())
                    .collect();
                if !to_remove.is_empty() {
                    for k in &to_remove {
                        stone_state.claims.remove(k);
                        released_count += 1;
                    }
                    recompute_device_state(stone_state);
                    affected_stones.push(stone_name.clone());
                }
            }
        }
        for stone in affected_stones {
            if let Some(snapshot) = self.pressure(&stone).await {
                let topic = format!("resources.stone.{}.pressure", stone.as_str());
                self.events.publish(&topic, &snapshot).await;
            }
        }
        tracing::info!(adapter, ?instance, released_count, "resources: evicted holder");
    }
}

/// RAII guard returned by [`Resources::claim`]. On drop, the claim
/// is released asynchronously via `tokio::spawn`. Callers that want
/// synchronous release semantics can explicitly call
/// [`ClaimGuard::release_now`].
pub struct ClaimGuard {
    pub id: ClaimId,
    pub stone: StoneName,
    resources: Option<Arc<Resources>>,
}

impl std::fmt::Debug for ClaimGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaimGuard")
            .field("id", &self.id)
            .field("stone", &self.stone)
            .finish()
    }
}

impl ClaimGuard {
    pub async fn release_now(mut self) {
        if let Some(r) = self.resources.take() {
            r.release(&self.id, &self.stone).await;
        }
    }
}

impl Drop for ClaimGuard {
    fn drop(&mut self) {
        if let Some(r) = self.resources.take() {
            let id = self.id.clone();
            let stone = self.stone.clone();
            tokio::spawn(async move {
                r.release(&id, &stone).await;
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn build_pressure(stone: &StoneResources) -> PressureSnapshot {
    let gpus = stone
        .gpus
        .iter()
        .map(|d| {
            let (hard, soft) = stone
                .claims
                .values()
                .filter(|c| match &c.request {
                    ResourceRequest::Gpu { device, .. } => *device == d.index,
                    _ => false,
                })
                .fold((0usize, 0usize), |(h, s), c| match c.kind {
                    ClaimKind::Hard => (h + 1, s),
                    ClaimKind::Soft => (h, s + 1),
                });
            GpuPressure {
                device: d.index,
                vendor: d.vendor,
                mode: d.mode,
                total_mb: d.total_vram_mb,
                committed_mb: d.committed_mb,
                available_mb: d.available_mb(),
                headroom_mb: d.headroom_mb,
                hard_claims: hard,
                soft_claims: soft,
            }
        })
        .collect();
    PressureSnapshot {
        stone: stone.name.clone(),
        gpus,
        memory: MemoryPressure {
            total_mb: stone.memory.total_mb,
            committed_mb: stone.memory.committed_mb,
            available_mb: stone
                .memory
                .total_mb
                .map(|t| t.saturating_sub(stone.memory.committed_mb)),
        },
    }
}

/// Recompute committed_mb + mode for every device on a stone after
/// a claim mutation.
fn recompute_device_state(stone: &mut StoneResources) {
    for dev in stone.gpus.iter_mut() {
        let dev_idx = dev.index;
        let mut hard_sized: u64 = 0;
        let mut any_unsized = false;
        let mut any_claim = false;
        for claim in stone.claims.values() {
            if let ResourceRequest::Gpu { device, vram_mb, .. } = &claim.request {
                if *device == dev_idx {
                    any_claim = true;
                    match (claim.kind, vram_mb) {
                        (ClaimKind::Hard, Some(mb)) => hard_sized = hard_sized.saturating_add(*mb),
                        (_, None) => any_unsized = true,
                        _ => {}
                    }
                }
            }
        }
        dev.committed_mb = hard_sized;
        dev.mode = if dev.total_vram_mb.is_none() {
            DeviceMode::Opaque
        } else if any_unsized {
            DeviceMode::Exclusive
        } else if any_claim {
            DeviceMode::Shared
        } else {
            DeviceMode::Shared
        };
    }
}

/// Initial mode derivation when a topology update brings a new
/// device online (no claims yet).
fn derive_mode(
    total_vram_mb: Option<u64>,
    _claims: &HashMap<String, Claim>,
    _device: u32,
) -> DeviceMode {
    if total_vram_mb.is_none() {
        DeviceMode::Opaque
    } else {
        DeviceMode::Shared
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::events::EventBus;

    fn nvidia_24g(idx: u32) -> TopologyGpu {
        TopologyGpu {
            index: idx,
            name: format!("NVIDIA RTX 4090"),
            vendor: GpuVendor::Nvidia,
            compute_stack: vec![ComputeStack::Cuda, ComputeStack::Vulkan],
            total_vram_mb: Some(24576),
        }
    }

    fn amd_16g(idx: u32) -> TopologyGpu {
        TopologyGpu {
            index: idx,
            name: format!("AMD Radeon VII"),
            vendor: GpuVendor::Amd,
            compute_stack: vec![ComputeStack::Rocm, ComputeStack::Vulkan],
            total_vram_mb: Some(16384),
        }
    }

    async fn test_setup() -> (Arc<Resources>, StoneName) {
        let bus = EventBus::new();
        let r = Resources::new(bus);
        let s = StoneName::new("stone-test");
        r.update_topology(
            s.clone(),
            StoneTopology {
                gpus: vec![nvidia_24g(0)],
                memory_total_mb: Some(32768),
            },
        )
        .await;
        (r, s)
    }

    #[tokio::test]
    async fn topology_added_to_state() {
        let (r, s) = test_setup().await;
        let snap = r.snapshot(&s).await.unwrap();
        assert_eq!(snap.gpus.len(), 1);
        assert_eq!(snap.gpus[0].total_vram_mb, Some(24576));
        assert_eq!(snap.gpus[0].mode, DeviceMode::Shared);
    }

    #[tokio::test]
    async fn sized_cuda_claim_succeeds() {
        let (r, s) = test_setup().await;
        let g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let snap = r.snapshot(&s).await.unwrap();
        assert_eq!(snap.gpus[0].committed_mb, 6144);
        assert_eq!(snap.gpus[0].mode, DeviceMode::Shared);
        drop(g);
    }

    #[tokio::test]
    async fn cuda_on_amd_rejected() {
        let bus = EventBus::new();
        let r = Resources::new(bus);
        let s = StoneName::new("stone-amd");
        r.update_topology(
            s.clone(),
            StoneTopology {
                gpus: vec![amd_16g(0)],
                memory_total_mb: Some(16384),
            },
        )
        .await;
        let err = r
            .claim(
                ClaimHolder::new("comfyui", "stone-amd"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        match err {
            ClaimError::UnsupportedComputeStack {
                required, available, ..
            } => {
                assert_eq!(required, ComputeStack::Cuda);
                assert!(available.contains(&ComputeStack::Rocm));
            }
            other => panic!("expected UnsupportedComputeStack, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rocm_on_amd_succeeds() {
        let bus = EventBus::new();
        let r = Resources::new(bus);
        let s = StoneName::new("stone-amd");
        r.update_topology(
            s.clone(),
            StoneTopology {
                gpus: vec![amd_16g(0)],
                memory_total_mb: Some(16384),
            },
        )
        .await;
        let g = r
            .claim(
                ClaimHolder::new("ollama", "stone-amd"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(2048),
                    required_stack: ComputeStack::Rocm,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let snap = r.snapshot(&s).await.unwrap();
        assert_eq!(snap.gpus[0].committed_mb, 2048);
        drop(g);
    }

    #[tokio::test]
    async fn two_sized_claims_compose() {
        let (r, s) = test_setup().await;
        let g1 = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let g2 = r
            .claim(
                ClaimHolder::new("ollama", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(2048),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let snap = r.snapshot(&s).await.unwrap();
        assert_eq!(snap.gpus[0].committed_mb, 8192);
        assert_eq!(snap.gpus[0].mode, DeviceMode::Shared);
        drop(g1);
        drop(g2);
    }

    #[tokio::test]
    async fn oversubscribe_rejected_with_available() {
        let (r, s) = test_setup().await;
        let _g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(20000),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        // 20000 + 4000 + 512 headroom = 24512 < 24576 — fits
        // Now try a 5000 MB claim — should reject
        let err = r
            .claim(
                ClaimHolder::new("ollama", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(5000),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        match err {
            ClaimError::InsufficientVram {
                requested,
                available,
                ..
            } => {
                assert_eq!(requested, 5000);
                // 24576 - 20000 - 512 = 4064
                assert_eq!(available, 4064);
            }
            other => panic!("expected InsufficientVram, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unsized_claim_forces_exclusive() {
        let (r, s) = test_setup().await;
        let _g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: None,
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        // Now try any claim — should be rejected as exclusive
        let err = r
            .claim(
                ClaimHolder::new("ollama", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(2048),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ClaimError::DeviceExclusivelyHeld { .. }));
    }

    #[tokio::test]
    async fn sized_blocks_unsized() {
        let (r, s) = test_setup().await;
        let _g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(2048),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let err = r
            .claim(
                ClaimHolder::new("ollama", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: None,
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ClaimError::DeviceExclusivelyHeld { .. }));
    }

    #[tokio::test]
    async fn release_decrements_committed() {
        let (r, s) = test_setup().await;
        let g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        assert_eq!(r.snapshot(&s).await.unwrap().gpus[0].committed_mb, 6144);
        g.release_now().await;
        assert_eq!(r.snapshot(&s).await.unwrap().gpus[0].committed_mb, 0);
    }

    #[tokio::test]
    async fn evict_holder_clears_claims() {
        let (r, s) = test_setup().await;
        let _g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        // Use Box::leak so the guard isn't dropped (which would
        // also release the claim)
        std::mem::forget(_g);
        assert_eq!(r.snapshot(&s).await.unwrap().gpus[0].committed_mb, 6144);
        r.evict_holder("comfyui", None).await;
        assert_eq!(r.snapshot(&s).await.unwrap().gpus[0].committed_mb, 0);
    }

    #[tokio::test]
    async fn soft_claim_does_not_block_hard() {
        let (r, s) = test_setup().await;
        let _soft = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Soft,
            )
            .await
            .unwrap();
        let _hard = r
            .claim(
                ClaimHolder::new("ollama", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(6144),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let snap = r.snapshot(&s).await.unwrap();
        // Only the hard claim contributes to committed_mb
        assert_eq!(snap.gpus[0].committed_mb, 6144);
        let pressure = r.pressure(&s).await.unwrap();
        assert_eq!(pressure.gpus[0].hard_claims, 1);
        assert_eq!(pressure.gpus[0].soft_claims, 1);
    }

    #[tokio::test]
    async fn unknown_device_returns_error() {
        let (r, s) = test_setup().await;
        let err = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 99,
                    vram_mb: Some(1024),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ClaimError::UnknownDevice { .. }));
    }

    #[tokio::test]
    async fn unknown_stone_returns_error() {
        let (r, _) = test_setup().await;
        let err = r
            .claim(
                ClaimHolder::new("comfyui", "no-stone"),
                ResourceRequest::Gpu {
                    stone: StoneName::new("ghost-stone"),
                    device: 0,
                    vram_mb: Some(1024),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ClaimError::UnknownStone(_)));
    }

    #[tokio::test]
    async fn pressure_snapshot_reports_state() {
        let (r, s) = test_setup().await;
        let _g = r
            .claim(
                ClaimHolder::new("comfyui", "stone-test"),
                ResourceRequest::Gpu {
                    stone: s.clone(),
                    device: 0,
                    vram_mb: Some(8192),
                    required_stack: ComputeStack::Cuda,
                },
                ClaimKind::Hard,
            )
            .await
            .unwrap();
        let pressure = r.pressure(&s).await.unwrap();
        assert_eq!(pressure.gpus[0].total_mb, Some(24576));
        assert_eq!(pressure.gpus[0].committed_mb, 8192);
        // 24576 - 8192 - 512 headroom = 15872
        assert_eq!(pressure.gpus[0].available_mb, Some(15872));
        assert_eq!(pressure.gpus[0].vendor, GpuVendor::Nvidia);
    }
}
