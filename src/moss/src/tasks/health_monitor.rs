//! Health monitoring background task
//!
//! Continuous monitoring loop that:
//! - Polls Docker container health every 30 seconds
//! - Updates offerings registry with current status/health
//! - Adopts unregistered zen-offering containers (self-heal)
//! - Updates resource metrics (CPU, memory)
//!
//! This is a non-blocking background task that runs for the lifetime of the daemon.

use crate::domain::adopt_offering_container;
use crate::AppState;
use garden_common::{
    NotificationTag, OfferingStatus, ServiceHealthStatus, NOTIF_SOURCE_OFFERINGS_DEGRADED,
};
use std::sync::atomic::Ordering;

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

        // Check Docker availability before attempting container operations
        // If Docker is unavailable, skip container health checks but continue other work
        if !state.subsystems.docker.ready.load(Ordering::Relaxed) {
            tracing::debug!("Health monitor: Docker unavailable, skipping container checks");
            continue;
        }

        // Get snapshot of managed offerings to check
        let managed_snapshot: Vec<(String, String, OfferingStatus, ServiceHealthStatus)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| o.is_managed())
                .map(|o| {
                    (
                        o.offering_id.clone(),
                        o.name.clone(),
                        o.status,
                        o.health.clone(),
                    )
                })
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
                if let Some(offering) = offerings.iter_mut().find(|o| o.offering_id == offering_id)
                {
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
                if let Some(offering) = offerings.iter_mut().find(|o| o.offering_id == offering_id)
                {
                    if let Some(ref mut managed) = offering.managed_data_mut() {
                        managed.resources = Some(resources);
                    }
                }
            }

            // Port reconciliation: detect if Docker port bindings differ from registry
            if new_status == OfferingStatus::Running {
                if let Ok(docker_ports) = state.docker.get_container_ports(&name).await {
                    if let Some((actual_host_port, _)) = docker_ports.first() {
                        let mut offerings = state.offerings.write().await;
                        if let Some(offering) =
                            offerings.iter_mut().find(|o| o.offering_id == offering_id)
                        {
                            if offering.location.port != *actual_host_port {
                                tracing::info!(
                                    offering = %name,
                                    registry_port = offering.location.port,
                                    docker_port = *actual_host_port,
                                    "Port mismatch detected, updating registry"
                                );
                                offering.location.port = *actual_host_port;
                                state_changed = true;
                            }
                        }
                    }
                }
            }
        }

        // TOPO-0002: Check running managed containers for missing topology mount.
        // Containers created before the topology directory feature lack the bind mount.
        // Recreate them to pick up the auto-injected mount from install_service().
        {
            let running_snapshot: Vec<String> = {
                let offerings = state.offerings.read().await;
                offerings
                    .iter()
                    .filter(|o| o.is_managed() && o.status == OfferingStatus::Running)
                    .map(|o| o.name.clone())
                    .collect()
            };

            for name in &running_snapshot {
                match state.docker.has_topology_mount(name).await {
                    Ok(true) => {} // mount present, nothing to do
                    Ok(false) => {
                        tracing::warn!(
                            offering = %name,
                            "Container missing topology mount, recreating"
                        );
                        match state.docker.get_container_recreate_config(name).await {
                            Ok((image, ports, env, volumes)) => {
                                if let Err(e) = state.docker.remove_service(name, None).await {
                                    tracing::error!(offering = %name, error = ?e, "Failed to remove container for mount remediation");
                                    continue;
                                }
                                if let Err(e) = state
                                    .docker
                                    .install_service(name, &image, ports, env, volumes, None)
                                    .await
                                {
                                    tracing::error!(offering = %name, error = ?e, "Failed to recreate container with topology mount");
                                } else {
                                    tracing::info!(offering = %name, "Recreated container with topology mount");
                                    state_changed = true;
                                }
                            }
                            Err(e) => {
                                tracing::error!(offering = %name, error = ?e, "Failed to extract container config for recreation");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!(offering = %name, error = ?e, "Could not check topology mount");
                    }
                }
            }
        }

        // Update notification registry based on current offering health
        // Set "attention" tag if any offering is degraded or offline
        {
            let offerings = state.offerings.read().await;
            let has_degraded = offerings.iter().any(|o| {
                matches!(
                    o.health,
                    ServiceHealthStatus::Degraded | ServiceHealthStatus::Offline
                )
            });
            state.notifications.set_if(
                NOTIF_SOURCE_OFFERINGS_DEGRADED,
                NotificationTag::Attention,
                has_degraded,
            );
        }

        // Check for containers not in offerings (external changes)
        // This provides self-heal: if someone manually starts a zen-offering container,
        // moss will adopt it into the offerings registry
        let cached_caps = state.capabilities.read().await.clone();
        let cached_caps_ref = cached_caps.as_ref();
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
                        match adopt_offering_container(
                            &state.docker,
                            &state.manifest_registry,
                            container_name,
                            &state.stone_name,
                            cached_caps_ref,
                        )
                        .await
                        {
                            Ok(Some(offering)) => {
                                state.upsert_offering(offering, true).await;
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
