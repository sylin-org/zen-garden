
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

use crate::AppState;
use crate::infra::storage::SeedBankRegistry;
use garden_common::metrics::system::{get_fast_metrics, get_storage_metrics, get_network_metrics};
use garden_common::constants::timeouts::{metrics_fast_interval, metrics_disk_interval};
use garden_common::storage::SeedBankInfo;

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
pub async fn run_metrics_collector(state: AppState) {
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

    // Collect initial seed bank registry (subsequent updates come from storage events)
    match SeedBankRegistry::scan().await {
        Ok(registry) => {
            let banks: Vec<SeedBankInfo> = registry.list().into_iter().cloned().collect();
            let count = banks.len();
            let mut cache = state.seed_bank_cache.write().await;
            *cache = banks;
            tracing::debug!(count, "Initial seed bank registry loaded");
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load initial seed bank registry");
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

                // Refresh seed bank disk usage (slow - involves statvfs per mount)
                // Note: The seed bank list itself is maintained by storage_monitor on events.
                // This just refreshes used_bytes/capacity_bytes for existing entries.
                {
                    let mut cache = state.seed_bank_cache.write().await;
                    for bank in cache.iter_mut() {
                        if let Some((used, avail)) = crate::infra::storage::DeviceAnalyzer::get_disk_usage(&bank.mount_path) {
                            bank.used_bytes = used;
                            bank.capacity_bytes = used + avail;
                        }
                    }
                }
            }
        }
    }
}
