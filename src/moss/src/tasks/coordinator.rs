//! Background task coordination
//!
//! Orchestrates all background tasks that run during daemon operation:
//! - UDP discovery listener
//! - Hardware capability detection
//! - Registry loading and container adoption
//! - Offerings catalog building
//! - Manifest loading
//! - Health monitoring
//! - Auto-adoption
//! - Lantern registration and network event handling
//!
//! Extracted from main.rs for cleaner separation of concerns.

use crate::domain::topology::{
    mark_stone_offline_dirty, upsert_from_chirp_dirty, TopologyCache, TopologyDirtyFlag,
};
use crate::tasks::backfill_missing_guidance;
use crate::tasks::network_monitor::{NetworkEvent, Network};
use crate::tasks::task_scheduler::{backfill_missing_tasks, start_task_scheduler};
use crate::{
    adopt_existing_containers, auto_adoption_task, detect_capabilities_background,
    ensure_offerings_index, health_monitor_task, infra, lantern_registration_loop, AppState,
};
use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use garden_common::infra::communications::p2p;
use garden_common::{HardwareCapabilities, ServiceHealthStatus};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Start topology maintenance task (TOPO-0002: with persistence)
///
/// Periodically marks stale stones as offline, evicts old offline stones,
/// and flushes dirty topology cache to disk.
/// Runs every 30 seconds (aligns with stone chirp interval).
pub fn start_topology_maintenance(
    topology_cache: TopologyCache,
    topology_dirty: TopologyDirtyFlag,
    self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,
    token: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        interval.tick().await; // Skip first immediate tick

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Topology maintenance shutting down (MOSS-0004)");
                    break;
                }
            }
            let (marked, evicted) = crate::domain::topology::maintain_and_persist(
                &topology_cache,
                &topology_dirty,
                &self_entry,
            )
            .await;
            if marked > 0 || evicted > 0 {
                tracing::debug!(
                    marked_offline = marked,
                    evicted = evicted,
                    "Topology maintenance complete"
                );
            }
        }
    });
}

/// Periodically reaps expired gateway entries from the registry.
///
/// Runs every 15 seconds (gateway TTL is 60s, orchestrators refresh every 30s).
/// Reaped entries are broadcast via SSE **and** UDP tools beacon so remote
/// stones learn about the removal promptly.
pub fn start_registry_maintenance(
    state: AppState,
    token: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        interval.tick().await; // Skip first immediate tick

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Registry maintenance shutting down");
                    break;
                }
            }
            let reaped = {
                let mut reg = state.fqn_handler.registry.write().await;
                reg.reap_expired(&state.stone_id)
            };
            if !reaped.is_empty() {
                tracing::debug!(
                    count = reaped.len(),
                    "Registry maintenance: reaped expired fqn_handler entries"
                );
                // Notify SSE subscribers AND broadcast beacon so remote
                // registries drop the reaped entries.
                state.publish_tool_deltas(reaped, true).await;
            }
        }
    });
}

/// Start UDP discovery listener with topology cache integration
///
/// Enables stone discovery via UDP broadcast.
/// Handles discovery requests (chirp response), stone chirps (topology updates),
/// goodbyes, and storage beacons (STORAGE-0003).
/// Returns immediately after spawning the listener.
#[allow(clippy::too_many_arguments)]
pub async fn start_discovery_listener(
    stone_id: String,
    stone_name: String,
    api_endpoint: String,
    topology_cache: TopologyCache,
    topology_dirty: TopologyDirtyFlag,
    tools: tokio::sync::broadcast::Sender<garden_common::tools::ToolDelta>,
    registry: crate::domain::GardenRegistry,
    self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,
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
                        let local_entry = self_entry.clone();
                        let local_registry = registry.clone();
                        let local_volumes = volumes.clone();
                        tokio::spawn(async move {
                            let resolved_endpoint = {
                                let current = local_entry.read().await.address.http_base();
                                if current.contains("0.0.0.0") {
                                    local_endpoint
                                } else {
                                    current
                                }
                            };

                            let roles = crate::domain::storage::roles_snapshot(&local_volumes).await;
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

                    tracing::debug!(
                        stone = %beacon.stone_name,
                        deltas = beacon.deltas.len(),
                        from = %from_addr,
                        "Tools beacon received, updating tools cache"
                    );

                    // TOOLS-0003: Apply to unified registry
                    let applied = {
                        let mut reg = registry.write().await;
                        reg.apply_remote_beacon(&beacon)
                    };
                    for delta in &applied {
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

/// Start background hardware detection
///
/// Progressively detects hardware capabilities (CPU fast, GPU slow).
pub fn start_hardware_detection(
    stone_name: String,
    capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,
    console: Arc<ConsolePrinter>,
    state: AppState,
) {
    tokio::spawn(async move {
        console.emit(ConsoleEvent::new(
            EventCategory::System,
            EventStatus::Scanning,
            "Hardware capabilities".to_string(),
        ));

        detect_capabilities_background(stone_name, capabilities, console.clone(), state).await;

        console.emit(ConsoleEvent::new(
            EventCategory::System,
            EventStatus::Updated,
            "Hardware capabilities (complete)".to_string(),
        ));
    });
}

/// Start registry loading and container adoption
///
/// Reconciles persisted offerings state with actual Docker state on startup.
pub fn start_registry_loader(state: AppState) {
    tokio::spawn(async move {
        // Reconcile existing offerings: if the container no longer exists, mark it offline
        // Snapshot managed offerings to avoid holding write lock during async Docker calls
        let managed_snapshot: Vec<(String, String)> = {
            let offerings = state.offerings.read().await;
            offerings
                .iter()
                .filter(|o| o.is_managed())
                .map(|o| (o.offering_id.clone(), o.name.to_string()))
                .collect()
        };
        let mut any_changed = false;
        for (offering_id, name) in managed_snapshot {
            if !state
                .docker
                .zen_container_exists(&name)
                .await
                .unwrap_or(false)
            {
                state.update_offering(&offering_id, false, |o| {
                    o.status = garden_common::OfferingStatus::Stopped;
                    o.health = ServiceHealthStatus::Offline;
                    true
                }).await;
                any_changed = true;
            }
        }
        if any_changed {
            state.sync_self_services(true).await;
            let _ = state.persist_offerings().await;
        }

        // Coalesce any duplicate offerings that accumulated from prior versions
        let coalesced = state.coalesce_duplicate_offerings().await;
        if coalesced > 0 {
            tracing::info!(
                coalesced,
                "Startup: removed duplicate offerings by FQN"
            );
        }

        // Backfill missing guidance for services that were installed before guidance caching
        let backfilled = backfill_missing_guidance(&state).await;
        if backfilled > 0 {
            tracing::info!(
                count = backfilled,
                "Backfilled guidance for existing services"
            );
        }

        // Backfill missing scheduled tasks for existing services
        let tasks_backfilled = backfill_missing_tasks(&state).await;
        if tasks_backfilled > 0 {
            tracing::info!(
                count = tasks_backfilled,
                "Backfilled scheduled tasks for existing services"
            );
        }

        // Startup self-heal: adopt any existing zen-offering containers
        adopt_existing_containers(&state).await;
    });
}

/// Start offerings catalog builder
///
/// Builds the offerings index from runtime templates.
pub fn start_catalog_builder(state: AppState, console: Arc<ConsolePrinter>) {
    tokio::spawn(async move {
        tracing::info!("Building offerings catalog...");

        console.emit(ConsoleEvent::new(
            EventCategory::Manifests,
            EventStatus::Scanning,
            "Runtime templates".to_string(),
        ));

        match ensure_offerings_index(&state, false).await {
            Ok(_) => {
                let idx_guard = state.offerings_index.read().await;
                if let Some(idx) = idx_guard.as_ref() {
                    tracing::info!(
                        offerings_count = idx.offerings.len(),
                        "Offerings catalog loaded successfully"
                    );
                    console.emit(ConsoleEvent::new(
                        EventCategory::Manifests,
                        EventStatus::Loaded,
                        format!("{} manifests", idx.offerings.len()),
                    ));
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to build offerings catalog");
                console.emit(ConsoleEvent::new(
                    EventCategory::Manifests,
                    EventStatus::Invalid,
                    "Catalog build failed".to_string(),
                ));
            }
        }
    });
}

/// Start health monitoring task
pub fn start_health_monitor(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        health_monitor_task(state, token).await;
    });
}

/// Start caretaking maintenance sweep (hourly background task)
///
/// Runs all domain sweepers sequentially every hour (5 min delay after boot).
/// Persists results to disk for API consumption.
pub fn start_maintenance_sweep(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        // Wait 5 minutes after boot before first sweep (or exit early on shutdown)
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(300)) => {}
            _ = token.cancelled() => {
                tracing::debug!("Maintenance sweep cancelled during startup delay (MOSS-0004)");
                return;
            }
        }

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Maintenance sweep shutting down (MOSS-0004)");
                    break;
                }
            }

            let run = crate::domain::maintenance::run_sweep(&state).await;
            tracing::info!(
                status = ?run.overall_status,
                duration_ms = run.duration_ms,
                domains = run.reports.len(),
                "Maintenance sweep complete"
            );

            if let Err(e) = crate::infra::maintenance_store::save_sweep_run(&run).await {
                tracing::warn!(error = ?e, "Failed to save sweep report");
            }
        }
    });
}

/// Start auto-adoption task if enabled
pub fn start_auto_adoption(
    state: AppState,
    config: infra::MossConfig,
    console: &ConsolePrinter,
    token: CancellationToken,
) {
    let adoption_config = config.adoption();
    start_auto_adoption_with_config(state, adoption_config, console, token);
}

/// Start auto-adoption task with explicit AdoptionConfig
///
/// Use this variant when no MossConfig file is available - it will use
/// deployment profile detection to determine if adoption should be enabled.
pub fn start_auto_adoption_with_config(
    state: AppState,
    adoption_config: infra::AdoptionConfig,
    console: &ConsolePrinter,
    token: CancellationToken,
) {
    if adoption_config.is_enabled() {
        tracing::info!("Auto-adoption enabled, starting adoption background task");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption enabled",
        ));

        tokio::spawn(async move {
            auto_adoption_task(state, adoption_config, token).await;
        });
    } else {
        tracing::info!("Auto-adoption disabled (deployment profile or configuration)");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption disabled",
        ));
    }
}

/// Start Lantern registration if LANTERN_ENDPOINT is configured
///
/// Spawns the main registration loop and (if using dynamic IP) an IP change handler
/// that triggers immediate re-registration when the network IP changes.
///
/// Console parameter is optional - pass None if console isn't available yet.
#[allow(clippy::too_many_arguments)]
pub async fn start_lantern_registration(
    stone_id: &str,
    stone_name: &str,
    api_endpoint: &str,
    port: u16,
    use_static_host: bool,
    network: &Network,
    console: Option<&ConsolePrinter>,
    token: CancellationToken,
) {
    let lantern_endpoint = match std::env::var(garden_common::ENV_LANTERN_ENDPOINT) {
        Ok(ep) => {
            let trimmed = ep.trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            trimmed
        }
        Err(_) => return,
    };

    if let Some(c) = console {
        c.emit(ConsoleEvent::new(
            EventCategory::Network,
            EventStatus::Starting,
            "Lantern registration",
        ));
    }

    // Main registration loop
    let reg_stone_id = stone_id.to_string();
    let reg_stone_name = stone_name.to_string();
    let reg_endpoint = api_endpoint.to_string();
    let lantern_url = lantern_endpoint.clone();

    tokio::spawn(async move {
        if let Err(e) = lantern_registration_loop(
            reg_stone_id,
            reg_stone_name,
            reg_endpoint,
            lantern_url,
            token,
        )
        .await
        {
            tracing::error!(error = ?e, "Lantern registration loop failed");
        }
    });

    // If using dynamic IP (not STONE_HOST), spawn IP change handler
    if !use_static_host {
        let change_stone_id = stone_id.to_string();
        let change_stone_name = stone_name.to_string();
        let change_lantern = lantern_endpoint.clone();
        let change_port = port;
        let mut network_rx = network.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = network_rx.recv().await {
                match event {
                    NetworkEvent::IpChanged { ref old, ref new } => {
                        let new_endpoint = format!("http://{}:{}", new, change_port);
                        tracing::info!(
                            old = %old,
                            new = %new,
                            endpoint = %new_endpoint,
                            "Network IP changed, triggering immediate Lantern re-registration"
                        );

                        // Immediate re-registration (don't wait for next heartbeat)
                        let client = reqwest::Client::new();
                        let register_url = format!("{}/api/register", change_lantern);
                        let request = garden_common::RegisterRequest {
                            stone_id: Some(change_stone_id.clone()),
                            stone_name: change_stone_name.clone(),
                            endpoint: new_endpoint,
                            services: vec![],
                        };

                        match client.post(&register_url).json(&request).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                tracing::info!("Re-registered with Lantern after IP change");
                            }
                            Ok(resp) => {
                                tracing::warn!(status = ?resp.status(), "Lantern re-registration returned non-success");
                            }
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to re-register with Lantern after IP change");
                            }
                        }
                    }
                    NetworkEvent::Reconnected { ref new } => {
                        let new_endpoint = format!("http://{}:{}", new, change_port);
                        tracing::info!(
                            new = %new,
                            endpoint = %new_endpoint,
                            "Network reconnected, triggering immediate Lantern re-registration"
                        );

                        // Immediate re-registration (don't wait for next heartbeat)
                        let client = reqwest::Client::new();
                        let register_url = format!("{}/api/register", change_lantern);
                        let request = garden_common::RegisterRequest {
                            stone_id: Some(change_stone_id.clone()),
                            stone_name: change_stone_name.clone(),
                            endpoint: new_endpoint,
                            services: vec![],
                        };

                        match client.post(&register_url).json(&request).send().await {
                            Ok(resp) if resp.status().is_success() => {
                                tracing::info!("Re-registered with Lantern after reconnect");
                            }
                            Ok(resp) => {
                                tracing::warn!(status = ?resp.status(), "Lantern re-registration returned non-success");
                            }
                            Err(e) => {
                                tracing::warn!(error = ?e, "Failed to re-register with Lantern after reconnect");
                            }
                        }
                    }
                    NetworkEvent::Disconnected { current, reason } => {
                        tracing::warn!(
                            ip = %current,
                            reason = %reason,
                            "Network disconnected, Lantern registration suspended until reconnect"
                        );
                    }
                }
            }
        });
    }

    if let Some(c) = console {
        c.emit(ConsoleEvent::new(
            EventCategory::Network,
            EventStatus::Started,
            "Lantern registration loop",
        ));
    }
}

/// Start resilient seed bank mount system
///
/// This is a comprehensive, self-healing mount system with two background tasks:
///
/// 1. **Mount Persistence Task** (every 5 seconds):
///    - Auto-mounts unmounted managed devices (manifest-based discovery)
///    - Health-ticks all volumes (capacity, liveness)
///    - Broadcasts storage beacon for garden-wide awareness
///
/// Unified across all platforms (STORAGE-0011). No separate MountTracker.
pub fn start_storage_lifecycle(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        interval.tick().await; // Skip first immediate tick

        tracing::info!("Storage lifecycle task started (10s interval)");

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Storage lifecycle shutting down");
                    break;
                }
            }

            // Auto-mount any unmounted managed devices (replaces legacy auto_mount_seed_banks)
            let connected = crate::domain::storage::auto_mount_unmounted(&state.storage.volumes).await;
            if !connected.is_empty() {
                for event in connected {
                    state.emit_storage_changed(event).await;
                }
                state.emit_storage_changed(
                    garden_common::storage::StorageChanged::Reclassified,
                ).await;
            }

            // Health tick all volumes (~10s)
            crate::domain::storage::health_tick_all(&state.storage.volumes).await;

            // Periodic beacon heartbeat (tools projection + beacon)
            state.refresh_local_tools_projection().await;
            state.broadcast_storage_beacon().await;
        }
    });
}

/// Subscribe to `StorageChanged` and render storage ribbons to the physical console.
///
/// Delegates to `PlatformRuntime` so output goes to the appropriate destination
/// on each platform (TTY1 on Linux, stdout on Windows).
pub fn start_storage_console_task(
    runtime: Arc<dyn garden_common::PlatformRuntime>,
    rx: tokio::sync::broadcast::Receiver<garden_common::storage::StorageChanged>,
    token: CancellationToken,
) {
    use garden_common::storage::StorageChanged;

    tokio::spawn(async move {
        let mut rx = rx;
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                result = rx.recv() => match result {
                    Ok(StorageChanged::Connected { name, roles, used_bytes }) => {
                        runtime.print_storage_connected(&name, &roles, used_bytes);
                    }
                    Ok(StorageChanged::Released { name }) => {
                        runtime.print_storage_released(&name);
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                },
            }
        }
    });
}

/// Start all background tasks
///
/// Convenience function to start all standard background tasks.
/// Call this after AppState is constructed.
pub async fn start_all_background_tasks(
    state: &AppState,
    stone_name: &str,
    api_endpoint: &str,
    capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,
    config: Option<infra::MossConfig>,
    token: CancellationToken,
) {
    let console = state.console.clone();

    // Start topology maintenance (mark stale offline, evict old, persist if dirty)
    start_topology_maintenance(
        state.topology_cache.clone(),
        state.topology_dirty.clone(),
        state.self_entry.clone(),
        token.child_token(),
    );

    // Start unified storage lifecycle (STORAGE-0011: auto-mount, health, beacon)
    start_storage_lifecycle(state.clone(), token.child_token());

    // Start storage console task — sole renderer of storage ribbons
    start_storage_console_task(state.runtime.clone(), state.subscribe_storage_changed(), token.child_token());

    // Start UDP discovery (immediate - critical for stone visibility)
    start_discovery_listener(
        state.stone_id.clone(),
        stone_name.to_string(),
        api_endpoint.to_string(),
        state.topology_cache.clone(),
        state.topology_dirty.clone(),
        state.tool.delta.clone(),
        state.tool.registry.clone(),
        state.self_entry.clone(),
        console.clone(),
        state.infrastructure_handlers.clone(),
        state.manifest_registry.clone(),
        state.orchestration.storage.nudge.clone(),
        state.storage.volumes.clone(),
        token.child_token(),
    )
    .await;

    // Start hardware detection (progressive)
    start_hardware_detection(
        stone_name.to_string(),
        capabilities,
        console.clone(),
        state.clone(),
    );

    // Start registry loading and adoption
    start_registry_loader(state.clone());

    // Start catalog building
    start_catalog_builder(state.clone(), console.clone());

    // Manifests already loaded via ManifestRegistry at startup

    // Start health monitoring
    start_health_monitor(state.clone(), token.child_token());

    // Start scheduled task scheduler
    start_task_scheduler(state.clone(), token.child_token());
    tracing::info!("Started scheduled task scheduler");

    // Start caretaking sweep (hourly maintenance)
    start_maintenance_sweep(state.clone(), token.child_token());

    // Start auto-adoption if configured
    if let Some(cfg) = config {
        start_auto_adoption(state.clone(), cfg, &console, token.child_token());
    } else {
        // No config - log that auto-adoption is disabled
        tracing::info!("No config provided, auto-adoption uses internal defaults");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption (no config)",
        ));
    }
}
