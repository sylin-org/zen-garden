//! Service discovery background tasks
//!
//! Handles continuous registration with service discovery systems:
//! - Lantern service registry (centralized discovery)
//! - mDNS broadcasts (local network discovery) - future
//! - Pond synchronization (distributed discovery) - future

use std::sync::Arc;

use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use garden_common::infra::communications::p2p;
use tokio_util::sync::CancellationToken;

use crate::AppState;
use crate::domain::topology::{
    TopologyCache, TopologyDirtyFlag, mark_stone_offline_dirty, upsert_from_chirp_dirty,
};

/// Lantern registration loop - registers this stone with Lantern every 45 seconds
///
/// Continuously registers this stone with the Lantern service discovery system.
/// Sends POST /api/register with stone ID, name, endpoint, and current service list.
///
/// Only runs if LANTERN_ENDPOINT environment variable is set.
pub async fn lantern_registration_loop(
    stone_id: String,
    stone_name: String,
    endpoint: String,
    lantern_endpoint: String,
    state: AppState,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    use garden_common::{RegisterRequest, RegisterServiceInfo};
    use reqwest::Client;

    tracing::info!(
        stone_id = %stone_id,
        stone_name = %stone_name,
        lantern_endpoint = %lantern_endpoint,
        "Starting Lantern registration loop"
    );

    let client = Client::new();
    let register_url = format!("{}/api/v1/register", lantern_endpoint);

    loop {
        // Build service list from current offerings
        let services = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .map(|o| RegisterServiceInfo {
                    name: o.name.to_string(),
                    service_type: o.offering.clone(),
                    status: o.status.to_string(),
                    connection_string: format!(
                        "{}:{}",
                        endpoint,
                        o.location.port_map.values().next().copied().unwrap_or(0)
                    ),
                })
                .collect()
        };

        let request = RegisterRequest {
            stone_id: Some(stone_id.clone()),
            stone_name: stone_name.clone(),
            endpoint: endpoint.clone(),
            services,
        };

        match client.post(&register_url).json(&request).send().await {
            Ok(response) if response.status().is_success() => {
                tracing::debug!("Registered with Lantern successfully");
            }
            Ok(response) => {
                tracing::warn!(
                    status = ?response.status(),
                    "Lantern registration returned non-success status"
                );
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to register with Lantern");
            }
        }

        // Sleep for 45 seconds before next heartbeat
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(45)) => {}
            _ = token.cancelled() => {
                tracing::info!("Lantern registration loop shutting down (cancellation requested)");
                return Ok(());
            }
        }
    }
}
// 14 individual parameters rather than a config struct is pre-existing.
// Collapsing into a struct would be mechanical but out of scope for
// Chapter 2 / Book I. Flagged for the future bootstrap-cleanup book.
#[allow(clippy::too_many_arguments)]
pub async fn start_discovery_listener(
    stone_id: String,
    stone_name: String,
    api_endpoint: String,
    topology_cache: TopologyCache,
    topology_dirty: TopologyDirtyFlag,
    tools: tokio::sync::broadcast::Sender<garden_common::tools::ToolDelta>,
    registry: crate::domain::GardenRegistry,
    address: Arc<tokio::sync::RwLock<garden_common::PeerAddress>>,
    console: Arc<ConsolePrinter>,
    infrastructure_handlers: Arc<crate::domain::InfrastructureHandlerRegistry>,
    manifest_registry: Arc<crate::infra::ManifestRegistry>,
    orchestration_nudge: Arc<tokio::sync::Notify>,
    volumes: crate::domain::Volumes,
    token: CancellationToken,
) {
    // Spawn UDP event monitor that handles chirps, goodbyes, and storage beacons
    tokio::spawn(async move {
        let mut all_events = match p2p::subscribe_to_all().await {
            Ok(rx) => rx,
            Err(e) => {
                tracing::error!(error = ?e, "Failed to subscribe to p2p events");
                console.emit(ConsoleEvent::new(
                    EventCategory::Network,
                    EventStatus::Failed,
                    format!("UDP listener: {}", e),
                ));
                return;
            }
        };

        console.emit(ConsoleEvent::new(
            EventCategory::Network,
            EventStatus::Started,
            format!(
                "UDP listener on port {}",
                garden_common::constants::DISCOVERY_UDP
            ),
        ));

        while let Some((announcement_type, payload, from_addr)) = all_events.recv().await {
            // MOSS-0004: check shutdown token each iteration
            if token.is_cancelled() {
                tracing::debug!("Discovery listener shutting down (MOSS-0004)");
                break;
            }
            match announcement_type.as_str() {
                garden_common::infra::communications::announcement_types::STONE_CHIRP => {
                    let chirp: garden_common::TopologyEntry = match serde_json::from_value(payload)
                    {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to parse chirp");
                            continue;
                        }
                    };

                    // Check if this is a NEW stone (not already in cache)
                    let is_new_stone = {
                        let cache = topology_cache.read().await;
                        !cache.contains_key(&chirp.stone_id)
                    };

                    tracing::debug!(
                        stone = %chirp.stone_name,
                        services = chirp.services.len(),
                        mac = ?chirp.mac,
                        health = %chirp.health,
                        from = %from_addr,
                        is_new = is_new_stone,
                        "Stone chirp received, updating topology cache"
                    );

                    // Update topology cache with chirp data (marks dirty for persistence)
                    upsert_from_chirp_dirty(&topology_cache, chirp.clone(), &topology_dirty).await;

                    // Trigger infrastructure handlers (MOSS-0002: garden-wide effects)
                    // Handlers react to topology changes and configure local infrastructure
                    // (e.g., Docker insecure-registries for container registries)
                    {
                        let handlers = infrastructure_handlers.clone();
                        let cache = topology_cache.clone();
                        let manifests = manifest_registry.clone();
                        tokio::spawn(async move {
                            handlers.on_topology_changed(&cache, &manifests).await;
                        });
                    }

                    // STORAGE-0003: If new stone, broadcast our storage beacon (if we have storage)
                    if is_new_stone && chirp.stone_id != stone_id {
                        let local_stone_id = stone_id.clone();
                        let local_stone_name = stone_name.clone();
                        let local_endpoint = api_endpoint.clone();
                        let local_address = address.clone();
                        let local_registry = registry.clone();
                        let local_volumes = volumes.clone();
                        tokio::spawn(async move {
                            let resolved_endpoint = {
                                let current = local_address.read().await.http_base();
                                if current.contains("0.0.0.0") {
                                    local_endpoint
                                } else {
                                    current
                                }
                            };

                            let roles =
                                crate::domain::storage::roles_snapshot(&local_volumes).await;
                            let pins = crate::domain::storage::pins_snapshot(&local_volumes).await;
                            match crate::infra::storage::broadcast_if_has_storage(
                                &local_stone_id,
                                &local_stone_name,
                                &resolved_endpoint,
                                &local_volumes,
                                Some(&roles),
                                Some(&pins),
                            )
                            .await
                            {
                                Ok(true) => {
                                    tracing::debug!(
                                        new_stone = %chirp.stone_name,
                                        "Broadcast storage beacon for new stone"
                                    );
                                }
                                Ok(false) => {
                                    // No storage, nothing to broadcast
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        new_stone = %chirp.stone_name,
                                        "Failed to broadcast storage beacon for new stone"
                                    );
                                }
                            }

                            // TOOLS-0003: Broadcast current local tools snapshot for new stone.
                            let snapshot_deltas = {
                                let reg = local_registry.read().await;
                                reg.local_snapshot_for_beacon(&local_stone_id)
                            };
                            if let Err(e) = crate::infra::broadcast_tools_snapshot_beacon(
                                &local_stone_id,
                                &local_stone_name,
                                &resolved_endpoint,
                                snapshot_deltas,
                            )
                            .await
                            {
                                tracing::warn!(
                                    error = %e,
                                    new_stone = %chirp.stone_name,
                                    "Failed to broadcast tools snapshot beacon for new stone"
                                );
                            }
                        });
                    }
                }
                garden_common::infra::communications::announcement_types::STONE_GOODBYE => {
                    let goodbye: garden_common::StoneGoodbyePayload =
                        match serde_json::from_value(payload) {
                            Ok(g) => g,
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to parse goodbye");
                                continue;
                            }
                        };

                    tracing::info!(
                        stone = %goodbye.stone_name,
                        from = %from_addr,
                        "Stone goodbye received, marking offline"
                    );
                    // Mark stone as offline immediately (marks dirty for persistence)
                    mark_stone_offline_dirty(&topology_cache, &goodbye.stone_id, &topology_dirty)
                        .await;

                    // TOOLS-0003: Remove all entries for offline stone from registry
                    let removed = {
                        let mut reg = registry.write().await;
                        reg.remove_stone(&goodbye.stone_id)
                    };
                    for delta in &removed {
                        let _ = tools.send(delta.clone());
                    }
                }
                garden_common::infra::communications::announcement_types::STORAGE_BEACON => {
                    // STORAGE-0003: Handle storage beacon from peer
                    let beacon: garden_common::storage::StorageBeacon =
                        match serde_json::from_value(payload) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to parse storage beacon");
                                continue;
                            }
                        };

                    tracing::debug!(
                        stone = %beacon.stone_name,
                        seed_banks = beacon.storages.len(),
                        from = %from_addr,
                        "Storage beacon received, updating storage cache"
                    );

                    // TOOLS-0003: Storage data now flows through ToolsBeacon / registry.
                    // StorageBeacon is kept for orchestration nudge only.

                    // Nudge orchestration so role resolution happens immediately
                    orchestration_nudge.notify_one();
                }
                garden_common::infra::communications::announcement_types::TOOLS_BEACON => {
                    let beacon: garden_common::tools::ToolsBeacon =
                        match serde_json::from_value(payload) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to parse tools beacon");
                                continue;
                            }
                        };

                    if beacon.stone_id == stone_id {
                        continue;
                    }

                    tracing::info!(
                        stone = %beacon.stone_name,
                        deltas = beacon.deltas.len(),
                        from = %from_addr,
                        "Tools beacon received from stone {}",
                        beacon.stone_name,
                    );

                    // TOOLS-0003: Apply to unified registry
                    let applied = {
                        let mut reg = registry.write().await;
                        reg.apply_remote_beacon(&beacon)
                    };
                    for delta in &applied {
                        if let Some(tool) = &delta.tool {
                            if tool.tool.category == garden_common::constants::CATEGORY_ORCHESTRATOR
                            {
                                tracing::info!(
                                    stone = %beacon.stone_name,
                                    offering = %tool.tool.tool_type,
                                    fqid = %tool.fqid,
                                    "Stone {} announces {} gateway for {}",
                                    beacon.stone_name,
                                    tool.fqid,
                                    tool.tool.tool_type,
                                );
                            }
                        } else if matches!(delta.kind, garden_common::tools::ToolDeltaKind::Remove)
                        {
                            tracing::info!(
                                stone = %beacon.stone_name,
                                fqid = %delta.fqid,
                                "Stone {} announces FQN handler removal for {}",
                                beacon.stone_name,
                                delta.fqid,
                            );
                        }
                        let _ = tools.send(delta.clone());
                    }
                }
                _ => {
                    // Ignore other announcement types (election events handled by election service, discovery handled by discovery_handler)
                }
            }
        }
        tracing::info!("UDP event monitor stopped");
    });
}
