//! Discovery task: find a stone via Koi, query topology for all Ollama
//! instances, then subscribe to the Tools API SSE stream for real-time
//! adds/removes.
//!
//! # Discovery flow
//!
//! 1. **Resolve a stone** — explicit override → cached tending → Koi mDNS browse.
//! 2. **Topology query** — `GET /api/v1/garden/topology` on the tended stone.
//!    This is the authoritative view of every stone and its offerings, populated
//!    via UDP chirp.  Unlike the Tools API SSE snapshot (which is eventually
//!    consistent), the topology returns every Ollama stone that is currently on
//!    the network — exactly what `garden-rake observe` uses.
//! 3. **Tools API stream** — subscribe to `GET /api/v1/garden/tools/stream` for
//!    real-time `tool.upsert` / `tool.remove` events so the orchestrator reacts
//!    to new Ollama instances coming online or going away after the initial load.
//! 4. On stream failure → clear tending, re-discover from step 1.

use crate::app_state::{AppState, TendedStone};
use crate::domain::types::{ComputeType, InstanceHealth, OllamaInstance};
use crate::infra::ollama_client::OllamaClient;
use crate::infra::stone_discovery;
use crate::infra::tools_stream::{self, ToolEvent};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How often to re-query the topology to catch stones the SSE stream missed.
const TOPOLOGY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Run the discovery loop. Resolves a stone, queries topology for all Ollama
/// instances, then subscribes to the Tools API for ongoing changes.
/// Reconnects and re-discovers on failure.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
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
        //
        // GET /api/v1/garden/topology returns ALL stones and their offerings
        // in one shot (populated by UDP chirp). This is the same data path
        // that `garden-rake observe` uses, and it sees every Ollama stone
        // immediately — unlike the Tools API SSE snapshot which only contains
        // tools that have propagated through the gossip layer.
        match stone_discovery::query_topology_ollama(&stone_endpoint).await {
            Ok(ollama_stones) => {
                tracing::info!(
                    count = ollama_stones.len(),
                    "topology returned Ollama stones, profiling all"
                );
                for topo_stone in &ollama_stones {
                    let endpoint = topo_stone.ollama_endpoint();
                    let moss_ep = topo_stone.moss_endpoint();
                    tracing::info!(
                        stone = %topo_stone.stone_name,
                        endpoint = %endpoint,
                        vram_mb = topo_stone.vram_total_bytes / 1_048_576,
                        gpu = ?topo_stone.gpu_name,
                        "discovered Ollama instance via topology"
                    );
                    let state = state.clone();
                    let client = client.clone();
                    let stone_id = topo_stone.stone_id.clone();
                    let stone_name = topo_stone.stone_name.clone();
                    let vram_total = topo_stone.vram_total_bytes;
                    let gpu_name = topo_stone.gpu_name.clone();
                    tokio::spawn(async move {
                        profile_instance(
                            state, client, stone_id, stone_name, endpoint,
                            Some(moss_ep), vram_total, gpu_name,
                        )
                        .await;
                    });
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "topology query failed, falling back to SSE-only");
            }
        }

        // ── Phase 3: SSE stream + periodic topology refresh ──────
        //
        // The SSE stream provides real-time events, but may not reliably
        // deliver all Ollama discoveries.  A parallel topology poll every
        // 30 s catches new stones and merges HW data updates.
        tracing::info!(endpoint = %stone_endpoint, "subscribing to Tools API stream + topology refresh");

        let state_clone = state.clone();
        let client_clone = client.clone();

        // Spawn periodic topology refresh alongside the SSE stream
        let refresh_handle = {
            let state = state.clone();
            let client = client.clone();
            let endpoint = stone_endpoint.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                topology_refresh_loop(endpoint, state, client, shutdown).await;
            })
        };

        let result = tools_stream::subscribe_tools_stream(&stone_endpoint, |event| {
            match event {
                ToolEvent::OllamaDiscovered {
                    stone_id,
                    stone_name,
                    endpoint,
                    ready,
                } => {
                    if !ready {
                        tracing::debug!(
                            stone = %stone_name,
                            endpoint = %endpoint,
                            "SSE: Ollama instance not ready (container stopped), skipping"
                        );
                        return;
                    }

                    tracing::info!(
                        stone = %stone_name,
                        endpoint = %endpoint,
                        "SSE: discovered Ollama instance, fetching HW caps"
                    );
                    let state = state_clone.clone();
                    let client = client_clone.clone();
                    tokio::spawn(async move {
                        // Resolve .local hostnames to IP via Koi (mDNS unreliable in Docker on Windows)
                        let endpoint = stone_discovery::resolve_endpoint(
                            &state.koi_endpoint, &endpoint,
                        ).await;
                        let stone_host = endpoint
                            .trim_start_matches("http://")
                            .split(':')
                            .next()
                            .unwrap_or("")
                            .to_string();
                        let moss_ep = format!("http://{}:{}", stone_host, garden_common::constants::MOSS_HTTP);
                        let (vram_total, gpu_name) =
                            stone_discovery::fetch_stone_hw(&stone_host).await;
                        tracing::info!(
                            stone = %stone_name,
                            vram_mb = vram_total / 1_048_576,
                            gpu = ?gpu_name,
                            "SSE: fetched HW capabilities"
                        );
                        profile_instance(
                            state, client, stone_id, stone_name, endpoint,
                            Some(moss_ep), vram_total, gpu_name,
                        )
                        .await;
                    });
                }
                ToolEvent::OllamaRemoved {
                    stone_id: _,
                    stone_name,
                } => {
                    tracing::info!(stone = %stone_name, "Ollama instance removed");
                    let state = state_clone.clone();
                    tokio::spawn(async move {
                        let endpoint = {
                            let instances = state.instances.read().await;
                            instances
                                .values()
                                .find(|i| i.stone_name == stone_name)
                                .map(|i| i.endpoint.clone())
                        };
                        if let Some(ep) = endpoint {
                            state.remove_instance(&ep).await;
                        }
                    });
                }
                ToolEvent::Heartbeat => {
                    tracing::trace!("tools stream heartbeat");
                }
            }
        })
        .await;

        // ── Stream ended — stop refresh loop and prepare for reconnect ─
        match result {
            Ok(()) => tracing::warn!("tools stream ended normally, will re-discover"),
            Err(e) => tracing::warn!(error = %e, "tools stream error, will re-discover"),
        }
        refresh_handle.abort();

        // Clear tending so we re-discover (the stone may have gone away)
        state.clear_tending().await;

        // Wait before reconnecting
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Periodically re-query the topology to catch stones the SSE stream missed.
///
/// For each topology stone:
/// - **New** (not in AppState) → profile and register it.
/// - **Existing with stale HW** (VRAM was 0 or GPU unknown) → merge real HW data.
/// - **Existing and current** → skip.
async fn topology_refresh_loop(
    stone_endpoint: String,
    state: AppState,
    client: OllamaClient,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(TOPOLOGY_REFRESH_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        match stone_discovery::query_topology_ollama(&stone_endpoint).await {
            Ok(ollama_stones) => {
                for topo_stone in &ollama_stones {
                    let endpoint = topo_stone.ollama_endpoint();

                    // Look up by endpoint first, then by stone identity
                    // (name/id).  A stone that rebooted with a new IP won't
                    // match its old endpoint but will match by name.
                    let (known, needs_hw_update, existing_endpoint) = {
                        let instances = state.instances.read().await;
                        let found = instances
                            .get(&endpoint)
                            .map(|inst| (endpoint.clone(), inst))
                            .or_else(|| {
                                instances.iter().find(|(_, inst)| {
                                    inst.stone_name == topo_stone.stone_name
                                        || (!topo_stone.stone_id.is_empty()
                                            && !inst.stone_id.is_empty()
                                            && inst.stone_id == topo_stone.stone_id)
                                }).map(|(ep, inst)| (ep.clone(), inst))
                            });
                        match found {
                            Some((ep, inst)) => {
                                let stale_hw = (inst.vram_total_bytes == 0
                                    && topo_stone.vram_total_bytes > 0)
                                    || (inst.gpu_name.is_none()
                                        && topo_stone.gpu_name.is_some());
                                let ip_changed = ep != endpoint;
                                (true, stale_hw, Some((ep, ip_changed)))
                            }
                            None => (false, false, None),
                        }
                    };

                    // Stone is known but its IP changed — re-profile at the
                    // new endpoint.  upsert_instance will evict the old entry.
                    let ip_changed = existing_endpoint
                        .as_ref()
                        .map(|(_, changed)| *changed)
                        .unwrap_or(false);

                    if known && ip_changed {
                        tracing::info!(
                            stone = %topo_stone.stone_name,
                            old_endpoint = %existing_endpoint.as_ref().unwrap().0,
                            new_endpoint = %endpoint,
                            "topology refresh: stone IP changed, re-profiling"
                        );
                        let state = state.clone();
                        let client = client.clone();
                        let stone_id = topo_stone.stone_id.clone();
                        let stone_name = topo_stone.stone_name.clone();
                        let vram_total = topo_stone.vram_total_bytes;
                        let gpu_name = topo_stone.gpu_name.clone();
                        let moss_ep = topo_stone.moss_endpoint();
                        tokio::spawn(async move {
                            profile_instance(
                                state, client, stone_id, stone_name, endpoint,
                                Some(moss_ep), vram_total, gpu_name,
                            )
                            .await;
                        });
                    } else if known && needs_hw_update {
                        let hw_endpoint = existing_endpoint
                            .map(|(ep, _)| ep)
                            .unwrap_or(endpoint);
                        tracing::info!(
                            stone = %topo_stone.stone_name,
                            vram_mb = topo_stone.vram_total_bytes / 1_048_576,
                            gpu = ?topo_stone.gpu_name,
                            "topology refresh: merging HW data"
                        );
                        state
                            .update_instance_hw(
                                &hw_endpoint,
                                topo_stone.vram_total_bytes,
                                topo_stone.gpu_name.clone(),
                            )
                            .await;
                    } else if !known {
                        tracing::info!(
                            stone = %topo_stone.stone_name,
                            endpoint = %endpoint,
                            vram_mb = topo_stone.vram_total_bytes / 1_048_576,
                            "topology refresh: new Ollama stone, profiling"
                        );
                        let state = state.clone();
                        let client = client.clone();
                        let stone_id = topo_stone.stone_id.clone();
                        let stone_name = topo_stone.stone_name.clone();
                        let vram_total = topo_stone.vram_total_bytes;
                        let gpu_name = topo_stone.gpu_name.clone();
                        let moss_ep = topo_stone.moss_endpoint();
                        tokio::spawn(async move {
                            profile_instance(
                                state, client, stone_id, stone_name, endpoint,
                                Some(moss_ep), vram_total, gpu_name,
                            )
                            .await;
                        });
                    }
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "topology refresh failed");
            }
        }
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
            if stone_discovery::check_stone_health(&stone.endpoint).await {
                tracing::info!(stone = %stone.stone_name, "cached stone is healthy");
                return Some(stone.endpoint.clone());
            }
            tracing::warn!(
                stone = %stone.stone_name,
                "cached stone unreachable, will re-discover"
            );
        }
    }
    // Drop read lock before clearing
    state.clear_tending().await;

    // ── 3. Koi mDNS discovery ────────────────────────────────────
    loop {
        if shutdown.is_cancelled() {
            return None;
        }

        // First, make sure Koi is reachable
        if !stone_discovery::check_koi_health(&state.koi_endpoint).await {
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

        match stone_discovery::discover_stones(&state.koi_endpoint).await {
            Ok(stones) if stones.is_empty() => {
                tracing::warn!("no stones found on the network, retrying in 10s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => continue,
                    _ = shutdown.cancelled() => return None,
                }
            }
            Ok(stones) => {
                // Pick the first healthy stone
                for stone in &stones {
                    let endpoint = stone.endpoint();
                    tracing::info!(
                        stone = %stone.stone_name,
                        endpoint = %endpoint,
                        "checking discovered stone health"
                    );
                    if stone_discovery::check_stone_health(&endpoint).await {
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

/// Profile a newly discovered Ollama instance.
///
/// Queries /api/tags, /api/ps, /api/show per model, then registers in AppState.
/// Also fetches Ollama environment from Moss to detect `OLLAMA_NUM_PARALLEL`.
#[allow(clippy::too_many_arguments)]
async fn profile_instance(
    state: AppState,
    client: OllamaClient,
    stone_id: String,
    stone_name: String,
    endpoint: String,
    moss_endpoint: Option<String>,
    topology_vram_bytes: u64,
    topology_gpu_name: Option<String>,
) {
    tracing::info!(
        stone = %stone_name,
        endpoint = %endpoint,
        topology_vram_mb = topology_vram_bytes / 1_048_576,
        "profiling instance"
    );

    // Fetch Ollama service env from Moss (best-effort, non-blocking)
    let num_parallel = if let Some(ref moss_ep) = moss_endpoint {
        let env = stone_discovery::fetch_service_env(moss_ep, "ollama").await;
        let np = env
            .get("OLLAMA_NUM_PARALLEL")
            .and_then(|v| v.parse::<u32>().ok());
        if let Some(n) = np {
            tracing::info!(stone = %stone_name, num_parallel = n, "detected OLLAMA_NUM_PARALLEL");
        }
        np
    } else {
        None
    };

    let profile = client.full_profile(&endpoint).await;

    match profile {
        Ok((models_available, models_loaded, model_infos, version)) => {
            // Use the real VRAM reported by the stone's hardware detection
            // (propagated via chirp → topology). Fall back to config budget.
            let vram_total = topology_vram_bytes;

            let vram_budget = state.vram_budget_for(&stone_name, vram_total).await;

            let gpu_name = topology_gpu_name;

            let instance = OllamaInstance {
                stone_id,
                stone_name: stone_name.clone(),
                endpoint: endpoint.clone(),
                moss_endpoint,
                ollama_version: version,
                gpu_name,
                vram_total_bytes: vram_total,
                vram_budget_bytes: vram_budget,
                num_parallel,
                compute_type: ComputeType::Gpu,
                health: InstanceHealth::Healthy,
                models_loaded,
                models_available,
                queue_depth: 0,
                last_seen: Instant::now(),
                last_profiled: Instant::now(),
            };

            // Register models
            for info in model_infos {
                state.upsert_model(info).await;
            }

            // Register instance (triggers tier recomputation)
            state.upsert_instance(instance).await;

            tracing::info!(stone = %stone_name, "instance profiled and added to routing pool");
        }
        Err(e) => {
            tracing::warn!(
                stone = %stone_name,
                endpoint = %endpoint,
                error = %e,
                "failed to profile instance"
            );
            // If the stone already exists in the registry, preserve its
            // hardware data and just mark it unhealthy.  Only create a
            // minimal placeholder when this is a genuinely new stone.
            let already_exists = {
                let instances = state.instances.read().await;
                instances.contains_key(&endpoint)
            };
            if already_exists {
                state
                    .set_instance_health(
                        &endpoint,
                        InstanceHealth::Unhealthy {
                            since: Instant::now(),
                            reason: e.to_string(),
                        },
                    )
                    .await;
            } else {
                // New stone we've never seen — register with topology HW
                // data so we don't lose the VRAM/GPU info.
                let vram_budget = state
                    .vram_budget_for(&stone_name, topology_vram_bytes)
                    .await;
                let instance = OllamaInstance {
                    stone_id,
                    stone_name,
                    endpoint,
                    moss_endpoint,
                    ollama_version: None,
                    gpu_name: topology_gpu_name,
                    vram_total_bytes: topology_vram_bytes,
                    vram_budget_bytes: vram_budget,
                    num_parallel,
                    compute_type: ComputeType::Gpu,
                    health: InstanceHealth::Unhealthy {
                        since: Instant::now(),
                        reason: e.to_string(),
                    },
                    models_loaded: vec![],
                    models_available: vec![],
                    queue_depth: 0,
                    last_seen: Instant::now(),
                    last_profiled: Instant::now(),
                };
                state.upsert_instance(instance).await;
            }
        }
    }
}
