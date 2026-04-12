use crate::api::responses::{GardenOverview, StoneInfo};
use crate::api::suggestions::{Suggestion, generate_suggestions};
use crate::domain::placement::{PlacementRequest, PlacementResponse};
use crate::{AppState, internal, not_found};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use garden_common::TopologyEntry;
use garden_common::resources::system as resources;
use garden_common::{
    CpuCapabilities, DetectionStatus, DiskCapabilities, HardwareCapabilities, HardwareInventory,
    MemoryCapabilities, RuntimeInfo,
};

/// GET /api/v1/garden - Get garden overview (all stones)
pub async fn get_garden_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<GardenOverview> {
    // Build stone list: self entry + all cached peers
    let self_entry = crate::domain::topology::composition::build_self_entry(&state).await;
    let cache_entries = state.topology.all_stones().await;

    let mut stones = Vec::new();
    let mut total_services: u32 = 0;
    let mut healthy_stones: u32 = 0;
    let mut degraded_stones: u32 = 0;

    // Get live resources for local stone
    let (cpu_usage, memory_usage) = match resources::get_fast_resources() {
        Ok((cpu, mem, _, _)) => (cpu.usage_percent, mem.used_percent),
        Err(_) => (0.0, 0.0),
    };

    // Add self first with live resources
    let self_info = topology_entry_to_stone_info_with_metrics(&self_entry, cpu_usage, memory_usage);
    total_services += self_info.services_count;
    if self_info.health == garden_common::constants::HEALTH_HEALTHY
        || self_info.health == garden_common::constants::VITALITY_THRIVING
    {
        healthy_stones += 1;
    } else {
        degraded_stones += 1;
    }
    stones.push(self_info);

    // Add all cached peers (skip self if present in cache)
    for entry in cache_entries {
        if entry.stone_id == state.current.stone.id {
            continue;
        }
        let info = topology_entry_to_stone_info(&entry);
        total_services += info.services_count;
        if info.health == garden_common::constants::HEALTH_HEALTHY
            || info.health == garden_common::constants::VITALITY_THRIVING
        {
            healthy_stones += 1;
        } else {
            degraded_stones += 1;
        }
        stones.push(info);
    }

    let overview = GardenOverview {
        stones,
        total_services,
        healthy_stones,
        degraded_stones,
        pond_status: None, // Phase 3
    };

    let ctx = Suggestion::from_headers(&headers, "observe_garden");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(overview, suggestions)
}

/// Convert TopologyEntry to StoneInfo for garden overview (peers - no live resources)
fn topology_entry_to_stone_info(entry: &TopologyEntry) -> StoneInfo {
    topology_entry_to_stone_info_with_metrics(entry, 0.0, 0.0)
}

/// Convert TopologyEntry to StoneInfo with live resources (for local stone)
fn topology_entry_to_stone_info_with_metrics(
    entry: &TopologyEntry,
    cpu_usage: f32,
    memory_usage: f32,
) -> StoneInfo {
    StoneInfo {
        name: entry.stone_name.clone(),
        endpoint: entry.address.http_base(),
        health: entry.health.clone(),
        services_count: entry.services.len() as u32,
        cpu_usage,
        memory_usage,
    }
}

/// GET /api/v1/garden/stones/:stone_name - Get specific stone details
pub async fn get_stone_v1(
    State(state): State<AppState>,
    Path(stone_name): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<HardwareCapabilities> {
    // For now, only support local stone
    if state.current.stone.name != stone_name {
        return Err(not_found(
            "STONE_NOT_FOUND",
            format!("Stone '{}' not found in garden", stone_name),
        ));
    }

    let caps = get_capabilities(&state).await;

    let ctx = Suggestion::from_headers(&headers, "observe_stone");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(caps, suggestions)
}

/// GET /api/v1/stone - Get local stone consolidated info
pub async fn get_local_stone_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<HardwareCapabilities> {
    let caps = get_capabilities(&state).await;

    let ctx = Suggestion::from_headers(&headers, "observe_stone");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(caps, suggestions)
}
/// POST /api/v1/garden/recommend - Get intelligent placement recommendation
pub async fn recommend_placement_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PlacementRequest>,
) -> crate::api::ApiResult<PlacementResponse> {
    match crate::domain::placement::recommend_placement(request.clone(), &state).await {
        Ok(response) => {
            let ctx = Suggestion::from_headers(&headers, "placement_success");
            let suggestions = generate_suggestions(&ctx);

            crate::api::ok_maybe(response, suggestions)
        }
        Err(e) => {
            tracing::error!(
                offering = %request.offering,
                error = ?e,
                "Placement recommendation failed"
            );

            Err(internal(
                "PLACEMENT_ERROR",
                format!("Failed to generate placement recommendation: {}", e),
            ))
        }
    }
}
// Helper function to build consolidated capabilities (based on main.rs capabilities handler)
async fn get_capabilities(state: &AppState) -> HardwareCapabilities {
    let (cpu_model, cpu_features, architecture) = resources::get_cpu_info().unwrap_or_else(|_| {
        (
            "Unknown".to_string(),
            vec![],
            std::env::consts::ARCH.to_string(),
        )
    });

    let resources = resources::collect_stone_resources().ok();
    let total_memory_mb = resources
        .as_ref()
        .map(|r| r.memory.total_bytes / 1024 / 1024)
        .unwrap_or(0);

    let gpus = resources::detect_gpus();

    let disk = resources.as_ref().map(|r| DiskCapabilities {
        total_gb: r
            .storage
            .iter()
            .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
            .or_else(|| r.storage.iter().max_by_key(|s| s.total_gb))
            .map(|s| s.total_gb)
            .unwrap_or(0),
        disk_type: r
            .storage
            .iter()
            .find(|s| s.mount_point == "/" || s.mount_point == "C:\\")
            .or_else(|| r.storage.iter().max_by_key(|s| s.total_gb))
            .map(|s| match &s.disk_type {
                garden_common::DiskType::NVMe => "NVMe".to_string(),
                garden_common::DiskType::SSD => "SSD".to_string(),
                garden_common::DiskType::HDD => "HDD".to_string(),
                garden_common::DiskType::Unknown => "Unknown".to_string(),
            }),
    });

    let cores = resources.as_ref().map(|r| r.cpu.cores).unwrap_or(1);

    let os_version = resources::detect_os_version();
    let kernel_version = resources::detect_kernel_version();
    let swap_mb = resources::detect_swap();
    let docker_version = state.platform.docker.get_docker_version().await.ok();

    HardwareCapabilities {
        stone_id: Some(state.current.stone.id.clone()),
        stone_name: state.current.stone.name.clone(),
        hardware: HardwareInventory {
            cpu: CpuCapabilities {
                model: if cpu_model == "Unknown" {
                    None
                } else {
                    Some(cpu_model)
                },
                cores,
                threads: None,
                architecture,
                features: if cpu_features.is_empty() {
                    None
                } else {
                    Some(cpu_features)
                },
            },
            memory: MemoryCapabilities {
                total_mb: total_memory_mb,
            },
            gpus,
            disk,
            swap_mb,
            ai_capabilities: None,
            system_manufacturer: None,
            system_product: None,
        },
        runtime: Some(RuntimeInfo {
            docker_version,
            os: format!(
                "{}/{}",
                std::env::consts::OS,
                os_version.unwrap_or_else(|| "Unknown".to_string())
            ),
            kernel: kernel_version,
        }),
        detection_status: DetectionStatus::Complete, // Synchronous detection
    }
}

// === TOPOLOGY API ===

/// GET /api/v1/garden/topology - Get all known stones in the garden
///
/// Returns all stones as TopologyEntry objects: self entry first, then peers from cache.
/// No conversion needed - TopologyEntry is the universal model.
pub async fn get_topology_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> crate::api::ApiResult<Vec<TopologyEntry>> {
    // Step 1: Build self entry on demand from source domains
    let self_entry = crate::domain::topology::composition::build_self_entry(&state).await;

    tracing::debug!(
        stone_id = %self_entry.stone_id,
        stone_name = %self_entry.stone_name,
        services = self_entry.services.len(),
        health = %self_entry.health,
        "Topology: self entry prepared"
    );

    // Step 2: Start response with self entry first
    let mut stones = vec![self_entry.clone()];

    // Step 3: Add all cached peer stones (skipping self if present)
    let cache_entries = state.topology.all_stones().await;

    for entry in cache_entries {
        if entry.stone_id == state.current.stone.id {
            tracing::debug!(
                cached_stone_id = %entry.stone_id,
                "Topology: skipping self from cache"
            );
            continue;
        }

        stones.push(entry);
    }

    tracing::debug!(total_stones = stones.len(), "Topology: response built");

    let ctx = Suggestion::from_headers(&headers, "topology_query");
    let suggestions = generate_suggestions(&ctx);

    crate::api::ok_maybe(stones, suggestions)
}

/// GET /api/v1/garden/inspect — Garden-wide hardware inspection with fan-out.
///
/// Queries every peer's `/api/v1/stone/capabilities` in parallel, collects
/// `FullCapabilities` for each reachable stone (including self), and returns
/// a `GardenInspection` summary.
pub async fn inspect_garden_v1(
    State(state): State<AppState>,
) -> crate::api::ApiResult<garden_common::types::hardware_topology::GardenInspection> {
    use garden_common::api_utils::responses::ApiResponse as CommonApiResponse;
    use garden_common::constants::timeouts::garden_inspect_timeout;
    use garden_common::types::hardware_topology::{
        FullCapabilities, GardenInspection, InspectionSummary, StoneInspection, UnreachableStone,
    };

    let mut stones: Vec<StoneInspection> = Vec::new();
    let mut unreachable: Vec<UnreachableStone> = Vec::new();

    // ── Self (local stone) ──────────────────────────────────────────
    let self_core = {
        let guard = state.current.capabilities.read().await;
        guard.clone().unwrap_or_else(|| {
            crate::infra::hardware::create_skeleton(state.current.stone.name.to_string())
        })
    };
    let self_topology = state.current.hardware_topology.read().await.clone();
    let self_address = state.current.address.read().await.http_base();

    stones.push(StoneInspection {
        name: state.current.stone.name.clone(),
        id: state.current.stone.id.clone(),
        endpoint: self_address,
        capabilities: FullCapabilities {
            core: self_core,
            topology: self_topology,
        },
    });

    // ── Peers (parallel fan-out) ────────────────────────────────────
    let peers = state.topology.all_stones().await;
    let timeout = garden_inspect_timeout();

    let tasks: Vec<_> = peers
        .into_iter()
        .filter(|e| e.stone_id != state.current.stone.id)
        .map(|entry| {
            let endpoint = entry.address.http_base();
            let stone_name = entry.stone_name.clone();
            let stone_id = entry.stone_id.clone();
            let client = crate::http::HTTP.clone();

            tokio::spawn(async move {
                let url = format!(
                    "{}/api/v1/stone/capabilities",
                    endpoint.trim_end_matches('/')
                );
                let result = tokio::time::timeout(timeout, client.get(&url).send()).await;

                match result {
                    Ok(Ok(resp)) if resp.status().is_success() => {
                        match resp.json::<CommonApiResponse<FullCapabilities>>().await {
                            Ok(api_resp) => Ok(StoneInspection {
                                name: stone_name,
                                id: stone_id,
                                endpoint,
                                capabilities: api_resp.data,
                            }),
                            Err(e) => Err(UnreachableStone {
                                name: stone_name,
                                endpoint,
                                reason: format!("parse error: {e}"),
                            }),
                        }
                    }
                    Ok(Ok(resp)) => Err(UnreachableStone {
                        name: stone_name,
                        endpoint,
                        reason: format!("HTTP {}", resp.status()),
                    }),
                    Ok(Err(e)) => Err(UnreachableStone {
                        name: stone_name,
                        endpoint,
                        reason: format!("connection error: {e}"),
                    }),
                    Err(_) => Err(UnreachableStone {
                        name: stone_name,
                        endpoint,
                        reason: "timeout".to_string(),
                    }),
                }
            })
        })
        .collect();

    for task in tasks {
        match task.await {
            Ok(Ok(inspection)) => stones.push(inspection),
            Ok(Err(unreachable_stone)) => unreachable.push(unreachable_stone),
            Err(e) => {
                tracing::warn!(error = ?e, "Inspect task panicked");
            }
        }
    }

    let total = stones.len() + unreachable.len();
    let inspection = GardenInspection {
        inspected_at: chrono::Utc::now().to_rfc3339(),
        summary: InspectionSummary {
            total,
            inspected: stones.len(),
            unreachable: unreachable.len(),
        },
        stones,
        unreachable,
    };

    crate::api::ok(inspection)
}

/// GET /api/v1/garden/capabilities — Aggregate capabilities across all stones.
///
/// Returns `FullCapabilities` for each stone in the garden:
/// - This stone: full Tier 1 + Tier 2 (topology available locally).
/// - Peer stones: Tier 1 from topology cache, Tier 2 = None
///   (peers' Tier 2 data is local to each stone — hit their
///   `/capabilities` endpoint directly for full data).
pub async fn get_garden_capabilities_v1(
    State(state): State<AppState>,
) -> crate::api::ApiResult<Vec<garden_common::types::hardware_topology::FullCapabilities>> {
    use garden_common::types::hardware_topology::FullCapabilities;

    let mut results = Vec::new();

    // Self — full Tier 1 + Tier 2
    let self_core = {
        let guard = state.current.capabilities.read().await;
        guard.clone().unwrap_or_else(|| {
            crate::infra::hardware::create_skeleton(state.current.stone.name.to_string())
        })
    };
    let self_topology = state.current.hardware_topology.read().await.clone();
    results.push(FullCapabilities {
        core: self_core,
        topology: self_topology,
    });

    // Peers — Tier 1 from topology cache, Tier 2 = None.
    //
    // The cached `HardwareCapabilities` may have been populated from
    // a chirp or probe that didn't carry the peer's stone_name, so
    // `caps.stone_name` can be empty even though the topology entry
    // has the name. Overlay the topology entry's stone_name /
    // stone_id onto the caps before returning so downstream
    // consumers (e.g. the AI orchestrator's Resources domain
    // keyed by stone name) get a non-colliding identity per stone.
    let peers = state.topology.all_stones().await;
    for peer in peers {
        if peer.stone_id == state.current.stone.id {
            continue;
        }
        if let Some(mut caps) = peer.capabilities {
            if caps.stone_name.is_empty() {
                caps.stone_name = peer.stone_name.clone();
            }
            if caps.stone_id.as_deref().is_none_or(str::is_empty) {
                caps.stone_id = Some(peer.stone_id.clone());
            }
            results.push(FullCapabilities {
                core: caps,
                topology: None,
            });
        }
    }

    crate::api::ok(results)
}
