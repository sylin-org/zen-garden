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
use tokio_util::sync::CancellationToken;

/// Background health monitoring loop
///
/// This task should be spawned with tokio::spawn() at daemon startup.
/// It runs indefinitely, polling Docker every 30 seconds.
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
///
/// # What It Does
/// 1. Polls all registered services for status/health
/// 2. Updates registry when status/health changes
/// 3. Fetches container resource metrics (CPU, memory)
/// 4. Discovers unregistered zen-offering containers
/// 5. Adopts discovered containers if they match templates (self-heal)
/// 6. Persists registry changes to disk
pub async fn health_monitor_task(state: AppState, token: CancellationToken) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = token.cancelled() => {
                tracing::debug!("Health monitor shutting down (MOSS-0004)");
                break;
            }
        }

        // Reap any terminated Companion processes to prevent zombies
        let reaped = state.companion.registry.reap_terminated().await;
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
                        o.name.to_string(),
                        o.status,
                        o.health.clone(),
                    )
                })
                .collect()
        };

        let mut state_changed = false;

        for (offering_id, name, old_status, old_health) in managed_snapshot {
            // Skip offerings that are currently installing — the container may not
            // exist yet (image pull in progress). The job executor will transition
            // the status when installation completes or fails.
            if old_status == OfferingStatus::Installing {
                tracing::trace!(
                    offering = %name,
                    "Skipping health check (currently installing)"
                );
                continue;
            }

            // Check container status (convert ServiceStatus → OfferingStatus)
            let (new_status, new_health) = match state.platform.docker.get_service_status(&name).await {
                Ok(service_status) => {
                    let health = state
                        .platform.docker
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

            // Update offerings if status or health changed (via gateway — syncs self_entry)
            if new_status != old_status || new_health != old_health {
                tracing::info!(
                    offering = %name,
                    old_status = ?old_status,
                    new_status = ?new_status,
                    old_health = ?old_health,
                    new_health = ?new_health,
                    "Offering state changed"
                );
                // auto_chirp=false: defer chirp to end of cycle for batching
                state.update_offering(&offering_id, false, |o| {
                    o.status = new_status;
                    o.health = new_health;
                    true
                }).await;
                state_changed = true;
            }

            // Update container resource metrics (detail-only, no chirp impact)
            if let Ok(resources) = state.platform.docker.get_container_stats(&name).await {
                state.update_offering(&offering_id, false, |o| {
                    if let Some(ref mut managed) = o.managed_data_mut() {
                        managed.resources = Some(resources);
                    }
                    false // resources are detail-only, don't trigger sync
                }).await;
            }

            // Port reconciliation: detect if Docker port bindings differ from registry.
            // Prefer the port matching the existing registry port (the manifest's primary
            // port), so multi-port services don't randomly switch to a secondary port.
            if new_status == OfferingStatus::Running {
                if let Ok(docker_ports) = state.platform.docker.get_container_ports(&name).await {
                    let current_port = {
                        let offerings = state.offerings.read().await;
                        offerings.iter().find(|o| o.offering_id == offering_id)
                            .map(|o| o.location.port)
                    };
                    if let Some(current_port) = current_port {
                        let best_port = docker_ports
                            .iter()
                            .find(|(h, _)| *h == current_port)
                            .or(docker_ports.first())
                            .map(|(h, _)| *h);

                        if let Some(actual_host_port) = best_port {
                            if current_port != actual_host_port {
                                tracing::info!(
                                    offering = %name,
                                    registry_port = current_port,
                                    docker_port = actual_host_port,
                                    "Port mismatch detected, updating registry"
                                );
                                state.update_offering(&offering_id, false, |o| {
                                    o.location.port = actual_host_port;
                                    true
                                }).await;
                                state_changed = true;
                            }
                        }
                    }
                }
            }

            // Protocol reconciliation: ensure the protocol matches the manifest/category
            // definition. Corrects stale "tcp" entries from before protocol inference was
            // wired into installation (self-heal on upgrade).
            if new_status == OfferingStatus::Running {
                if let Some(template) = state.manifest_registry.get_offering(&name) {
                    let expected_protocol =
                        crate::domain::connection::infer_protocol_from_manifest_metadata(
                            &name,
                            &template.category,
                            template.connection.as_ref(),
                        );

                    let current_protocol = {
                        let offerings = state.offerings.read().await;
                        offerings.iter().find(|o| o.offering_id == offering_id)
                            .map(|o| o.location.protocol.clone())
                    };
                    if let Some(current_protocol) = current_protocol {
                        if current_protocol != expected_protocol {
                            tracing::info!(
                                offering = %name,
                                old_protocol = %current_protocol,
                                new_protocol = %expected_protocol,
                                "Protocol mismatch detected, updating registry"
                            );
                            state.update_offering(&offering_id, false, |o| {
                                o.location.protocol = expected_protocol;
                                true
                            }).await;
                            state_changed = true;
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
                    .map(|o| o.name.to_string())
                    .collect()
            };

            for name in &running_snapshot {
                match state.platform.docker.has_topology_mount(name).await {
                    Ok(true) => {} // mount present, nothing to do
                    Ok(false) => {
                        tracing::warn!(
                            offering = %name,
                            "Container missing topology mount, recreating"
                        );
                        match state.platform.docker.inspect_container_spec(name).await {
                            Ok(spec) => {
                                if let Err(e) = state.platform.docker.remove_service(name, None).await {
                                    tracing::error!(offering = %name, error = ?e, "Failed to remove container for mount remediation");
                                    continue;
                                }
                                if let Err(e) = state
                                    .platform.docker
                                    .install_service(name, &spec, None)
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
            state.presence.notifications.set_if(
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
        match state.platform.docker.list_zen_containers().await {
            Ok(container_names) => {
                for container_name in &container_names {
                    // Check if already in offerings (acquire read lock briefly)
                    let exists = {
                        let offerings = state.offerings.read().await;
                        offerings.iter().any(|o| o.name.to_string() == *container_name)
                    };

                    if !exists {
                        tracing::warn!(container = %container_name, "Found zen-offering container not in registry (adopting)");
                        match adopt_offering_container(
                            &state.platform.docker,
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

        // Sync + chirp + persist if we made changes.
        // The gateway calls above used auto_chirp=false to batch changes;
        // now emit a single chirp with the final state.
        if state_changed {
            state.sync_self_services(true).await;
            let _ = state.persist_offerings().await;
        }
    }
}
