//! Health monitoring background task
//!
//! Continuous monitoring loop that:
//! - Polls Docker container health every 30 seconds
//! - Updates offerings registry with current status/health
//! - Adopts unregistered zen-offering containers (self-heal)
//! - Updates resource metrics (CPU, memory)
//!
//! This is a non-blocking background task that runs for the lifetime of the daemon.

use crate::AppState;
use crate::domain::adopt_offering_container;
use garden_common::{OfferingStatus, ServiceHealthStatus};

/// Background health monitoring loop
///
/// This task should be spawned with tokio::spawn() at daemon startup.
/// It runs indefinitely, polling Docker every 30 seconds.
///
/// # Non-Blocking
/// This function never returns - it's designed to run in the background
/// for the entire daemon lifetime. Spawn it and forget it.
///
/// # What It Does
/// 1. Polls all registered services for status/health
/// 2. Updates registry when status/health changes
/// 3. Fetches container resource metrics (CPU, memory)
/// 4. Discovers unregistered zen-offering containers
/// 5. Adopts discoveredcontainers if they match templates (self-heal)
/// 6. Persists registry changes to disk
///
/// # Example
/// ```rust,ignore
/// // At daemon startup
/// let state_clone = state.clone();
/// tokio::spawn(async move {
///     health_monitor_task(state_clone).await;
/// });
/// // Task runs forever in background
/// ```
pub async fn health_monitor_task(state: AppState) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        // Reap any terminated Companion processes to prevent zombies
        let reaped = state.companion_registry.reap_terminated().await;
        if reaped > 0 {
            tracing::debug!(reaped = reaped, "Reaped terminated Companion processes");
        }

        // Get snapshot of managed offerings to check
        let managed_snapshot: Vec<(String, String, OfferingStatus, ServiceHealthStatus)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| o.is_managed())
                .map(|o| (o.offering_id.clone(), o.name.clone(), o.status.clone(), o.health.clone()))
                .collect()
        };

        let mut state_changed = false;

        for (offering_id, name, old_status, old_health) in managed_snapshot {
            // Check container status (convert ServiceStatus → OfferingStatus)
            let (new_status, new_health) = match state.docker.get_service_status(&name).await {
                Ok(service_status) => {
                    let health = state
                        .docker
                        .get_service_health(&name)
                        .await
                        .unwrap_or(ServiceHealthStatus::Offline);
                    (OfferingStatus::from(service_status), health)
                }
                Err(e) => {
                    tracing::warn!(
                        offering = %name,
                        error = ?e,
                        "Failed to get offering status, marking as offline"
                    );
                    (OfferingStatus::Stopped, ServiceHealthStatus::Offline)
                }
            };

            // Update offerings if status or health changed
            if new_status != old_status || new_health != old_health {
                let mut offerings = state.offerings.write().await;
                if let Some(offering) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
                    tracing::info!(
                        offering = %name,
                        old_status = ?old_status,
                        new_status = ?new_status,
                        old_health = ?old_health,
                        new_health = ?new_health,
                        "Offering state changed"
                    );
                    offering.status = new_status;
                    offering.health = new_health;
                    state_changed = true;
                }
            }

            // Update container resource metrics
            if let Ok(resources) = state.docker.get_container_stats(&name).await {
                let mut offerings = state.offerings.write().await;
                if let Some(offering) = offerings.iter_mut().find(|o| o.offering_id == offering_id) {
                    if let Some(ref mut managed) = offering.managed_data_mut() {
                        managed.resources = Some(resources);
                    }
                }
            }
        }

        // Check for containers not in offerings (external changes)
        // This provides self-heal: if someone manually starts a zen-offering container,
        // moss will adopt it into the offerings registry
        match state.docker.list_zen_containers().await {
            Ok(container_names) => {
                for container_name in &container_names {
                    // Check if already in offerings (acquire read lock briefly)
                    let exists = {
                        let offerings = state.offerings.read().await;
                        offerings.iter().any(|o| o.name == *container_name)
                    };

                    if !exists {
                        tracing::warn!(container = %container_name, "Found zen-offering container not in registry (adopting)");
                        match adopt_offering_container(&state.docker, &state.manifest_registry, container_name, &state.stone_name).await {
                            Ok(Some(info)) => {
                                // Convert to UnifiedOffering and upsert
                                let unified = garden_common::UnifiedOffering::from_service_info(info);
                                state.upsert_offering(unified, true).await;
                                state_changed = true;
                            }
                            Ok(None) => {
                                tracing::warn!(container = %container_name, "No matching template for container; leaving unregistered");
                            }
                            Err(e) => {
                                tracing::warn!(container = %container_name, error = ?e, "Failed to adopt container; leaving it alone");
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "Failed to list zen containers");
            }
        }

        // Persist if we made changes (upsert_offering already persists, but manual changes need explicit persist)
        if state_changed {
            let _ = state.persist_offerings().await;
        }
    }
}
