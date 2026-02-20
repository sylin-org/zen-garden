//! System metrics collection task
//!
//! Periodically gathers CPU, memory, disk, and network metrics using garden_common::metrics::system.
//! Updates AppState caches at different intervals:
//! - Fast metrics (CPU, memory, uptime, network): every 5 seconds
//! - Slow metrics (disk, seed bank usage): every 30 seconds (involves filesystem stat calls)
//!
//! IMPORTANT: API endpoints MUST NOT perform I/O - they read from these caches only.
//! This is the single source of truth for runtime system metrics used by:
//! - Portrait endpoint (/api/v1/stone/portrait) - requires <10ms latency
//! - Presence protocol (SSE streaming to Companions)
//! - Topology chirps (future: include metrics in discovery)
//! - Health monitoring (CPU/memory thresholds)

use tokio::time::interval;

use crate::infra::storage::SeedBankRegistry;
use crate::AppState;
use garden_common::constants::timeouts::{metrics_disk_interval, metrics_fast_interval};
use garden_common::metrics::system::{
    get_fast_metrics, get_gpu_utilization, get_network_metrics, get_storage_metrics,
};
#[cfg(target_os = "linux")]
use garden_common::storage::StorageDetectedInfo;
#[cfg(target_os = "linux")]
use garden_common::{NotificationTag, NOTIF_SOURCE_CANDIDATES};

/// Run system metrics collector with dual intervals
///
/// Fast metrics (CPU, memory, uptime) collected every 5s by default (in-memory kernel data).
/// Disk metrics collected every 30s by default (involves filesystem stat syscalls).
///
/// Tunable via environment variables:
/// - `GARDEN_METRICS_FAST_INTERVAL_SECS` (default: 5)
/// - `GARDEN_METRICS_DISK_INTERVAL_SECS` (default: 30)
///
/// **Rationale:** Single collector task prevents redundant sysinfo refreshes.
/// Multiple consumers (presence SSE, load monitor, health checks) read from cache.
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub async fn run_metrics_collector(state: AppState, token: tokio_util::sync::CancellationToken) {
    let mut fast_interval = interval(metrics_fast_interval());
    let mut disk_interval = interval(metrics_disk_interval());

    // Collect initial complete snapshot immediately
    match get_fast_metrics() {
        Ok((cpu, memory, uptime_seconds, uptime_friendly)) => {
            // Also get initial storage metrics
            let storage = get_storage_metrics().unwrap_or_else(|_| Vec::new());

            let mut cache = state.system_resources.write().await;
            *cache = Some(garden_common::StoneResources {
                cpu: cpu.clone(),
                memory: memory.clone(),
                storage,
                uptime_seconds,
                uptime_friendly: uptime_friendly.clone(),
            });
            tracing::debug!(
                cpu = cpu.usage_percent,
                memory = memory.used_percent,
                "Initial system metrics collected"
            );
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to collect initial fast metrics");
        }
    }

    // Collect initial network metrics
    {
        let network = get_network_metrics();
        let mut cache = state.network_metrics_cache.write().await;
        *cache = Some(network);
        tracing::debug!("Initial network metrics collected");
    }

    // Collect initial seed bank registry
    // Lifecycle objects (state.seed_banks) are the source of truth;
    // this scan populates disk usage via health ticks.
    match SeedBankRegistry::scan().await {
        Ok(registry) => {
            let count = registry.list().len();
            tracing::debug!(count, "Initial seed bank registry scanned");
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load initial seed bank registry");
        }
    }

    // Collect initial candidates (USB devices eligible for preparation)
    // Linux-only; other platforms always have empty candidates
    #[cfg(target_os = "linux")]
    {
        match scan_candidates().await {
            Ok(candidates) => {
                let count = candidates.len();
                let mut cache = state.candidates_cache.write().await;
                *cache = candidates;
                // Update notification registry for cross-stone awareness
                state.notifications.set_if(
                    NOTIF_SOURCE_CANDIDATES,
                    NotificationTag::Opportunity,
                    count > 0,
                );
                if count > 0 {
                    tracing::info!(count, "Initial candidate devices detected");
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to scan initial candidates");
            }
        }
    }

    loop {
        tokio::select! {
            _ = fast_interval.tick() => {
                // Update CPU, memory, uptime (fast - in-memory kernel data)
                match get_fast_metrics() {
                    Ok((cpu, memory, uptime_seconds, uptime_friendly)) => {
                        let mut cache = state.system_resources.write().await;
                        if let Some(ref mut resources) = *cache {
                            resources.cpu = cpu;
                            resources.memory = memory;
                            resources.uptime_seconds = uptime_seconds;
                            resources.uptime_friendly = uptime_friendly;
                        } else {
                            // First fast update, initialize with placeholder disk
                            tracing::warn!("Fast metrics collected but no disk data yet");
                        }

                        tracing::trace!(
                            cpu = cache.as_ref().unwrap().cpu.usage_percent,
                            memory = cache.as_ref().unwrap().memory.used_percent,
                            "Fast metrics updated"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "Failed to collect fast metrics");
                    }
                }

                // Update network metrics (fast - reads from kernel counters)
                {
                    let network = get_network_metrics();
                    let mut cache = state.network_metrics_cache.write().await;
                    *cache = Some(network);
                }

                // FIREFLY-0003: Update GPU utilization (fast - shell out to nvidia-smi/rocm-smi)
                {
                    let gpu_util = tokio::task::spawn_blocking(get_gpu_utilization)
                        .await
                        .unwrap_or(None);
                    let mut cache = state.gpu_utilization.write().await;
                    *cache = gpu_util;
                }
            }

            _ = disk_interval.tick() => {
                // Update storage metrics (slow - involves statvfs syscalls)
                match get_storage_metrics() {
                    Ok(storage) => {
                        let mut cache = state.system_resources.write().await;
                        if let Some(ref mut resources) = *cache {
                            resources.storage = storage;
                            let disk_count = resources.storage.len();
                            tracing::trace!(
                                disks = disk_count,
                                "Storage metrics updated"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "Failed to collect storage metrics");
                    }
                }

                // Refresh seed bank disk usage via lifecycle objects' health ticks
                // (used_bytes / capacity_bytes updated through StorageDevice::health_tick)
                {
                    let mut banks = state.seed_banks.write().await;
                    for bank in banks.values_mut() {
                        if let Some((used, avail)) = crate::infra::storage::DeviceAnalyzer::get_disk_usage(&bank.storage.mount_path.to_string_lossy()) {
                            bank.storage.used_bytes = used;
                            bank.storage.capacity_bytes = used + avail;
                        }
                    }
                }

                // Refresh candidates cache (Linux-only)
                // Candidates change on USB insert/remove, detected by storage_monitor via udev.
                // This periodic refresh catches any missed events and ensures consistency.
                #[cfg(target_os = "linux")]
                {
                    match scan_candidates().await {
                        Ok(candidates) => {
                            let mut cache = state.candidates_cache.write().await;
                            let prev_count = cache.len();
                            let new_count = candidates.len();
                            *cache = candidates;
                            // Update notification registry for cross-stone awareness
                            state.notifications.set_if(
                                NOTIF_SOURCE_CANDIDATES,
                                NotificationTag::Opportunity,
                                new_count > 0,
                            );
                            if new_count != prev_count {
                                tracing::debug!(prev = prev_count, new = new_count, "Candidates cache updated");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to refresh candidates cache");
                        }
                    }
                }
            }

            _ = token.cancelled() => {
                tracing::debug!("Metrics collector shutting down (MOSS-0004)");
                break;
            }
        }
    }
}

/// Scan for candidate devices (Linux-only)
#[cfg(target_os = "linux")]
async fn scan_candidates() -> anyhow::Result<Vec<StorageDetectedInfo>> {
    use crate::infra::storage::list_usb_partitions;
    tokio::task::spawn_blocking(|| list_usb_partitions())
        .await
        .map_err(|e| anyhow::anyhow!("Spawn blocking failed: {}", e))?
}
