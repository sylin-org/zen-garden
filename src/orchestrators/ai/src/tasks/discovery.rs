//! Discovery task: find a stone via Koi, query topology for all AI service
//! instances, then subscribe to the Tools API SSE stream for real-time
//! adds/removes.
//!
//! # Discovery flow
//!
//! 1. **Resolve a stone** — explicit override -> cached tending -> Koi mDNS browse.
//! 2. **Topology query** — `GET /api/v1/garden/topology` on the tended stone.
//!    Returns ALL stones and their offerings (populated via UDP chirp).
//! 3. **Tools API stream** — subscribe to `GET /api/v1/garden/tools/stream` for
//!    real-time `tool.upsert` / `tool.remove` events.
//! 4. On stream failure -> clear tending, re-discover from step 1.
//!
//! Generalized from ollama-orchestrator discovery.rs — discovers ALL AI
//! offering types by iterating the topology and filtering SSE events.

use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::app_state::{AppState, TendedStone};
use crate::domain::types::*;

/// How often to re-query topology to catch stones the SSE stream missed.
const TOPOLOGY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Known AI offering names for topology/SSE filtering.
const AI_OFFERING_NAMES: &[&str] = &[
    "ollama",
    "ollama-cpu",
    "comfyui",
    "whispercpp",
    "speaches",
    "speaches-cpu",
    "openedai-speech",
    "openedai-speech-min",
    "infinity",
    "infinity-cpu",
    "libretranslate",
];

/// Check if a tool FQN belongs to an AI offering.
fn is_ai_offering(fqid: &str) -> bool {
    // fqid format: "offering:ollama" or "offering:ollama:instance"
    let name = fqid
        .strip_prefix("offering:")
        .unwrap_or(fqid)
        .split(':')
        .next()
        .unwrap_or(fqid);
    AI_OFFERING_NAMES.contains(&name)
}

/// Map an offering name from topology/SSE to an OfferingKind.
fn offering_kind_from_name(name: &str) -> Option<OfferingKind> {
    match name {
        "ollama" | "ollama-cpu" => Some(OfferingKind::Ollama),
        "comfyui" => Some(OfferingKind::ComfyUi),
        "whispercpp" => Some(OfferingKind::WhisperCpp),
        "speaches" | "speaches-cpu" => Some(OfferingKind::Speaches),
        "openedai-speech" | "openedai-speech-min" => Some(OfferingKind::OpenedaiSpeech),
        "infinity" | "infinity-cpu" => Some(OfferingKind::Infinity),
        "libretranslate" => Some(OfferingKind::LibreTranslate),
        _ => None,
    }
}

/// Run the discovery loop.
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
        if let Err(e) = discover_from_topology(&stone_endpoint, &state).await {
            tracing::warn!(error = %e, "topology query failed, falling back to SSE-only");
        }

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

        let state_sse = state.clone();
        let result = orchestrator_common::tools_stream::subscribe_tools_stream(
            &stone_endpoint,
            |fqid: &str| is_ai_offering(fqid),
            move |event| {
                handle_tool_event(state_sse.clone(), event);
            },
        )
        .await;

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

/// Handle a single tool event from the SSE stream.
fn handle_tool_event(state: AppState, event: orchestrator_common::tools_stream::ToolStreamEvent) {
    use orchestrator_common::tools_stream::ToolStreamEvent;

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
                    "SSE: offering not ready, skipping"
                );
                return;
            }

            let offering_name = tool_fqid
                .strip_prefix("offering:")
                .unwrap_or(&tool_fqid)
                .split(':')
                .next()
                .unwrap_or(&tool_fqid)
                .to_string();

            let kind = match offering_kind_from_name(&offering_name) {
                Some(k) => k,
                None => return,
            };

            tracing::info!(
                stone = %stone_name,
                offering = %offering_name,
                endpoint = %endpoint,
                "SSE: discovered AI instance"
            );

            tokio::spawn(async move {
                let endpoint = orchestrator_common::discovery::resolve_endpoint(
                    &state.koi_endpoint,
                    &endpoint,
                )
                .await;

                profile_and_register(state, stone_id, stone_name, endpoint, kind, 0, None).await;
            });
        }
        ToolStreamEvent::OfferingRemoved {
            stone_name,
            ..
        } => {
            tracing::info!(stone = %stone_name, "SSE: AI instance removed");
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

/// Query topology and discover all AI offering instances.
async fn discover_from_topology(
    stone_endpoint: &str,
    state: &AppState,
) -> anyhow::Result<()> {
    // Query topology for each known AI offering.
    // query_topology_for_offering filters by offering name; we call it per type.
    let mut all_stones = Vec::new();
    for &offering_name in AI_OFFERING_NAMES {
        match orchestrator_common::topology::query_topology_for_offering(
            stone_endpoint,
            offering_name,
        )
        .await
        {
            Ok(stones) => all_stones.extend(stones),
            Err(e) => {
                tracing::debug!(
                    offering = %offering_name,
                    error = %e,
                    "topology query failed for offering"
                );
            }
        }
    }

    tracing::info!(count = all_stones.len(), "topology: discovered AI instances");

    for entry in &all_stones {
        let offering_name = &entry.fqn.offering;
        let kind = match offering_kind_from_name(offering_name) {
            Some(k) => k,
            None => continue,
        };

        // TopologyOfferingStone.ip is a String (not Option), prefer over hostname.
        let endpoint = format!("http://{}:{}", entry.ip, entry.moss_port);

        let (vram_total, gpu_name) = entry
            .capabilities
            .as_ref()
            .and_then(|c| c.hardware.gpus.first())
            .map(|gpu| {
                let vram = gpu.vram_mb.unwrap_or(0) * 1_048_576; // MB → bytes
                let name = Some(gpu.model.clone());
                (vram, name)
            })
            .unwrap_or((0, None));

        tracing::info!(
            stone = %entry.stone_name,
            offering = %offering_name,
            endpoint = %endpoint,
            vram_mb = vram_total / 1_048_576,
            "topology: discovered AI instance"
        );

        let state = state.clone();
        let stone_id = entry.stone_id.clone();
        let stone_name = entry.stone_name.clone();
        tokio::spawn(async move {
            profile_and_register(state, stone_id, stone_name, endpoint, kind, vram_total, gpu_name)
                .await;
        });
    }

    Ok(())
}

/// Periodically re-query topology to catch missed discoveries and HW updates.
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

        for &offering_name in AI_OFFERING_NAMES {
            let stones = match orchestrator_common::topology::query_topology_for_offering(
                &stone_endpoint,
                offering_name,
            )
            .await
            {
                Ok(s) => s,
                Err(_) => continue,
            };

            for entry in &stones {
                let kind = match offering_kind_from_name(&entry.fqn.offering) {
                    Some(k) => k,
                    None => continue,
                };

                let endpoint = format!("http://{}:{}", entry.ip, entry.moss_port);

                let (vram_total, gpu_name) = entry
                    .capabilities
                    .as_ref()
                    .and_then(|c| c.hardware.gpus.first())
                    .map(|gpu| {
                        let vram = gpu.vram_mb.unwrap_or(0) * 1_048_576;
                        let name = Some(gpu.model.clone());
                        (vram, name)
                    })
                    .unwrap_or((0, None));

                let known = {
                    let instances = state.instances.read().await;
                    instances.contains_key(&endpoint)
                        || instances
                            .values()
                            .any(|i| i.stone.name == entry.stone_name)
                };

                if !known {
                    tracing::info!(
                        stone = %entry.stone_name,
                        offering = %offering_name,
                        endpoint = %endpoint,
                        "topology refresh: new AI instance"
                    );
                    let state = state.clone();
                    let stone_id = entry.stone_id.clone();
                    let stone_name = entry.stone_name.clone();
                    tokio::spawn(async move {
                        profile_and_register(
                            state, stone_id, stone_name, endpoint, kind, vram_total, gpu_name,
                        )
                        .await;
                    });
                } else if vram_total > 0 {
                    state
                        .update_instance_hw(&endpoint, vram_total, gpu_name)
                        .await;
                }
            }
        }
    }
}

/// Profile an AI service instance and register it in AppState.
async fn profile_and_register(
    state: AppState,
    stone_id: String,
    stone_name: String,
    endpoint: String,
    kind: OfferingKind,
    vram_total_bytes: u64,
    gpu_name: Option<String>,
) {
    let offering = match state.catalog.get(kind) {
        Some(o) => o.clone(),
        None => {
            tracing::debug!(
                offering = ?kind,
                "no adapter registered for offering type, skipping"
            );
            return;
        }
    };

    // Probe for health and metadata.
    let probe = match offering.probe(&endpoint).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                stone = %stone_name,
                endpoint = %endpoint,
                error = %e,
                "failed to probe instance"
            );
            let vram_budget = state.vram_budget_for(&stone_name, vram_total_bytes).await;
            let instance = ServiceInstance {
                stone: Stone {
                    id: stone_id,
                    name: stone_name,
                },
                endpoint,
                kind,
                gpu: Gpu {
                    name: gpu_name,
                    compute: if vram_total_bytes > 0 {
                        ComputeType::Gpu
                    } else {
                        ComputeType::Cpu
                    },
                },
                vram: Vram {
                    total_bytes: vram_total_bytes,
                    budget_bytes: vram_budget,
                    free_bytes: None,
                },
                health: InstanceHealth::Unhealthy {
                    since: Instant::now(),
                    reason: e.to_string(),
                },
                models_available: vec![],
                models_loaded: vec![],
                capabilities: offering.capabilities().to_vec(),
                queue_depth: 0,
                last_seen: Instant::now(),
                metadata: serde_json::Value::Null,
                priority: 0,
            };
            state.upsert_instance(instance).await;
            return;
        }
    };

    // Enumerate models/resources.
    let models = match offering.enumerate(&endpoint).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                stone = %stone_name,
                endpoint = %endpoint,
                error = %e,
                "enumerate failed, registering with probe data only"
            );
            vec![]
        }
    };

    let vram_budget = state.vram_budget_for(&stone_name, vram_total_bytes).await;

    let models_available: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
    let models_loaded: Vec<LoadedModel> = models
        .iter()
        .filter_map(|m| {
            m.vram_bytes.map(|vram| LoadedModel {
                name: m.name.clone(),
                vram_bytes: vram,
                expires_at: None,
            })
        })
        .collect();

    // Register model metadata.
    for m in &models {
        let info = ModelInfo {
            name: m.name.clone(),
            parameter_count: m.metadata.get("parameter_count").and_then(|v| v.as_u64()),
            parameter_size: m
                .metadata
                .get("parameter_size")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            quantization_level: m
                .metadata
                .get("quantization_level")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            family: m
                .metadata
                .get("family")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            families: vec![],
            capabilities: m.capabilities.iter().map(|c| c.to_string()).collect(),
            format: m
                .metadata
                .get("format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            size_disk: 0,
            vram_bytes: m.vram_bytes,
            context_length: m.metadata.get("context_length").and_then(|v| v.as_u64()),
        };
        state.upsert_model(info).await;
    }

    let instance = ServiceInstance {
        stone: Stone {
            id: stone_id,
            name: stone_name.clone(),
        },
        endpoint: endpoint.clone(),
        kind,
        gpu: Gpu {
            name: gpu_name,
            compute: if vram_total_bytes > 0 {
                ComputeType::Gpu
            } else {
                ComputeType::Cpu
            },
        },
        vram: Vram {
            total_bytes: vram_total_bytes,
            budget_bytes: vram_budget,
            free_bytes: probe.vram_free_bytes,
        },
        health: InstanceHealth::Healthy,
        models_available,
        models_loaded,
        capabilities: probe.capabilities,
        queue_depth: 0,
        last_seen: Instant::now(),
        metadata: probe.metadata,
        priority: 0,
    };

    state.upsert_instance(instance).await;
    tracing::info!(
        stone = %stone_name,
        offering = ?kind,
        "instance profiled and registered"
    );
}

/// Resolve a stone endpoint through the priority cascade.
async fn resolve_stone(state: &AppState, shutdown: &CancellationToken) -> Option<String> {
    // 1. Explicit override
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

    // 2. Cached tending
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
            tracing::warn!(stone = %stone.stone_name, "cached stone unreachable");
        }
    }
    state.clear_tending().await;

    // 3. Koi mDNS discovery
    loop {
        if shutdown.is_cancelled() {
            return None;
        }

        if !orchestrator_common::discovery::check_koi_health(&state.koi_endpoint).await {
            tracing::warn!(koi = %state.koi_endpoint, "Koi not reachable, retrying in 5s");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
                _ = shutdown.cancelled() => return None,
            }
        }

        tracing::info!(koi = %state.koi_endpoint, "discovering stones via Koi mDNS");

        match orchestrator_common::discovery::discover_stones(&state.koi_endpoint).await {
            Ok(stones) if stones.is_empty() => {
                tracing::warn!("no stones found, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Ok(stones) => {
                for stone in &stones {
                    let endpoint = stone.endpoint();
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
                tracing::warn!("no healthy stones found, retrying in 10s");
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
