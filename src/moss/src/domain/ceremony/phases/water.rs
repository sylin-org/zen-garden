//! Water phase - bring service up and verify health
//!
//! Starts the newly created container and waits for it to become healthy.
//! If health check fails and auto_rollback is enabled, restores from harvest.

use crate::infra::restore_harvest;
use crate::AppState;
use anyhow::{Context, Result};
use std::time::Duration;

/// Default health check timeout
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 120;

/// Health check polling interval
const HEALTH_POLL_INTERVAL_SECS: u64 = 3;

/// Execute the water phase
///
/// Starts the container and waits for health check to pass.
/// If health fails and we have a harvest, rolls back automatically.
pub async fn execute(
    state: &AppState,
    offering: &str,
    harvest_id: Option<&str>,
    auto_rollback: bool,
) -> Result<()> {
    tracing::info!(offering, "Starting water phase");

    // Step 1: Start the container
    state
        .docker
        .start_service(offering, Some(&state.console))
        .await
        .context("Failed to start container")?;

    tracing::info!(offering, "Container started, waiting for health check");

    // Step 2: Wait for health
    let timeout = Duration::from_secs(DEFAULT_HEALTH_TIMEOUT_SECS);
    let healthy = wait_for_health(state, offering, timeout).await;

    if healthy {
        tracing::info!(offering, "Service is healthy - water phase completed");
        return Ok(());
    }

    // Health check failed
    tracing::warn!(offering, "Health check failed after {:?}", timeout);

    // Step 3: Handle failure - rollback or bail
    if let Some(harvest_id) = harvest_id.filter(|_| auto_rollback) {
        tracing::warn!(
            offering,
            harvest_id,
            "Auto-rollback enabled, restoring from harvest"
        );

        // Stop the unhealthy container
        let _ = state
            .docker
            .stop_service(offering, Some(&state.console))
            .await;

        // Restore volumes from harvest
        restore_harvest(&state.docker, &state.harvest_store, harvest_id)
            .await
            .context("Failed to restore from harvest during rollback")?;

        // Start the container again (now with original volumes)
        state
            .docker
            .start_service(offering, Some(&state.console))
            .await
            .context("Failed to start container after rollback")?;

        // Verify rollback succeeded
        let rollback_healthy = wait_for_health(state, offering, Duration::from_secs(60)).await;

        if rollback_healthy {
            anyhow::bail!("Health check failed after nourishment, rolled back to previous version");
        } else {
            anyhow::bail!(
                "Health check failed after nourishment AND after rollback - manual intervention required"
            );
        }
    } else {
        anyhow::bail!(
            "Health check failed after nourishment (auto_rollback={}, harvest={})",
            auto_rollback,
            harvest_id.is_some()
        );
    }
}

/// Wait for container to become healthy
///
/// Polls health status until healthy or timeout expires.
async fn wait_for_health(state: &AppState, offering: &str, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_secs(HEALTH_POLL_INTERVAL_SECS);

    while start.elapsed() < timeout {
        match state.docker.get_service_health(offering).await {
            Ok(health) => {
                if health == garden_common::ServiceHealthStatus::Healthy {
                    return true;
                }
                tracing::debug!(
                    offering = offering,
                    health = ?health,
                    elapsed = ?start.elapsed(),
                    "Waiting for health..."
                );
            }
            Err(e) => {
                tracing::debug!(
                    offering = offering,
                    error = %e,
                    "Health check error, retrying..."
                );
            }
        }
        tokio::time::sleep(poll_interval).await;
    }

    false
}

#[cfg(test)]
mod tests {
    // Integration tests require Docker - see tests/ceremony_integration.rs
}
