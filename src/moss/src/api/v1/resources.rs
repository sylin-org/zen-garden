//! System resources API endpoint
//!
//! Provides real-time hardware resource snapshots including:
//! - CPU usage and core count
//! - Memory usage (total, used, available)
//! - Disk usage (total, used, available)
//! - Network throughput
//! - System uptime
//!
//! Renamed from `metrics.rs` in ARCH-0018 Book I Chapter 2 — "metrics"
//! is now reserved for software observability (see `api::v1::metrics`,
//! coming in Chapter 5).
//!
//! Resources are collected by the `resources-collector` background task
//! (5s fast tick for CPU/memory, 30s slow tick for disk).
//! If collection has not yet run, a fallback with zero values is returned.

use crate::api::responses::ApiResponse;
use crate::domain::Current;
use axum::{Json, extract::State};
use garden_common::{
    CpuResources, DiskResources, MemoryResources, ResourcesSnapshot, StoneResources,
};
use std::sync::Arc;

/// GET /api/v1/stone/resources — Real-time hardware resource snapshot
///
/// Returns current CPU, memory, disk, network, and uptime from the cached
/// snapshot maintained by the `resources-collector` background task.
/// Fast tick: 5s (CPU/memory/network). Slow tick: 30s (disk).
///
/// # Fallback Behavior
/// If collection has not yet run, returns a fallback response with zero values.
pub async fn get_resources(
    State(current): State<Arc<Current>>,
) -> Json<ApiResponse<ResourcesSnapshot>> {
    let resources = {
        let resources_guard = current.resources.system.read().await;
        resources_guard
            .as_ref()
            .cloned()
            .unwrap_or_else(create_fallback_resources)
    };

    // Convert the data partition to DiskResources for backward compatibility
    let disk = resources
        .data_partition()
        .map(|s| DiskResources {
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
                .map(|s| DiskResources {
                    total_bytes: s.total_gb * 1024 * 1024 * 1024,
                    used_bytes: s.used_gb * 1024 * 1024 * 1024,
                    available_bytes: s.available_gb * 1024 * 1024 * 1024,
                    used_percent: s.used_percent,
                    path: s.mount_point.clone(),
                    total_friendly: String::new(),
                    used_friendly: String::new(),
                    available_friendly: String::new(),
                })
                .unwrap_or_else(|| DiskResources {
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

    let network = current.resources.network.read().await.clone();

    let snapshot = ResourcesSnapshot {
        timestamp: chrono::Utc::now().to_rfc3339(),
        cpu: resources.cpu,
        memory: resources.memory,
        disk,
        network,
        uptime_seconds: resources.uptime_seconds,
    };

    Json(ApiResponse::new(snapshot))
}

/// Create fallback resource snapshot with zero values.
///
/// Used when collection has not yet run to ensure the endpoint always
/// returns valid data.
fn create_fallback_resources() -> StoneResources {
    StoneResources {
        cpu: CpuResources {
            cores: 1,
            usage_percent: 0.0,
            usage_friendly: "0%".to_string(),
        },
        memory: MemoryResources {
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
        cpu_temperature: None,
    }
}
