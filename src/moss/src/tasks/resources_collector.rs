//! System resources collection task
//!
//! Periodically gathers CPU, memory, disk, and network resource snapshots
//! using `garden_common::resources::system`.
//! Updates Moss caches at different intervals:
//! - Fast resources (CPU, memory, uptime, network): every 5 seconds
//! - Slow resources (disk, seed bank usage): every 30 seconds (involves
//!   filesystem stat calls)
//!
//! IMPORTANT: API endpoints MUST NOT perform I/O — they read from these
//! caches only. This is the single source of truth for runtime hardware
//! resource snapshots used by:
//! - Portrait endpoint (/api/v1/stone/portrait) — requires <10ms latency
//! - Presence protocol (SSE streaming to Companions)
//! - Topology chirps (future: include resources in discovery)
//! - Health monitoring (CPU/memory thresholds)
//!
//! Renamed from `metrics_collector` in ARCH-0018 Book I Chapter 2 —
//! "metrics" is now reserved for software observability
//! (see `domain::metrics`).

use tokio::time::interval;

use crate::Moss;
use garden_common::constants::timeouts::{resources_disk_interval, resources_fast_interval};
use garden_common::notifications::{NOTIF_SOURCE_CANDIDATES, NotificationTag};
use garden_common::resources::system::{
    get_fast_resources, get_gpu_utilization, get_network_resources, get_storage_resources,
};

/// Run the system resources collector with dual intervals.
///
/// Fast resources (CPU, memory, uptime) are collected every 5s by
/// default (in-memory kernel data). Disk resources are collected every
/// 30s by default (involves filesystem stat syscalls).
///
/// Tunable via environment variables (the old `GARDEN_METRICS_*` names
/// are preserved for backwards compatibility with existing deployments):
/// - `GARDEN_METRICS_FAST_INTERVAL_SECS` (default: 5)
/// - `GARDEN_METRICS_DISK_INTERVAL_SECS` (default: 30)
///
/// **Rationale:** a single collector task prevents redundant sysinfo
/// refreshes. Multiple consumers (presence SSE, load monitor, health
/// checks) read from the cache. Exits cooperatively when the shutdown
/// token is cancelled (MOSS-0004).
pub async fn run_resources_collector(state: Moss, token: tokio_util::sync::CancellationToken) {
    let mut fast_interval = interval(resources_fast_interval());
    let mut disk_interval = interval(resources_disk_interval());

    // Collect initial complete snapshot immediately
    match get_fast_resources() {
        Ok((cpu, memory, uptime_seconds, uptime_friendly)) => {
            // Also get initial storage resources
            let storage = get_storage_resources().unwrap_or_else(|_| Vec::new());

            let mut cache = state.current.resources.system.write().await;
            *cache = Some(garden_common::StoneResources {
                cpu: cpu.clone(),
                memory: memory.clone(),
                storage,
                uptime_seconds,
                uptime_friendly: uptime_friendly.clone(),
                cpu_temperature: garden_common::resources::system::get_cpu_temperature(),
            });
            tracing::debug!(
                cpu = cpu.usage_percent,
                memory = memory.used_percent,
                "Initial system resources collected"
            );
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to collect initial fast resources");
        }
    }

    // Collect initial network resources
    {
        let network = get_network_resources();
        let mut cache = state.current.resources.network.write().await;
        *cache = Some(network);
        tracing::debug!("Initial network resources collected");
    }

    // STORAGE-0011: Candidates derived from Volumes (cross-platform).
    // A candidate is any unmanaged, removable, online volume.
    {
        let count = {
            let map = state.current.storage.volumes.read().await;
            map.values()
                .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
                .count()
        };
        state.presence.notifications.set_if(
            NOTIF_SOURCE_CANDIDATES,
            NotificationTag::Opportunity,
            count > 0,
        );
        if count > 0 {
            tracing::info!(count, "Initial candidate devices detected");
        }
    }

    // STORAGE-0013: Subscribe to StorageChanged for immediate candidates refresh.
    // Without this, candidates notification waits up to 30s for the next disk tick.
    let mut storage_rx = state.current.storage.changed.subscribe();

    loop {
        tokio::select! {
            // STORAGE-0013: React immediately to storage mutations for candidates
            result = storage_rx.recv() => {
                match result {
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let count = {
                            let map = state.current.storage.volumes.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
                                .count()
                        };
                        state.presence.notifications.set_if(
                            NOTIF_SOURCE_CANDIDATES,
                            NotificationTag::Opportunity,
                            count > 0,
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        // Channel closed — stop reacting but keep collecting resources
                    }
                }
            }

            _ = fast_interval.tick() => {
                // Update CPU, memory, uptime (fast - in-memory kernel data)
                match get_fast_resources() {
                    Ok((cpu, memory, uptime_seconds, uptime_friendly)) => {
                        let mut cache = state.current.resources.system.write().await;
                        if let Some(ref mut resources) = *cache {
                            resources.cpu = cpu;
                            resources.memory = memory;
                            resources.uptime_seconds = uptime_seconds;
                            resources.uptime_friendly = uptime_friendly;
                            resources.cpu_temperature = garden_common::resources::system::get_cpu_temperature();
                        } else {
                            // First fast update, initialize with placeholder disk
                            tracing::warn!("Fast resources collected but no disk data yet");
                        }

                        tracing::trace!(
                            cpu = cache.as_ref().unwrap().cpu.usage_percent,
                            memory = cache.as_ref().unwrap().memory.used_percent,
                            "Fast resources updated"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "Failed to collect fast resources");
                    }
                }

                // Update network resources (fast - reads from kernel counters)
                {
                    let network = get_network_resources();
                    let mut cache = state.current.resources.network.write().await;
                    *cache = Some(network);
                }

                // FIREFLY-0003: Update GPU utilization (fast - shell out to nvidia-smi/rocm-smi)
                {
                    let gpu_util = tokio::task::spawn_blocking(get_gpu_utilization)
                        .await
                        .unwrap_or(None);
                    let mut cache = state.current.resources.gpu.write().await;
                    *cache = gpu_util;
                }
            }

            _ = disk_interval.tick() => {
                // Update storage resources (slow - involves statvfs syscalls)
                match get_storage_resources() {
                    Ok(storage) => {
                        let mut cache = state.current.resources.system.write().await;
                        if let Some(ref mut resources) = *cache {
                            resources.storage = storage;
                            let disk_count = resources.storage.len();
                            tracing::trace!(
                                disks = disk_count,
                                "Storage resources updated"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "Failed to collect storage resources");
                    }
                }

                // STORAGE-0011: Refresh volume disk usage via platform adapter
                {
                    let mut map = state.current.storage.volumes.write().await;
                    for vol in map.values_mut() {
                        let path_str = vol.mount_path().to_string_lossy().to_string();
                        if let Some(usage) = crate::infra::storage::platform::disk_usage(&path_str) {
                            vol.observe(
                                Some(crate::domain::storage::DiskResources {
                                    capacity_bytes: usage.used_bytes + usage.available_bytes,
                                    used_bytes: usage.used_bytes,
                                }),
                                crate::domain::storage::DeviceHealth::healthy(),
                            );
                        }
                    }
                }

                // STORAGE-0011: Refresh candidates notification from Volumes
                {
                    let count = {
                        let map = state.current.storage.volumes.read().await;
                        map.values()
                            .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
                            .count()
                    };
                    state.presence.notifications.set_if(
                        NOTIF_SOURCE_CANDIDATES,
                        NotificationTag::Opportunity,
                        count > 0,
                    );
                }
            }

            _ = token.cancelled() => {
                tracing::debug!("Metrics collector shutting down (MOSS-0004)");
                break;
            }
        }
    }
}
