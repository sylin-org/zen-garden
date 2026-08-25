//! Water phase - bring service up and verify health
//!
//! Starts the newly created container and waits for it to become healthy.
//! If health check fails and auto_rollback is enabled, restores from harvest.
//!
//! The wait-for-healthy polling logic lives in
//! [`crate::domain::health::wait`] so plant flows (ORCH-0039) can
//! reuse it without dragging in ceremony state management. Water
//! drives the same primitive plus the rollback bookkeeping.

use crate::Moss;
use crate::domain::traits::HarvestOps;
use anyhow::{Context, Result};
use std::time::Duration;

/// Default health check timeout
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 120;

/// Execute the water phase
///
/// Starts the container and waits for health check to pass.
/// If health fails and we have a harvest, rolls back automatically.
pub async fn execute(
    state: &Moss,
    offering: &str,
    harvest_id: Option<&str>,
    auto_rollback: bool,
) -> Result<()> {
    tracing::info!(offering, "Starting water phase");

    // Step 1: Start the container
    state
        .platform
        .container
        .start_service(offering, Some(&state.console))
        .await
        .context("Failed to start container")?;

    tracing::info!(offering, "Container started, waiting for health check");

    // Step 2: Wait for health
    let timeout = Duration::from_secs(DEFAULT_HEALTH_TIMEOUT_SECS);
    let healthy = state.health.wait_until_healthy(offering, timeout).await;

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
            .platform
            .container
            .stop_service(offering, Some(&state.console))
            .await;

        // Restore volumes from harvest via trait object (no infra import)
        state
            .nurturing
            .harvest_ops
            .restore_harvest(harvest_id)
            .await
            .context("Failed to restore from harvest during rollback")?;

        // Start the container again (now with original volumes)
        state
            .platform
            .container
            .start_service(offering, Some(&state.console))
            .await
            .context("Failed to start container after rollback")?;

        // Verify rollback succeeded
        let rollback_healthy = state
            .health
            .wait_until_healthy(offering, Duration::from_secs(60))
            .await;

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

#[cfg(test)]
mod tests {
    // Integration tests require Docker - see tests/ceremony_integration.rs
}
