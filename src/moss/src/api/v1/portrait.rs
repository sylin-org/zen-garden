//! Stone Portrait - Living landing page for Moss daemon
//!
//! Provides a single-page application that displays a stone's identity,
//! resources, offerings, adapters, and visible network topology.
//!
//! See: docs/decisions/PORTRAIT-0001-stone-landing-page.md

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::app_state::AppState;
use crate::cli;
use crate::domain::topology;

/// Embedded HTML template (baked into binary at compile time)
const PORTRAIT_HTML: &str = include_str!("../../../assets/portrait.html");

/// Stone identity section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitIdentity {
    pub id: String,
    pub name: String,
    pub role: String,
    pub version: String,
    pub color: String,
    pub endpoint: String,
    /// System (stone) uptime - how long the machine has been running
    pub uptime: String,
    /// Moss daemon uptime - how long the daemon has been running
    pub moss_uptime: String,
    /// Hardware manufacturer (e.g., "Dell Inc.")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Hardware model (e.g., "Wyse 5070")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// CPU metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationCpu {
    pub cores: usize,
    pub percent: f32,
}

/// Memory metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationMemory {
    pub total_gb: f32,
    pub used_gb: f32,
    pub percent: f32,
}

/// Disk metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationDisk {
    pub total_gb: u64,
    pub used_gb: u64,
    pub percent: f32,
}

/// Network metrics for foundation
#[derive(Debug, Clone, Serialize)]
pub struct FoundationNetwork {
    /// Total bytes received across all interfaces
    pub rx_bytes: u64,
    /// Total bytes transmitted across all interfaces
    pub tx_bytes: u64,
    /// Human-readable received bytes (e.g., "1.5 GB")
    pub rx_friendly: String,
    /// Human-readable transmitted bytes (e.g., "500 MB")
    pub tx_friendly: String,
}

/// Foundation metrics section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitFoundation {
    pub cpu: FoundationCpu,
    pub memory: FoundationMemory,
    pub disk: FoundationDisk,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<FoundationNetwork>,
}

/// Offering entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitOffering {
    pub name: String,
    pub container: Option<String>,
    pub port: u16,
    pub status: String,
    pub health: String,
}

/// Companion (adapter) entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitCompanion {
    pub id: String,
    pub name: String,
    pub description: String,
    pub port: Option<u16>,
    pub status: String,
}

/// Seed bank entry
#[derive(Debug, Clone, Serialize)]
pub struct PortraitSeedBank {
    pub name: String,
    pub used_gb: f32,
    pub capacity_gb: f32,
    pub filesystem: String,
    pub visibility: String,
    pub roaming: bool,
    pub online: bool,
}

/// Horizon stone entry
#[derive(Debug, Clone, Serialize)]
pub struct HorizonStone {
    pub name: String,
    pub endpoint: String,
    pub health: String,
    pub color: String,
    /// Number of CPU cores (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<usize>,
    /// Total memory in GB (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<u64>,
    /// Number of running services
    pub service_count: usize,
    /// Hardware manufacturer (e.g., "Dell Inc.")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Hardware model (e.g., "Wyse 5070")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Horizon section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitHorizon {
    pub count: usize,
    pub stones: Vec<HorizonStone>,
}

/// Complete portrait response
#[derive(Debug, Clone, Serialize)]
pub struct PortraitResponse {
    pub identity: PortraitIdentity,
    pub foundation: PortraitFoundation,
    pub offerings: Vec<PortraitOffering>,
    pub seed_banks: Vec<PortraitSeedBank>,
    pub companions: Vec<PortraitCompanion>,
    pub horizon: PortraitHorizon,
}

/// Derive a unique HSL color from stone ID
///
/// Uses hash of stone_id to generate a consistent hue (0-360),
/// with fixed saturation (55%) and lightness (50%) for balanced visibility.
fn derive_stone_color(stone_id: &str) -> String {
    let mut hasher = DefaultHasher::new();
    stone_id.hash(&mut hasher);
    let hash = hasher.finish();
    let hue = (hash % 360) as u16;
    format!("hsl({}, 55%, 50%)", hue)
}

/// GET /
///
/// Returns the portrait SPA HTML page.
/// The page uses Alpine.js to poll /api/v1/stone/portrait for data.
pub async fn get_portrait_page() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(PORTRAIT_HTML),
    )
}

/// GET /api/v1/stone/portrait
///
/// Returns JSON data for the portrait SPA.
/// Aggregates identity, foundation metrics, offerings, adapters, and topology.
///
/// PERF: This endpoint MUST only read from cached AppState data - NO I/O operations.
/// All metrics are collected by background tasks and cached in AppState.
/// Target latency: <10ms. Any I/O here will cause latency regression.
pub async fn get_portrait_data(
    State(state): State<AppState>,
) -> Result<Json<PortraitResponse>, StatusCode> {
    // === Identity ===
    let stone_color = derive_stone_color(&state.stone_id);
    
    // Determine role (simple heuristic - could be enhanced)
    let role = "STONE".to_string(); // TODO: detect LANTERN/CORNERSTONE from state
    
    // Build endpoint URL
    let endpoint = format!("http://{}:{}", state.stone_name, state.api_port);
    
    // Get uptime from resources
    let uptime = {
        let resources = state.system_resources.read().await;
        resources.as_ref()
            .map(|r| r.uptime_friendly.clone())
            .unwrap_or_else(|| "–".into())
    };
    
    // Get Moss daemon uptime
    let moss_uptime = {
        let secs = state.start_time.elapsed().as_secs();
        garden_common::utils::format_uptime(secs)
    };

    // Get hardware manufacturer/model from capabilities
    let (manufacturer, model) = {
        let caps = state.capabilities.read().await;
        if let Some(ref c) = *caps {
            (
                c.hardware.system_manufacturer.clone(),
                c.hardware.system_product.clone(),
            )
        } else {
            (None, None)
        }
    };

    let identity = PortraitIdentity {
        id: state.stone_id.clone(),
        name: state.stone_name.clone(),
        role,
        version: cli::VERSION.to_string(),
        color: stone_color,
        endpoint,
        uptime,
        moss_uptime,
        manufacturer,
        model,
    };

    // === Foundation (system resources) ===
    // NOTE: All metrics read from cache - no I/O allowed here
    let foundation = {
        let resources = state.system_resources.read().await;

        // Read network metrics from cache (populated by health_monitor task)
        let network = {
            let cached = state.network_metrics_cache.read().await;
            cached.as_ref().map(|m| FoundationNetwork {
                rx_bytes: m.total_rx_bytes,
                tx_bytes: m.total_tx_bytes,
                rx_friendly: m.total_rx_friendly.clone(),
                tx_friendly: m.total_tx_friendly.clone(),
            })
        };

        if let Some(ref res) = *resources {
            // Find primary disk (root mount or first available)
            let primary_disk = res.storage.iter()
                .find(|d| d.mount_point == "/" || d.mount_point == "C:\\")
                .or_else(|| res.storage.first());

            let disk = if let Some(d) = primary_disk {
                FoundationDisk {
                    total_gb: d.total_gb,
                    used_gb: d.used_gb,
                    percent: d.used_percent,
                }
            } else {
                FoundationDisk {
                    total_gb: 0,
                    used_gb: 0,
                    percent: 0.0,
                }
            };

            PortraitFoundation {
                cpu: FoundationCpu {
                    cores: res.cpu.cores,
                    percent: res.cpu.usage_percent,
                },
                memory: FoundationMemory {
                    total_gb: res.memory.total_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                    used_gb: res.memory.used_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                    percent: res.memory.used_percent,
                },
                disk,
                network,
            }
        } else {
            // No metrics yet - return placeholder
            PortraitFoundation {
                cpu: FoundationCpu { cores: 0, percent: 0.0 },
                memory: FoundationMemory { total_gb: 0.0, used_gb: 0.0, percent: 0.0 },
                disk: FoundationDisk { total_gb: 0, used_gb: 0, percent: 0.0 },
                network,
            }
        }
    };

    // === Offerings (services) ===
    let offerings = {
        let registry = state.registry.read().await;
        registry
            .iter()
            .map(|svc| {
                let status_str = match svc.status {
                    garden_common::ServiceStatus::Running => "running",
                    garden_common::ServiceStatus::Stopped => "stopped",
                    garden_common::ServiceStatus::Installing => "installing",
                    garden_common::ServiceStatus::Maintenance => "maintenance",
                    garden_common::ServiceStatus::Degraded => "degraded",
                    garden_common::ServiceStatus::Unknown => "unknown",
                };
                let health_str = match svc.health {
                    garden_common::ServiceHealthStatus::Healthy => "healthy",
                    garden_common::ServiceHealthStatus::Degraded => "degraded",
                    garden_common::ServiceHealthStatus::Offline => "offline",
                };

                PortraitOffering {
                    name: svc.name.clone(),
                    container: Some(svc.offering.clone()),
                    port: svc.ports.native,
                    status: status_str.to_string(),
                    health: health_str.to_string(),
                }
            })
            .collect()
    };

    // === Seed Banks ===
    // NOTE: Read from cache - populated by storage_monitor on events + periodic refresh
    let seed_banks = {
        let cached = state.seed_bank_cache.read().await;
        cached.iter().map(|bank| {
            PortraitSeedBank {
                name: bank.name.clone(),
                used_gb: bank.used_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                capacity_gb: bank.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
                filesystem: if bank.btrfs { "btrfs".into() } else { "ext4".into() },
                visibility: bank.visibility.to_string(),
                roaming: bank.roaming,
                online: bank.online,
            }
        }).collect()
    };

    // === Companions (adapters) ===
    let companions = {
        let adapters = state.companion_registry.list().await;
        let mut result = Vec::new();
        for adapter in adapters {
            let running = state.companion_registry.is_running(&adapter.id).await;
            result.push(PortraitCompanion {
                id: adapter.manifest.id.clone(),
                name: adapter.manifest.name.clone(),
                description: adapter.manifest.description.clone(),
                port: adapter.port(),
                status: if running { "running".into() } else { "stopped".into() },
            });
        }
        result
    };

    // === Horizon (visible stones) ===
    let horizon = {
        let visible_stones = topology::get_online_stones(&state.topology_cache).await;
        let stones: Vec<HorizonStone> = visible_stones
            .into_iter()
            .filter(|entry| entry.stone_id != state.stone_id) // Exclude self
            .map(|entry| {
                // Extract resource hints from capabilities
                let caps = entry.capabilities.as_ref();
                let cpu_cores = caps.map(|c| c.hardware.cpu.cores);
                let memory_gb = caps.map(|c| c.hardware.memory.total_mb / 1024);
                let manufacturer = caps.and_then(|c| c.hardware.system_manufacturer.clone());
                let model = caps.and_then(|c| c.hardware.system_product.clone());
                let service_count = entry.services.len();

                HorizonStone {
                    name: entry.stone_name.clone(),
                    endpoint: entry.endpoint.clone(),
                    health: entry.health.clone(),
                    color: derive_stone_color(&entry.stone_id),
                    cpu_cores,
                    memory_gb,
                    service_count,
                    manufacturer,
                    model,
                }
            })
            .collect();

        PortraitHorizon {
            count: stones.len(),
            stones,
        }
    };

    Ok(Json(PortraitResponse {
        identity,
        foundation,
        offerings,
        seed_banks,
        companions,
        horizon,
    }))
}

/// GET /api/v1/stone/portrait/guidance
///
/// Returns compiled markdown containing all offering guidance.
/// Each offering's guidance is separated by a header with the offering name.
/// Supports HTTP caching via ETag header.
///
/// Returns 204 No Content if no offerings have guidance.
pub async fn get_portrait_guidance(
    State(state): State<AppState>,
) -> axum::response::Response {
    use axum::response::Response;
    use axum::body::Body;

    // Collect all guidance from installed services
    let guidance_sections: Vec<(String, String)> = {
        let registry = state.registry.read().await;
        registry
            .iter()
            .filter_map(|svc| {
                svc.guidance
                    .as_ref()
                    .map(|g| (svc.name.clone(), g.content.clone()))
            })
            .collect()
    };

    // Return 204 if no guidance available
    if guidance_sections.is_empty() {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
            .body(Body::empty())
            .unwrap();
    }

    // Build combined markdown document
    let mut markdown = String::new();
    for (i, (name, content)) in guidance_sections.iter().enumerate() {
        if i > 0 {
            markdown.push_str("\n\n---\n\n");
        }
        // Use offering name as section header (capitalize first letter)
        let display_name = name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
            + &name[1..];
        markdown.push_str(&format!("# {}\n\n", display_name));
        markdown.push_str(content);
    }

    // Generate ETag from content hash
    let mut hasher = DefaultHasher::new();
    markdown.hash(&mut hasher);
    let etag = format!("\"{}\"", hasher.finish());

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .header(header::ETAG, etag)
        .body(Body::from(markdown))
        .unwrap()
}
