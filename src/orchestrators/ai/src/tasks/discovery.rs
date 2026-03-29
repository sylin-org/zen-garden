//! Multi-offering discovery task.
//!
//! Discovers ALL AI offering instances across the garden, not just one type.
//!
//! # Discovery flow
//!
//! 1. **Resolve a stone** — explicit override -> cached tending -> Koi mDNS browse.
//! 2. **Topology query** — `GET /api/v1/garden/topology` on the tended stone.
//!    Parses every stone's services and registers those matching any known AI
//!    offering type (ollama, comfyui, speaches, infinity, etc.).
//! 3. **Tools API stream** — subscribe to `GET /api/v1/garden/tools/stream` for
//!    real-time `tool.upsert` / `tool.remove` events so the orchestrator reacts
//!    to new AI instances coming online or going away after the initial load.
//! 4. On stream failure -> clear tending, re-discover from step 1.

use crate::app_state::{AppState, TendedStone};
use crate::domain::types::{
    ComputeType, Gpu, InstanceHealth, OfferingKind, ServiceInstance, Stone, Vram,
};
use orchestrator_common::tools_stream::{self, ToolStreamEvent};
use orchestrator_common::topology;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How often to re-query the topology to catch stones the SSE stream missed.
const TOPOLOGY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Run the multi-offering discovery loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        if shutdown.is_cancelled() {
            return;
        }

        // ── Phase 1: Resolve a stone endpoint ────────────────────
        let stone_endpoint = match resolve_stone(&state, &shutdown).await {
            Some(ep) => ep,
            None => return,
        };

        // ── Phase 2: Topology query — authoritative initial load ─
        discover_from_topology(&stone_endpoint, &state).await;

        // ── Phase 3: SSE stream + periodic topology refresh ──────
        tracing::info!(
            endpoint = %stone_endpoint,
            "subscribing to Tools API stream + topology refresh"
        );

        let refresh_handle = {
            let state = state.clone();
            let endpoint = stone_endpoint.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                topology_refresh_loop(endpoint, state, shutdown).await;
            })
        };

        let state_for_stream = state.clone();
        let result = tools_stream::subscribe_tools_stream(
            &stone_endpoint,
            |fqid| is_ai_offering_fqid(fqid),
            |event| {
                handle_tool_event(&state_for_stream, event);
            },
        )
        .await;

        // ── Stream ended — stop refresh loop and reconnect ───────
        match result {
            Ok(()) => tracing::warn!("tools stream ended normally, will re-discover"),
            Err(e) => tracing::warn!(error = %e, "tools stream error, will re-discover"),
        }
        refresh_handle.abort();

        state.clear_tending().await;

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Check if a tool FQID matches any AI offering type.
fn is_ai_offering_fqid(fqid: &str) -> bool {
    OfferingKind::LOCAL_OFFERING_NAMES
        .iter()
        .any(|name| fqid.starts_with(&format!("offering:{name}")))
}

/// Handle a single tool stream event.
fn handle_tool_event(state: &AppState, event: ToolStreamEvent) {
    match event {
        ToolStreamEvent::OfferingDiscovered {
            stone_id,
            stone_name,
            endpoint,
            tool_fqid,
            ready,
        } => {
            if !ready {
                tracing::debug!(
                    stone = %stone_name,
                    fqid = %tool_fqid,
                    "SSE: AI instance not ready (container stopped), skipping"
                );
                return;
            }

            // Parse offering kind from the tool FQID (e.g. "offering:ollama" -> Ollama)
            let offering_name = tool_fqid
                .strip_prefix("offering:")
                .unwrap_or(&tool_fqid)
                .split(':')
                .next()
                .unwrap_or("");

            let kind = match OfferingKind::from_topology_name(offering_name) {
                Some(k) => k,
                None => {
                    tracing::debug!(fqid = %tool_fqid, "SSE: unrecognized AI offering, skipping");
                    return;
                }
            };

            tracing::info!(
                stone = %stone_name,
                kind = %kind,
                endpoint = %endpoint,
                "SSE: discovered AI instance"
            );

            let state = state.clone();
            tokio::spawn(async move {
                let instance = build_instance_from_discovery(
                    stone_id,
                    stone_name,
                    endpoint,
                    kind,
                    0,
                    None,
                );
                state.upsert_instance(instance).await;
            });
        }
        ToolStreamEvent::OfferingRemoved {
            stone_id: _,
            stone_name,
        } => {
            tracing::info!(stone = %stone_name, "SSE: AI instance removed");
            let state = state.clone();
            tokio::spawn(async move {
                let endpoint = {
                    let instances = state.instances.read().await;
                    instances
                        .values()
                        .find(|i| i.stone.name == stone_name)
                        .map(|i| i.endpoint.clone())
                };
                if let Some(ep) = endpoint {
                    state.remove_instance(&ep).await;
                }
            });
        }
        ToolStreamEvent::Heartbeat => {
            tracing::trace!("tools stream heartbeat");
        }
    }
}

/// Query topology for all AI offering instances and register them.
async fn discover_from_topology(stone_endpoint: &str, state: &AppState) {
    for offering_name in OfferingKind::LOCAL_OFFERING_NAMES {
        match topology::query_topology_for_offering(stone_endpoint, offering_name).await {
            Ok(stones) => {
                if stones.is_empty() {
                    continue;
                }
                let kind = match OfferingKind::from_topology_name(offering_name) {
                    Some(k) => k,
                    None => continue,
                };
                tracing::info!(
                    count = stones.len(),
                    offering = %offering_name,
                    "topology: discovered AI instances"
                );
                for topo_stone in &stones {
                    let (vram_total, gpu_name) = extract_hw_from_caps(&topo_stone.capabilities);

                    let service_port = kind.default_service_port().unwrap_or(0);
                    let endpoint = format!(
                        "http://{}:{}",
                        topo_stone.ip, service_port
                    );

                    tracing::info!(
                        stone = %topo_stone.stone_name,
                        kind = %kind,
                        endpoint = %endpoint,
                        vram_mb = vram_total / 1_048_576,
                        gpu = ?gpu_name,
                        "topology: registering AI instance"
                    );

                    let instance = build_instance_from_discovery(
                        topo_stone.stone_id.clone(),
                        topo_stone.stone_name.clone(),
                        endpoint,
                        kind,
                        vram_total,
                        gpu_name,
                    );
                    state.upsert_instance(instance).await;
                }
            }
            Err(e) => {
                tracing::debug!(
                    offering = %offering_name,
                    error = %e,
                    "topology query failed for offering"
                );
            }
        }
    }
}

/// Extract VRAM total bytes and GPU name from hardware capabilities.
fn extract_hw_from_caps(
    caps: &Option<garden_common::types::HardwareCapabilities>,
) -> (u64, Option<String>) {
    let Some(caps) = caps else {
        return (0, None);
    };

    let vram_mb = caps
        .hardware
        .ai_capabilities
        .as_ref()
        .map(|ai| ai.total_vram_mb)
        .unwrap_or(0);

    let gpu_name = caps.hardware.gpus.first().map(|g| g.model.clone());

    (vram_mb * 1_048_576, gpu_name)
}

/// Build a `ServiceInstance` from discovery data with defaults.
fn build_instance_from_discovery(
    stone_id: String,
    stone_name: String,
    endpoint: String,
    kind: OfferingKind,
    vram_total_bytes: u64,
    gpu_name: Option<String>,
) -> ServiceInstance {
    let priority = if kind.is_cloud() { -10 } else { 0 };
    let compute = if vram_total_bytes > 0 {
        ComputeType::Gpu
    } else {
        ComputeType::Cpu
    };

    ServiceInstance {
        stone: Stone {
            id: stone_id,
            name: stone_name,
        },
        endpoint,
        kind,
        gpu: Gpu {
            name: gpu_name,
            compute,
        },
        vram: Vram {
            total_bytes: vram_total_bytes,
            budget_bytes: vram_total_bytes,
            free_bytes: None,
        },
        health: InstanceHealth::Profiling,
        models_available: vec![],
        models_loaded: vec![],
        capabilities: vec![],
        queue_depth: 0,
        last_seen: Instant::now(),
        metadata: serde_json::Value::Null,
        priority,
    }
}

/// Periodically re-query the topology to catch stones the SSE stream missed.
async fn topology_refresh_loop(
    stone_endpoint: String,
    state: AppState,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(TOPOLOGY_REFRESH_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        discover_from_topology(&stone_endpoint, &state).await;
    }
}

/// Resolve a stone endpoint through the priority cascade:
///
/// 1. Explicit `--stone` / `GARDEN_STONE` override
/// 2. Cached tending (persisted `.tending` file, validated via health check)
/// 3. Koi mDNS discovery (browse for `_moss._tcp`, pick first healthy stone)
///
/// Returns `None` only if shutdown is requested.
async fn resolve_stone(state: &AppState, shutdown: &CancellationToken) -> Option<String> {
    // ── 1. Explicit stone override ───────────────────────────────
    if let Some(ref explicit) = state.explicit_stone {
        tracing::info!(endpoint = %explicit, "using explicit stone override");
        let tended = TendedStone {
            stone_name: "explicit".to_string(),
            stone_id: None,
            endpoint: explicit.clone(),
            last_seen: chrono::Utc::now(),
        };
        state.tend_to(tended).await;
        return Some(explicit.clone());
    }

    // ── 2. Cached tending state ──────────────────────────────────
    {
        let tended = state.tended_stone.read().await;
        if let Some(ref stone) = *tended {
            tracing::info!(
                stone = %stone.stone_name,
                endpoint = %stone.endpoint,
                "checking cached tending state"
            );
            if orchestrator_common::discovery::check_stone_health(&stone.endpoint).await {
                tracing::info!(stone = %stone.stone_name, "cached stone is healthy");
                return Some(stone.endpoint.clone());
            }
            tracing::warn!(
                stone = %stone.stone_name,
                "cached stone unreachable, will re-discover"
            );
        }
    }
    state.clear_tending().await;

    // ── 3. Koi mDNS discovery ────────────────────────────────────
    loop {
        if shutdown.is_cancelled() {
            return None;
        }

        if !orchestrator_common::discovery::check_koi_health(&state.koi_endpoint).await {
            tracing::warn!(
                koi = %state.koi_endpoint,
                "Koi not reachable, retrying in 5s"
            );
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
                _ = shutdown.cancelled() => return None,
            }
        }

        tracing::info!(koi = %state.koi_endpoint, "discovering stones via Koi mDNS");

        match orchestrator_common::discovery::discover_stones(&state.koi_endpoint).await {
            Ok(stones) if stones.is_empty() => {
                tracing::warn!("no stones found on the network, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Ok(stones) => {
                for stone in &stones {
                    let endpoint = stone.endpoint();
                    tracing::info!(
                        stone = %stone.stone_name,
                        endpoint = %endpoint,
                        "checking discovered stone health"
                    );
                    if orchestrator_common::discovery::check_stone_health(&endpoint).await {
                        let tended = TendedStone {
                            stone_name: stone.stone_name.clone(),
                            stone_id: stone.stone_id.clone(),
                            endpoint: endpoint.clone(),
                            last_seen: chrono::Utc::now(),
                        };
                        state.tend_to(tended).await;
                        return Some(endpoint);
                    }
                }
                tracing::warn!(
                    "discovered {} stone(s) but none are healthy, retrying in 10s",
                    stones.len()
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "mDNS discovery failed, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
        }
    }
}
