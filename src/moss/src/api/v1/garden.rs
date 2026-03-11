use crate::api::responses::{ApiResponse, GardenOverview, StoneInfo};
use crate::api::suggestions::{generate_suggestions, Suggestion};
use crate::domain::{
    placement::{PlacementRequest, PlacementResponse},
    topology, TopologyEntry,
};
use crate::{error_response, AppState};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use garden_common::metrics::system as metrics;
use garden_common::{
    api_utils::ApiErrorResponse, CpuCapabilities, DetectionStatus, DiskCapabilities,
    HardwareCapabilities, HardwareInventory, MemoryCapabilities, RuntimeInfo,
};

/// GET /api/v1/garden - Get garden overview (all stones)
pub async fn get_garden_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<GardenOverview>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Build stone list: self entry + all cached peers
    let self_entry = state.current.topology.self_entry.read().await.clone();
    let cache_entries = topology::get_all_stones(&state.current.topology.cache).await;

    let mut stones = Vec::new();
    let mut total_services: u32 = 0;
    let mut healthy_stones: u32 = 0;
    let mut degraded_stones: u32 = 0;

    // Get live metrics for local stone
    let (cpu_usage, memory_usage) = match metrics::get_fast_metrics() {
        Ok((cpu, mem, _, _)) => (cpu.usage_percent, mem.used_percent),
        Err(_) => (0.0, 0.0),
    };

    // Add self first with live metrics
    let self_info = topology_entry_to_stone_info_with_metrics(&self_entry, cpu_usage, memory_usage);
    total_services += self_info.services_count;
    if self_info.health == garden_common::constants::HEALTH_HEALTHY || self_info.health == garden_common::constants::VITALITY_THRIVING {
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
        if info.health == garden_common::constants::HEALTH_HEALTHY || info.health == garden_common::constants::VITALITY_THRIVING {
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

    Ok(Json(ApiResponse {
        data: overview,
        suggestions,
    }))
}

/// Convert TopologyEntry to StoneInfo for garden overview (peers - no live metrics)
fn topology_entry_to_stone_info(entry: &TopologyEntry) -> StoneInfo {
    topology_entry_to_stone_info_with_metrics(entry, 0.0, 0.0)
}

/// Convert TopologyEntry to StoneInfo with live metrics (for local stone)
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
) -> Result<Json<ApiResponse<HardwareCapabilities>>, (StatusCode, Json<ApiErrorResponse>)> {
    // For now, only support local stone
    if state.current.stone.name != stone_name {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            "STONE_NOT_FOUND",
            format!("Stone '{}' not found in garden", stone_name),
            None,
        ));
    }

    let caps = get_capabilities(&state).await;

    let ctx = Suggestion::from_headers(&headers, "observe_stone");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: caps,
        suggestions,
    }))
}

/// GET /api/v1/stone - Get local stone consolidated info
pub async fn get_local_stone_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<HardwareCapabilities>>, (StatusCode, Json<ApiErrorResponse>)> {
    let caps = get_capabilities(&state).await;

    let ctx = Suggestion::from_headers(&headers, "observe_stone");
    let suggestions = generate_suggestions(&ctx);

    Ok(Json(ApiResponse {
        data: caps,
        suggestions,
    }))
}
/// POST /api/v1/garden/recommend - Get intelligent placement recommendation
pub async fn recommend_placement_v1(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PlacementRequest>,
) -> Result<Json<ApiResponse<PlacementResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    match crate::domain::placement::recommend_placement(request.clone(), &state).await {
        Ok(response) => {
            let ctx = Suggestion::from_headers(&headers, "placement_success");
            let suggestions = generate_suggestions(&ctx);

            Ok(Json(ApiResponse {
                data: response,
                suggestions,
            }))
        }
        Err(e) => {
            tracing::error!(
                offering = %request.offering,
                error = ?e,
                "Placement recommendation failed"
            );

            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "PLACEMENT_ERROR",
                format!("Failed to generate placement recommendation: {}", e),
                None,
            ))
        }
    }
}
// Helper function to build consolidated capabilities (based on main.rs capabilities handler)
async fn get_capabilities(state: &AppState) -> HardwareCapabilities {
    let (cpu_model, cpu_features, architecture) = metrics::get_cpu_info().unwrap_or_else(|_| {
        (
            "Unknown".to_string(),
            vec![],
            std::env::consts::ARCH.to_string(),
        )
    });

    let resources = metrics::collect_stone_resources().ok();
    let total_memory_mb = resources
        .as_ref()
        .map(|r| r.memory.total_bytes / 1024 / 1024)
        .unwrap_or(0);

    let gpus = metrics::detect_gpus();

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

    let os_version = metrics::detect_os_version();
    let kernel_version = metrics::detect_kernel_version();
    let swap_mb = metrics::detect_swap();
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
) -> Result<Json<ApiResponse<Vec<TopologyEntry>>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Step 1: Read self entry (single source of truth for local stone)
    let self_entry = state.current.topology.self_entry.read().await.clone();

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
    let cache_entries = topology::get_all_stones(&state.current.topology.cache).await;

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

    Ok(Json(ApiResponse {
        data: stones,
        suggestions,
    }))
}
