//! Registry domain — instances, tiers, queue depths (ORCH-0020).
//!
//! Owns the mutable state for service instances. Publishes immutable
//! snapshots via `watch`. API handlers read snapshots with zero locks.

use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

use super::tiering;
use super::types::*;

// ── Snapshot ───────────────────────────────────────────────────

/// Immutable view of the registry, published atomically.
#[derive(Debug, Clone)]
pub struct RegistrySnapshot {
    pub instances: Arc<HashMap<String, ServiceInstance>>,
    pub tiers: Arc<Vec<Tier>>,
    pub queue_counters: Arc<HashMap<String, Arc<AtomicU32>>>,
}

impl RegistrySnapshot {
    pub fn empty() -> Self {
        Self {
            instances: Arc::new(HashMap::new()),
            tiers: Arc::new(Vec::new()),
            queue_counters: Arc::new(HashMap::new()),
        }
    }
}

// ── Domain ─────────────────────────────────────────────────────

struct RegistryState {
    instances: HashMap<String, ServiceInstance>,
    queue_counters: HashMap<String, Arc<AtomicU32>>,
}

pub struct RegistryDomain {
    state: Mutex<RegistryState>,
    tx: watch::Sender<Arc<RegistrySnapshot>>,
}

impl RegistryDomain {
    pub fn new(tx: watch::Sender<Arc<RegistrySnapshot>>) -> Self {
        Self {
            state: Mutex::new(RegistryState {
                instances: HashMap::new(),
                queue_counters: HashMap::new(),
            }),
            tx,
        }
    }

    /// Borrow the latest snapshot (atomic load, zero locks).
    pub fn snapshot(&self) -> watch::Ref<'_, Arc<RegistrySnapshot>> {
        self.tx.borrow()
    }

    /// Subscribe for reactive consumers (Intelligence loop).
    pub fn subscribe(&self) -> watch::Receiver<Arc<RegistrySnapshot>> {
        self.tx.subscribe()
    }

    // ── Mutations ──────────────────────────────────────────────

    pub async fn upsert_instance(&self, mut instance: ServiceInstance, config: &OrchestratorConfig) {
        let mut state = self.state.lock().await;

        let endpoint = instance.endpoint.clone();
        let stone_name = instance.stone.name.clone();
        let stone_id = instance.stone.id.clone();

        // Ensure queue counter exists
        state
            .queue_counters
            .entry(endpoint.clone())
            .or_insert_with(|| Arc::new(AtomicU32::new(0)));

        // Evict stale instance for same stone + offering kind at different endpoint
        let stale_endpoint = state
            .instances
            .iter()
            .find(|(ep, existing)| {
                *ep != &endpoint
                    && existing.kind == instance.kind
                    && (existing.stone.name == stone_name
                        || (!stone_id.is_empty()
                            && !existing.stone.id.is_empty()
                            && existing.stone.id == stone_id))
            })
            .map(|(ep, _)| ep.clone());

        if let Some(ref stale_ep) = stale_endpoint {
            tracing::info!(
                stone = %stone_name,
                old_endpoint = %stale_ep,
                new_endpoint = %endpoint,
                "evicting stale instance (IP changed)"
            );
            state.queue_counters.remove(stale_ep);
        }

        // Preserve known HW data when incoming has zeroes
        let donor = stale_endpoint
            .as_ref()
            .and_then(|ep| state.instances.remove(ep))
            .or_else(|| state.instances.get(&endpoint).cloned());

        if let Some(existing) = donor {
            if instance.vram.total_bytes == 0 && existing.vram.total_bytes > 0 {
                instance.vram.total_bytes = existing.vram.total_bytes;
                instance.vram.budget_bytes = existing.vram.budget_bytes;
            }
            if instance.gpu.name.is_none() && existing.gpu.name.is_some() {
                instance.gpu.name = existing.gpu.name;
            }
        }

        // Apply VRAM budget from config
        if let Some(budget_mb) = config.stones.get(&stone_name).and_then(|s| s.vram_budget_mb) {
            instance.vram.budget_bytes = budget_mb * 1_048_576;
        }

        state.instances.insert(endpoint, instance);

        self.publish(&state);
    }

    pub async fn remove_instance(&self, endpoint: &str) {
        let mut state = self.state.lock().await;
        state.instances.remove(endpoint);
        state.queue_counters.remove(endpoint);
        self.publish(&state);
    }

    /// Remove instances whose endpoints are not in the given set.
    /// Returns the number of evicted instances.
    pub async fn evict_stale(&self, live_endpoints: &std::collections::HashSet<String>) -> usize {
        let mut state = self.state.lock().await;
        let stale: Vec<String> = state.instances.keys()
            .filter(|ep| !live_endpoints.contains(*ep))
            .cloned()
            .collect();

        let count = stale.len();
        for ep in &stale {
            if let Some(inst) = state.instances.remove(ep) {
                state.queue_counters.remove(ep);
                tracing::info!(
                    endpoint = %ep,
                    stone = %inst.stone.name,
                    kind = %inst.kind,
                    "evicted stale instance — no longer in topology"
                );
            }
        }

        if count > 0 {
            self.publish(&state);
        }
        count
    }

    pub async fn set_instance_health(&self, endpoint: &str, health: InstanceHealth) -> bool {
        let mut state = self.state.lock().await;
        if let Some(inst) = state.instances.get_mut(endpoint) {
            if inst.health != health {
                inst.health = health;
                self.publish(&state);
                return true;
            }
        }
        false
    }

    pub async fn update_instance_models(
        &self,
        endpoint: &str,
        available: Vec<String>,
        loaded: Vec<LoadedModel>,
    ) {
        let mut state = self.state.lock().await;
        if let Some(inst) = state.instances.get_mut(endpoint) {
            inst.models_available = available;
            inst.models_loaded = loaded;
            self.publish(&state);
        }
    }

    pub async fn update_instance_capabilities(
        &self,
        endpoint: &str,
        capabilities: Vec<Capability>,
        version: Option<String>,
    ) {
        let mut state = self.state.lock().await;
        if let Some(inst) = state.instances.get_mut(endpoint) {
            inst.capabilities = capabilities;
            if let Some(ref v) = version {
                inst.metadata = serde_json::json!({ "version": v });
            }
            // No publish needed — capabilities don't affect routing snapshot
            // The next upsert/health change will publish
        }
    }

    pub async fn update_instance_hw(
        &self,
        endpoint: &str,
        vram_total: u64,
        gpu_name: Option<String>,
        config: &OrchestratorConfig,
    ) {
        let mut state = self.state.lock().await;
        if let Some(inst) = state.instances.get_mut(endpoint) {
            if vram_total > 0 {
                inst.vram.total_bytes = vram_total;
                let stone_name = &inst.stone.name;
                inst.vram.budget_bytes = config
                    .stones
                    .get(stone_name)
                    .and_then(|s| s.vram_budget_mb)
                    .map(|mb| mb * 1_048_576)
                    .unwrap_or(vram_total);
            }
            if gpu_name.is_some() {
                inst.gpu.name = gpu_name;
            }
            self.publish(&state);
        }
    }

    pub async fn queue_counter(&self, endpoint: &str) -> Arc<AtomicU32> {
        let state = self.state.lock().await;
        state
            .queue_counters
            .get(endpoint)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicU32::new(0)))
    }

    // ── Publish ────────────────────────────────────────────────

    fn publish(&self, state: &RegistryState) {
        let tiers = tiering::compute_tiers(&state.instances);

        let snapshot = Arc::new(RegistrySnapshot {
            instances: Arc::new(state.instances.clone()),
            tiers: Arc::new(tiers),
            queue_counters: Arc::new(state.queue_counters.clone()),
        });

        self.tx.send_modify(|current| *current = snapshot);
    }
}
