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
    pub uptime: String,
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

/// Foundation metrics section
#[derive(Debug, Clone, Serialize)]
pub struct PortraitFoundation {
    pub cpu: FoundationCpu,
    pub memory: FoundationMemory,
    pub disk: FoundationDisk,
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

/// Horizon stone entry
#[derive(Debug, Clone, Serialize)]
pub struct HorizonStone {
    pub name: String,
    pub endpoint: String,
    pub health: String,
    pub color: String,
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
    
    let identity = PortraitIdentity {
        id: state.stone_id.clone(),
        name: state.stone_name.clone(),
        role,
        version: env!("CARGO_PKG_VERSION").to_string(),
        color: stone_color,
        endpoint,
        uptime,
    };

    // === Foundation (system resources) ===
    let foundation = {
        let resources = state.system_resources.read().await;
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
            }
        } else {
            // No metrics yet - return placeholder
            PortraitFoundation {
                cpu: FoundationCpu { cores: 0, percent: 0.0 },
                memory: FoundationMemory { total_gb: 0.0, used_gb: 0.0, percent: 0.0 },
                disk: FoundationDisk { total_gb: 0, used_gb: 0, percent: 0.0 },
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

    // === Companions (adapters) ===
    let companions = {
        let adapters = state.adapter_registry.list().await;
        let mut result = Vec::new();
        for adapter in adapters {
            let running = state.adapter_registry.is_running(&adapter.id).await;
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
            .map(|entry| HorizonStone {
                name: entry.stone_name.clone(),
                endpoint: entry.endpoint.clone(),
                health: entry.health.clone(),
                color: derive_stone_color(&entry.stone_id),
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
        companions,
        horizon,
    }))
}
