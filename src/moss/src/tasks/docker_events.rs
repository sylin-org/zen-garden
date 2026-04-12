//! Docker container events stream monitor
//!
//! Subscribes to Docker's event stream for real-time container state changes,
//! supplementing the 30-second polling in the health monitor. Provides
//! immediate notification of container crashes, restarts, and health changes.
//!
//! ## Architecture
//! - Connects to Docker events API filtered for container lifecycle events
//! - Translates Docker events (start/stop/die/health_status) into offering
//!   status updates and domain events via the EventBus
//! - Handles stream disconnections with exponential backoff reconnection
//! - Skips containers not matching the `zen-offering-` prefix
//!
//! ## Relationship to Health Monitor
//! This task **supplements** the health monitor (does not replace it).
//! The health monitor continues running as a safety net for:
//! - Resource usage (CPU, memory)
//! - Port reconciliation
//! - Protocol reconciliation
//! - Topology mount checks
//! - Self-heal adoption of orphaned containers

use crate::AppState;
use crate::docker::{ContainerEvent, decode_zen_offering_container_name};
use crate::domain::events::OfferingEvent;
use futures_util::StreamExt;
use garden_common::constants::OFFERING_CONTAINER_PREFIX;
use garden_common::{OfferingStatus, ServiceHealthStatus};

use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Maximum backoff interval for reconnection attempts (60 seconds).
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(60);

/// Initial backoff interval for reconnection (2 seconds).
const INITIAL_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);

/// Background task that streams Docker container events and reacts to
/// state transitions in real time.
///
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub async fn docker_events_task(state: AppState, token: CancellationToken) {
    let mut backoff = INITIAL_RECONNECT_BACKOFF;

    loop {
        if token.is_cancelled() {
            tracing::debug!("Docker events task shutting down (MOSS-0004)");
            break;
        }

        // Wait for Docker to be available before subscribing
        if !state.subsystems.is_ready("docker") {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
                _ = token.cancelled() => break,
            }
        }

        tracing::info!("Docker events stream: subscribing to container events");
        let mut events = state.platform.container.container_events();
        let mut received_any = false;

        loop {
            tokio::select! {
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(ref event)) => {
                            received_any = true;
                            handle_container_event(&state, event).await;
                        }
                        Some(Err(e)) => {
                            tracing::warn!(
                                error = %e,
                                backoff_secs = backoff.as_secs(),
                                "Docker events stream error, will reconnect"
                            );
                            break; // break inner loop to reconnect
                        }
                        None => {
                            tracing::info!("Docker events stream ended, will reconnect");
                            break; // stream closed, reconnect
                        }
                    }
                }
                _ = token.cancelled() => {
                    tracing::debug!("Docker events task shutting down (MOSS-0004)");
                    return;
                }
            }
        }

        // Reset backoff if the stream was healthy (received events before disconnect)
        if received_any {
            backoff = INITIAL_RECONNECT_BACKOFF;
        }

        // Exponential backoff before reconnecting
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = token.cancelled() => break,
        }
        backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
    }
}

/// Process a single container lifecycle event.
///
/// Checks whether the container is a managed zen-offering container,
/// and updates the offering status and emits domain events accordingly.
async fn handle_container_event(state: &AppState, event: &ContainerEvent) {
    let action = event.action.as_str();
    let container_name = event.container_name.as_str();

    // Only process zen-offering containers
    if !container_name.starts_with(OFFERING_CONTAINER_PREFIX) {
        return;
    }

    let offering_name = match decode_zen_offering_container_name(container_name) {
        Some(name) => name,
        None => return,
    };

    tracing::info!(
        offering = %offering_name,
        action = %action,
        container = %container_name,
        "Docker event received"
    );

    // Look up the offering in the registry
    let offering_snapshot = state
        .offerings
        .with_active(|offerings| {
            offerings
                .iter()
                .find(|o| o.name.to_string() == offering_name)
                .map(|o| (o.offering_id.clone(), o.status, o.health.clone()))
        })
        .await;

    let (offering_id, old_status, old_health) = match offering_snapshot {
        Some(snap) => snap,
        None => {
            tracing::debug!(
                offering = %offering_name,
                action = %action,
                "Docker event for unregistered container, skipping (health monitor will adopt)"
            );
            return;
        }
    };

    // Skip offerings that are currently installing
    if old_status == OfferingStatus::Installing {
        tracing::trace!(
            offering = %offering_name,
            action = %action,
            "Skipping Docker event (currently installing)"
        );
        return;
    }

    // Map Docker event action to new status/health
    let (new_status, new_health) = match action {
        "start" => (OfferingStatus::Running, ServiceHealthStatus::Healthy),
        "stop" => (OfferingStatus::Stopped, ServiceHealthStatus::Offline),
        "die" => {
            // Container died unexpectedly -- mark as stopped/offline.
            // Docker may auto-restart it (restart policy), in which case
            // a subsequent "start" event will flip it back to running.
            (OfferingStatus::Stopped, ServiceHealthStatus::Offline)
        }
        "kill" => {
            // Kill signal sent; container will stop shortly (die event follows).
            // Don't update yet -- wait for the "die" event.
            return;
        }
        "destroy" => (OfferingStatus::Stopped, ServiceHealthStatus::Offline),
        a if a.starts_with("health_status:") => {
            // Format: "health_status: healthy" or "health_status: unhealthy"
            let health_str = a.trim_start_matches("health_status:").trim();
            let health = match health_str {
                "healthy" => ServiceHealthStatus::Healthy,
                "unhealthy" => ServiceHealthStatus::Degraded,
                _ => ServiceHealthStatus::Degraded,
            };
            // Container is still running during health checks
            (OfferingStatus::Running, health)
        }
        _ => return, // Unknown action, ignore
    };

    // Only update if something actually changed
    if new_status == old_status && new_health == old_health {
        return;
    }

    tracing::info!(
        offering = %offering_name,
        old_status = ?old_status,
        new_status = ?new_status,
        old_health = ?old_health,
        new_health = ?new_health,
        action = %action,
        "Offering state changed (Docker event)"
    );

    let stone_id = state.current.stone.id.clone();

    // Delegate mutation + event emission to Health aggregate (ARCH-0024)
    state
        .health
        .apply_docker_event(
            &state.offerings,
            &offering_id,
            &offering_name,
            &old_health,
            new_status,
            new_health.clone(),
        )
        .await;

    // Emit domain event for listeners (chirp, pulse/SSE, companions)
    let domain_event = match action {
        "start" => OfferingEvent::started(&offering_id, &offering_name, &stone_id),
        "stop" | "die" | "destroy" => {
            OfferingEvent::stopped(&offering_id, &offering_name, &stone_id)
        }
        a if a.starts_with("health_status:") => OfferingEvent::health_changed(
            &offering_id,
            &offering_name,
            &stone_id,
            format!("{:?}", new_health),
        ),
        _ => return,
    };

    state.event_bus.emit(domain_event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_action_parsing() {
        // Verify the health_status action prefix parsing logic
        let action = "health_status: healthy";
        assert!(action.starts_with("health_status:"));
        let health_str = action.trim_start_matches("health_status:").trim();
        assert_eq!(health_str, "healthy");

        let action = "health_status: unhealthy";
        let health_str = action.trim_start_matches("health_status:").trim();
        assert_eq!(health_str, "unhealthy");
    }

    #[test]
    fn test_container_name_filtering() {
        // Verify that only zen-offering containers are processed
        assert!("zen-offering-mongodb".starts_with(OFFERING_CONTAINER_PREFIX));
        assert!("zen-offering-ollama--dev".starts_with(OFFERING_CONTAINER_PREFIX));
        assert!(!"my-custom-container".starts_with(OFFERING_CONTAINER_PREFIX));
        assert!(!"zen-companion-cricket".starts_with(OFFERING_CONTAINER_PREFIX));
    }

    #[test]
    fn test_backoff_calculation() {
        let mut backoff = INITIAL_RECONNECT_BACKOFF;
        assert_eq!(backoff, Duration::from_secs(2));

        backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
        assert_eq!(backoff, Duration::from_secs(4));

        backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
        assert_eq!(backoff, Duration::from_secs(8));

        // Eventually caps at MAX_RECONNECT_BACKOFF
        for _ in 0..10 {
            backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
        }
        assert_eq!(backoff, MAX_RECONNECT_BACKOFF);
    }
}
