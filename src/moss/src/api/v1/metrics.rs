//! System metrics API endpoint
//!
//! Provides real-time system resource metrics including:
//! - CPU usage and core count
//! - Memory usage (total, used, available)
//! - Disk usage (total, used, available)
//! - System uptime
//!
//! Metrics are collected on-demand from the system.
//! If collection fails, a fallback with zero values is returned.

use crate::api::responses::ApiResponse;
use crate::AppState;
use axum::{extract::State, Json};
use garden_common::{CpuMetrics, DiskMetrics, MemoryMetrics, MetricsSnapshot, StoneResources};

/// GET /metrics - Real-time system resource metrics
///
/// Returns current CPU, memory, disk usage and system uptime from cached metrics.
/// Metrics are collected every 5s (CPU/memory) and 30s (disk) by background task.
///
/// # Fallback Behavior
/// If metrics not yet collected, returns a fallback response with zero values.
pub async fn get_metrics(State(state): State<AppState>) -> Json<ApiResponse<MetricsSnapshot>> {
    let resources_guard = state.system_resources.read().await;
    let resources = resources_guard
        .as_ref()
        .cloned()
        .unwrap_or_else(create_fallback_resources);
    drop(resources_guard);

    // Convert primary storage mount to DiskMetrics for backward compatibility
    let disk = resources
        .storage
        .iter()
        .find(|s| s.mount_point == "/" || s.mount_point == "C:\\\\")
        .or_else(|| resources.storage.iter().max_by_key(|s| s.total_gb))
        .map(|s| DiskMetrics {
            total_bytes: s.total_gb * 1024 * 1024 * 1024,
            used_bytes: s.used_gb * 1024 * 1024 * 1024,
            available_bytes: s.available_gb * 1024 * 1024 * 1024,
            used_percent: s.used_percent,
            path: s.mount_point.clone(),
            total_friendly: garden_common::format_bytes(s.total_gb * 1024 * 1024 * 1024),
            used_friendly: garden_common::format_bytes(s.used_gb * 1024 * 1024 * 1024),
            available_friendly: garden_common::format_bytes(s.available_gb * 1024 * 1024 * 1024),
        })
        .unwrap_or_else(|| {
            create_fallback_resources()
                .storage
                .first()
                .cloned()
                .map(|s| DiskMetrics {
                    total_bytes: s.total_gb * 1024 * 1024 * 1024,
                    used_bytes: s.used_gb * 1024 * 1024 * 1024,
                    available_bytes: s.available_gb * 1024 * 1024 * 1024,
                    used_percent: s.used_percent,
                    path: s.mount_point.clone(),
                    total_friendly: String::new(),
                    used_friendly: String::new(),
                    available_friendly: String::new(),
                })
                .unwrap_or_else(|| DiskMetrics {
                    total_bytes: 0,
                    used_bytes: 0,
                    available_bytes: 0,
                    used_percent: 0.0,
                    path: "/".to_string(),
                    total_friendly: "0 B".to_string(),
                    used_friendly: "0 B".to_string(),
                    available_friendly: "0 B".to_string(),
                })
        });

    let network = state.network_metrics_cache.read().await.clone();

    let snapshot = MetricsSnapshot {
        timestamp: chrono::Utc::now().to_rfc3339(),
        cpu: resources.cpu,
        memory: resources.memory,
        disk,
        network,
        uptime_seconds: resources.uptime_seconds,
    };

    Json(ApiResponse {
        data: snapshot,
        suggestions: None,
    })
}

/// Create fallback resource metrics with zero values
///
/// Used when metrics not yet collected to ensure endpoint always returns valid data.
fn create_fallback_resources() -> StoneResources {
    StoneResources {
        cpu: CpuMetrics {
            cores: 1,
            usage_percent: 0.0,
            usage_friendly: "0%".to_string(),
        },
        memory: MemoryMetrics {
            total_bytes: 0,
            used_bytes: 0,
            available_bytes: 0,
            used_percent: 0.0,
            total_friendly: "0 B".to_string(),
            used_friendly: "0 B".to_string(),
            available_friendly: "0 B".to_string(),
        },
        storage: Vec::new(),
        uptime_seconds: 0,
        uptime_friendly: "0s".to_string(),
    }
}
