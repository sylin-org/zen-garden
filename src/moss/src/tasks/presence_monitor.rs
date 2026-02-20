//! Stone metrics monitoring for presence protocol
//!
//! Emits StoneEvent::LoadUpdated and StoneEvent::HealthChanged via EventBus.

use std::time::Duration;
use tokio::time::interval;

use crate::domain::StoneEvent;
use crate::AppState;

/// GPU activity threshold — above this percentage, gpu_active = true (FIREFLY-0003)
const GPU_ACTIVE_THRESHOLD: f64 = 10.0;

/// Run load monitoring task (every 5s)
///
/// Emits StoneEvent::LoadUpdated via EventBus for presence stream.
/// FIREFLY-0003: Now includes disk, I/O, GPU, and network metrics.
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub async fn run_load_monitor_task(state: AppState, token: tokio_util::sync::CancellationToken) {
    let mut interval = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = token.cancelled() => {
                tracing::debug!("Presence load monitor shutting down (MOSS-0004)");
                break;
            }
        }

        // Get real system metrics from shared cache
        let (cpu_percent, memory_percent, disk_percent) = {
            let resources = state.system_resources.read().await;
            if let Some(ref res) = *resources {
                let primary_disk = res
                    .storage
                    .iter()
                    .find(|s| s.mount_point == "/" || s.mount_point == "C:\\\\")
                    .or_else(|| res.storage.iter().max_by_key(|s| s.total_gb))
                    .map(|s| s.used_percent as f64)
                    .unwrap_or(0.0);
                (res.cpu.usage_percent as f64, res.memory.used_percent as f64, primary_disk)
            } else {
                (0.0, 0.0, 0.0)
            }
        };

        // FIREFLY-0003: Get GPU utilization from cache
        let gpu_percent = {
            let gpu = state.gpu_utilization.read().await;
            gpu.unwrap_or(0.0) as f64
        };
        let gpu_active = gpu_percent > GPU_ACTIVE_THRESHOLD;

        // FIREFLY-0003: Get network rates from cache
        let (net_rx, net_tx) = {
            let network = state.network_metrics_cache.read().await;
            network
                .as_ref()
                .map(|n| {
                    (
                        n.rx_bytes_per_sec.unwrap_or(0),
                        n.tx_bytes_per_sec.unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0))
        };

        // FIREFLY-0003: I/O utilization — placeholder (0.0) until per-platform I/O collection is added
        let io_percent = 0.0_f64;

        // Emit StoneEvent::LoadUpdated via EventBus (FIREFLY-0003: extended)
        let event = StoneEvent::load_updated(
            cpu_percent,
            memory_percent,
            disk_percent,
            io_percent,
            gpu_percent,
            gpu_active,
            net_rx,
            net_tx,
        );
        state.event_bus.emit(event);
    }
}

/// Run health monitor task (every 30s)
///
/// Computes stone health from metrics and emits StoneEvent::HealthChanged when status changes.
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub async fn run_health_monitor_task(state: AppState, token: tokio_util::sync::CancellationToken) {
    let mut interval = interval(Duration::from_secs(30));
    let mut last_health = "thriving".to_string();

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = token.cancelled() => {
                tracing::debug!("Presence health monitor shutting down (MOSS-0004)");
                break;
            }
        }

        // Get real metrics from shared cache
        let (cpu, memory) = {
            let resources = state.system_resources.read().await;
            if let Some(ref res) = *resources {
                (res.cpu.usage_percent as f64, res.memory.used_percent as f64)
            } else {
                (0.0, 0.0)
            }
        };

        let new_health = if cpu > 95.0 || memory > 95.0 {
            "wilting"
        } else if cpu > 80.0 || memory > 80.0 {
            "withering"
        } else {
            "thriving"
        };

        if new_health != last_health {
            tracing::info!(
                old = %last_health,
                new = %new_health,
                "Stone health changed"
            );

            // Emit StoneEvent::HealthChanged via EventBus
            let event = StoneEvent::health_changed(new_health, cpu, memory);
            state.event_bus.emit(event);

            last_health = new_health.to_string();
        }
    }
}
