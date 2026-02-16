//! Stone metrics monitoring for presence protocol
//!
//! Emits StoneEvent::LoadUpdated and StoneEvent::HealthChanged via EventBus.

use std::time::Duration;
use tokio::time::interval;

use crate::domain::StoneEvent;
use crate::AppState;

/// Run load monitoring task (every 5s)
///
/// Emits StoneEvent::LoadUpdated via EventBus for presence stream.
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
        let (cpu_percent, memory_percent) = {
            let resources = state.system_resources.read().await;
            if let Some(ref res) = *resources {
                (res.cpu.usage_percent as f64, res.memory.used_percent as f64)
            } else {
                (0.0, 0.0)
            }
        };

        // Emit StoneEvent::LoadUpdated via EventBus
        let event = StoneEvent::load_updated(cpu_percent, memory_percent);
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
