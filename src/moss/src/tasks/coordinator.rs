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
    ensure_offerings_index, health_monitor_task, infra, lantern_registration_loop, mdns, AppState,
};
use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use garden_common::infra::communications::p2p;
use garden_common::{HardwareCapabilities, ServiceHealthStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::domain::traits::ManagementStoreOps;
use crate::infra::storage::{ContentStore, OsPlatform};

/// Start topology maintenance task (TOPO-0002: with persistence)
///
/// Periodically marks stale stones as offline, evicts old offline stones,
/// and flushes dirty topology cache to disk.
/// Runs every 30 seconds (aligns with stone chirp interval).
pub fn start_topology_maintenance(
    topology_cache: TopologyCache,
    topology_dirty: TopologyDirtyFlag,
    self_entry: Arc<RwLock<garden_common::TopologyEntry>>,
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
                reg.reap_expired(&state.current.stone.id)
            };
            if !reaped.is_empty() {
                tracing::info!(
                    count = reaped.len(),
                    "Registry maintenance: reaped expired FQN handler entries"
                );
                for r in &reaped {
                    tracing::info!(
                        fqid = %r.fqid,
                        "{} FQN handler entry expired (stale)",
                        r.fqid,
                    );
                }
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
    self_entry: Arc<RwLock<garden_common::TopologyEntry>>,
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
                            if tool.tool.category == "orchestrator" {
                                tracing::info!(
                                    stone = %beacon.stone_name,
                                    offering = %tool.tool.tool_type,
                                    fqid = %tool.fqid,
                                    "Stone {} announces {} FQN handler registration for {}",
                                    beacon.stone_name,
                                    tool.fqid,
                                    tool.tool.tool_type,
                                );
                            }
                        } else if matches!(delta.kind, garden_common::tools::ToolDeltaKind::Remove) {
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
                .platform.docker
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

        match ensure_offerings_index(&state, false, &crate::infra::persistence::OsOfferingsCache).await {
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

            let task_store = crate::infra::TaskStore::new();
            let run = crate::domain::maintenance::run_sweep(&state, &task_store).await;
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
    let lantern_endpoint = match std::env::var(garden_common::constants::ENV_LANTERN_ENDPOINT) {
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

            // Auto-mount any unmounted managed devices.
            // Connected ribbons come from the VolumeMonitor — no events emitted here.
            let mounted = crate::domain::storage::auto_mount_unmounted(&OsPlatform).await;
            if mounted > 0 {
                state.emit_storage_changed(
                    garden_common::storage::StorageChanged::Reclassified,
                ).await;
            }

            // Health tick all volumes (~10s)
            crate::domain::storage::health_tick_all(&state.current.storage.volumes, &OsPlatform).await;

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
                    Ok(StorageChanged::Sensed { .. }) => {}
                    Ok(StorageChanged::Connected { name, roles, used_bytes, capacity_bytes }) => {
                        runtime.print_storage_connected(&name, &roles, used_bytes, capacity_bytes);
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
        state.current.topology.cache.clone(),
        state.current.topology.dirty.clone(),
        state.current.topology.self_entry.clone(),
        token.child_token(),
    );

    // Start unified storage lifecycle (STORAGE-0011: auto-mount, health, beacon)
    start_storage_lifecycle(state.clone(), token.child_token());

    // Start storage console task — sole renderer of storage ribbons
    start_storage_console_task(state.platform.runtime.clone(), state.subscribe_storage_changed(), token.child_token());

    // Start UDP discovery (immediate - critical for stone visibility)
    start_discovery_listener(
        state.current.stone.id.clone(),
        stone_name.to_string(),
        api_endpoint.to_string(),
        state.current.topology.cache.clone(),
        state.current.topology.dirty.clone(),
        state.tool.delta.clone(),
        state.tool.registry.clone(),
        state.current.topology.self_entry.clone(),
        console.clone(),
        state.platform.handlers.clone(),
        state.manifest_registry.clone(),
        state.orchestration.storage.nudge.clone(),
        state.current.storage.volumes.clone(),
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

// ============================================================================
// Task supervisor
// ============================================================================

/// Spawn all background tasks.
///
/// Called once after `build_state` completes. Takes ownership of
/// `BuildArtifacts` (consuming `volume_rescan_rx`) and returns the API
/// endpoint string for `serve`.
pub(crate) async fn start_background_tasks(
    state: AppState,
    artifacts: crate::bootstrap::BuildArtifacts,
    config: Option<infra::MossConfig>,
) -> String {
    use garden_common::console;
    // Rebind Stage-1 locals from AppState so the task-wiring body is verbatim.
    let stone_id         = state.current.stone.id.clone();
    let stone_name       = state.current.stone.name.clone();
    let shutdown_token   = state.shutdown_token.clone();
    let capabilities     = state.current.capabilities.clone();
    let console_printer  = state.console.clone();
    let event_bus        = state.event_bus.clone();
    let pulse            = state.pulse.clone();
    let koi_handle       = state.discovery.koi.clone();
    let election_service_final = state.presence.elections.clone();
    let api_endpoint     = artifacts.api_endpoint;
    let volume_rescan_rx = artifacts.volume_rescan_rx;
    // bool: true when ZG_STONE_HOST was set (gates IP-change handler variant)
    let use_static_host  = artifacts.use_static_host;

    // Phase 11.post2: Start election service listener (subscribes to p2p events)
    tokio::spawn(async move {
        if let Err(e) = election_service_final.run_listener().await {
            tracing::error!(error = ?e, "Election service listener failed");
        }
    });

    // Phase 11.post3: Start discovery handler (responds to discovery requests)
    let self_entry_for_discovery = state.current.topology.self_entry.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::tasks::discovery_handler::start_discovery_handler(self_entry_for_discovery).await
        {
            tracing::error!(error = ?e, "Discovery handler failed");
        }
    });
    tracing::info!("Discovery handler initialized (using p2p transport)");

    // Phase 11.post4a: Initial volume scan (STORAGE-0011)
    // Populates the unified Volumes map with all currently attached volumes.
    // Cross-platform: uses platform::scan_volumes() (Linux: /proc/mounts, Windows: GetLogicalDrives).
    {
        let volumes = state.current.storage.volumes.clone();
        let platform: Arc<dyn crate::domain::traits::StoragePlatform> = Arc::new(OsPlatform);
        let make_store = |path: PathBuf| -> Arc<dyn ManagementStoreOps> {
            Arc::new(ContentStore::new(path, None))
        };
        crate::domain::storage::initial_scan(&volumes, platform, &make_store).await;
    }

    // Phase 11.post4b: Initial media scan (STORAGE-0011)
    // Detects physical disks including those without partitions or drive letters.
    // Uses PowerShell Get-Disk (Windows) or lsblk (Linux).
    {
        let media = state.current.storage.media.clone();
        let snapshots = tokio::task::spawn_blocking(crate::infra::storage::platform::scan_media)
            .await
            .unwrap_or_default();
        crate::domain::storage::reconcile_media(&media, &snapshots).await;
    }

    // Phase 11.post4: Start offering lifecycle event listeners
    {
        // ChirpListener: Broadcasts topology changes via UDP
        let self_entry_for_chirp = state.current.topology.self_entry.clone();
        let chirp_listener = Arc::new(infra::ChirpListener::new(Arc::new(move || {
            let entry = self_entry_for_chirp.clone();
            tokio::spawn(async move {
                let topology_entry = entry.read().await.clone();
                if let Err(e) = crate::announcement::announce(&topology_entry).await {
                    tracing::warn!(error = ?e, "Failed to chirp from event listener");
                }
            });
        })));
        let _chirp_handle = infra::spawn_listener(&event_bus, chirp_listener);

        // PulseDomainBridge: Bridges domain events into the unified pulse channel
        // Replaces former SseListener — presence.rs and pulse.rs subscribe to pulse
        let pulse_bridge = infra::PulseDomainBridge::new(pulse.clone());
        let _pulse_handle = infra::spawn_listener(&event_bus, Arc::new(pulse_bridge));

        // Transport tap: Bridges raw UDP announcements into pulse channel
        let _transport_tap_handle =
            infra::spawn_transport_tap(pulse.clone(), shutdown_token.clone());

        // TimerListener: Manages nurturing schedule timers (stub - no callback yet)
        let timer_listener = Arc::new(infra::TimerListener::new());
        let _timer_handle = infra::spawn_listener(&event_bus, timer_listener);

        tracing::info!("Domain event listeners started (chirp, pulse, timer)");
    }

    // Phase 11.0.5: Ceremony recovery (detect incomplete ceremonies from previous run)
    match state.recover_ceremonies().await {
        Ok(0) => tracing::debug!("No incomplete ceremonies to recover"),
        Ok(n) => tracing::warn!(count = n, "Recovered incomplete ceremonies"),
        Err(e) => tracing::error!(error = ?e, "Failed to recover ceremonies"),
    }

    // Phase 12: Start background tasks
    // UDP listener already started in Phase 1
    // ManifestRegistry already loaded in Phase 10
    start_hardware_detection(
        stone_name.clone(),
        capabilities.clone(),
        console_printer.clone(),
        state.clone(),
    );
    start_registry_loader(state.clone());
    start_catalog_builder(state.clone(), console_printer.clone());

    // System metrics collector (feeds presence protocol and health monitors)
    tracing::info!("Starting system metrics collector");
    let metrics_collector_state = state.clone();
    let metrics_token = shutdown_token.child_token();
    tokio::spawn(async move {
        crate::tasks::run_metrics_collector(metrics_collector_state, metrics_token).await;
    });

    // Companion registry scan and auto-start (discover and start Companions)
    tracing::info!("Scanning Companion registry");
    let companion_scan_state = state.clone();
    tokio::spawn(async move {
        // Get endpoint for Companion communication
        let endpoint = companion_scan_state
            .current.topology.self_entry
            .read()
            .await
            .address
            .http_base();
        match companion_scan_state
            .companion.registry
            .scan_and_autostart(&endpoint)
            .await
        {
            Ok((registered, started)) => tracing::info!(
                registered = registered,
                started = started,
                "Companion scan and auto-start complete"
            ),
            Err(e) => tracing::warn!(error = ?e, "Companion scan failed"),
        }
    });

    // Presence monitoring (PRESENCE-0001)
    tracing::info!("Starting presence load monitor");
    let load_monitor_state = state.clone();
    let load_token = shutdown_token.child_token();
    tokio::spawn(async move {
        crate::tasks::presence_monitor::run_load_monitor_task(load_monitor_state, load_token).await;
    });

    tracing::info!("Starting presence health monitor");
    let health_monitor_state = state.clone();
    let health_presence_token = shutdown_token.child_token();
    tokio::spawn(async move {
        crate::tasks::presence_monitor::run_health_monitor_task(
            health_monitor_state,
            health_presence_token,
        )
        .await;
    });

    // Phase 11.1: IP change handler (resolution announcements)
    // Uses AppState.announce_resolution_change() for proper SoC
    if !use_static_host {
        let state_for_ip = state.clone();
        let mut network_rx = state.platform.network.subscribe();

        tokio::spawn(async move {
            while let Ok(event) = network_rx.recv().await {
                let new_ip = match &event {
                    NetworkEvent::IpChanged { new, .. } => Some(new.clone()),
                    NetworkEvent::Reconnected { new } => Some(new.clone()),
                    NetworkEvent::Disconnected { .. } => None,
                };

                if let Some(ip) = new_ip {
                    // Reinitialize P2P sender sockets when network becomes available
                    // This is critical on Linux where interfaces may not be ready at boot
                    if matches!(event, NetworkEvent::Reconnected { .. }) {
                        tracing::info!("Network reconnected, reinitializing P2P senders");
                        garden_common::infra::communications::p2p::reinit_senders().await;
                    }

                    // Delegate all resolution change handling to AppState
                    state_for_ip.announce_resolution_change(&ip).await;
                }
            }
        });
        tracing::debug!("IP change handler spawned (uses AppState.announce_resolution_change)");
    }

    // Phase 11.2: mDNS health-change listener
    // Re-registers mDNS TXT record when stone health transitions (ARCH-0066)
    if let Some(ref mdns) = state.discovery.mdns {
        let mdns_for_health = mdns.clone();
        let mut health_rx = state.event_bus.subscribe();
        tokio::spawn(async move {
            loop {
                match health_rx.recv().await {
                    Ok(crate::domain::DomainEvent::Stone(
                        crate::domain::StoneEvent::HealthChanged { ref health, .. },
                    )) => {
                        mdns_for_health.update_health(health).await;
                    }
                    Ok(_) => {} // Ignore non-health events
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "mDNS health listener: missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("mDNS health listener: event bus closed");
                        break;
                    }
                }
            }
        });
        tracing::debug!("mDNS health-change listener spawned (ARCH-0066)");
    }

    // Phase 11.3: Enrollment-change listener (Pond domain event)
    // Reacts to PondEvent::EnrollmentChanged by starting/stopping HTTPS + chirp signing.
    // This eliminates the need for handlers to manage HTTPS directly.
    {
        let state_for_pond = state.clone();
        let console_for_pond = console_printer.clone();
        let mut pond_rx = state.event_bus.subscribe();
        tokio::spawn(async move {
            loop {
                match pond_rx.recv().await {
                    Ok(crate::domain::DomainEvent::Pond(
                        crate::domain::PondEvent::EnrollmentChanged { enrolled, .. },
                    )) => {
                        // Reload inter-stone TLS client with fresh cert material
                        state_for_pond.security.stone_client.reload_tls();

                        if enrolled {
                            crate::bootstrap::run::activate_pond_security(&state_for_pond, &console_for_pond).await;
                        } else {
                            // HTTPS shutdown is not implemented yet (Phase 3+).
                            // For now, just update the flag so new connections see the change.
                            state_for_pond
                                .security.https
                                .store(false, std::sync::atomic::Ordering::Relaxed);
                            tracing::info!("Pond unenrolled — HTTPS deactivated (flag cleared)");
                        }
                    }
                    Ok(_) => {} // Ignore non-pond events
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "Pond enrollment listener: missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("Pond enrollment listener: event bus closed");
                        break;
                    }
                }
            }
        });
        tracing::debug!("Pond enrollment-change listener spawned");
    }

    // Phase 11.4: Sync self_entry services after registry loads
    let state_for_sync = state.clone();
    tokio::spawn(async move {
        // Wait for registry to load
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Use helper method to sync and chirp
        state_for_sync.sync_self_services(true).await;
        tracing::debug!("Initial service sync complete");
    });

    // Phase 11.5: mDNS lurk-listener (passive topology discovery)
    // Listens for mDNS announcements from neighbor stones to populate topology cache
    let topology_cache_for_mdns = state.current.topology.cache.clone();
    let topology_dirty_for_mdns = state.current.topology.dirty.clone();
    let self_stone_name_for_mdns = stone_name.clone();
    let mdns_result = mdns::start_mdns_lurk_listener(koi_handle.clone(), stone_name.clone()).await;
    if let Ok(mut mdns_rx) = mdns_result
    {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Discovery,
            console::EventStatus::MdnsActive,
            "Lurk-listener active (passive topology discovery)".to_string(),
        ));
        tokio::spawn(async move {
            loop {
                match mdns_rx.recv().await {
                    Ok(discovered) => {
                        // Skip self-announcements (common parser no longer filters these)
                        if discovered.stone_name == self_stone_name_for_mdns {
                            continue;
                        }

                        tracing::debug!(
                            stone_id = ?discovered.stone_id,
                            stone_name = %discovered.stone_name,
                            address = %discovered.address,
                            mac = ?discovered.mac,
                            "mDNS: Neighbor stone discovered and cached"
                        );
                        // Add to topology cache (only if stone_id is present)
                        if let Some(sid) = discovered.stone_id {
                            let entry = garden_common::TopologyEntry {
                                stone_id: sid,
                                stone_name: discovered.stone_name,
                                address: discovered.address,
                                moss_version: discovered
                                    .version
                                    .unwrap_or_else(|| "unknown".to_string()),
                                services: vec![], // mDNS doesn't provide services
                                mac: discovered.mac,
                                health: discovered.health.unwrap_or_else(|| {
                                    garden_common::constants::STONE_INITIALIZING.to_string()
                                }),
                                capabilities: None, // mDNS doesn't provide capabilities
                                status: garden_common::StoneStatus::Online,
                                discovered_at: chrono::Utc::now(),
                                last_seen: chrono::Utc::now(),
                                tags: vec![], // mDNS doesn't provide tags
                                gateways: vec![], // mDNS doesn't provide gateways
                            };
                            crate::domain::topology::upsert_from_chirp_dirty(
                                &topology_cache_for_mdns,
                                entry,
                                &topology_dirty_for_mdns,
                            )
                            .await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(missed = n, "mDNS lurk-listener: missed events");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::debug!("mDNS lurk-listener channel closed");
                        break;
                    }
                }
            }
        });
    }

    // Phase 12: Active peer discovery (send discovery request at startup)
    tracing::info!("Discovering peer stones...");

    // Subscribe to discovery responses before sending request
    if let Ok(mut discovery_rx) =
        garden_common::infra::communications::p2p::subscribe_to_announcement(
            garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE,
        )
        .await
    {
        // Send discovery request
        let request = garden_common::DiscoveryRequest {
            discover: "moss".to_string(),
            request_id: garden_common::utils::ids::generate_guidv7(),
            requester: stone_id.clone(),
        };

        if let Err(e) = garden_common::infra::communications::p2p::send_announcement(
            garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
            &request,
        )
        .await
        {
            tracing::warn!(error = ?e, "Failed to send discovery request");
        } else {
            // Collect responses for 3 seconds
            let timeout_fut = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                let mut responses = Vec::new();
                while let Some((payload, _from_addr)) = discovery_rx.recv().await {
                    if let Ok(response) =
                        serde_json::from_value::<garden_common::DiscoveryResponse>(payload)
                    {
                        responses.push(response);
                    }
                }
                responses
            });

            let discovered_peers = timeout_fut.await.unwrap_or_else(|_| Vec::new());

            for peer in discovered_peers {
                if let Some(peer_id) = peer.stone_id {
                    let entry = garden_common::TopologyEntry {
                        stone_id: peer_id,
                        stone_name: peer.stone_name,
                        address: peer.address,
                        moss_version: peer.moss_version,
                        services: vec![],
                        mac: None,
                        health: garden_common::constants::STONE_INITIALIZING.to_string(),
                        capabilities: None,
                        status: garden_common::StoneStatus::Online,
                        discovered_at: chrono::Utc::now(),
                        last_seen: chrono::Utc::now(),
                        tags: vec![],
                        gateways: vec![],
                    };
                    crate::domain::topology::upsert_from_chirp_dirty(
                        &state.current.topology.cache,
                        entry,
                        &state.current.topology.dirty,
                    )
                    .await;
                }
            }
        }
    } else {
        tracing::warn!("Failed to subscribe to discovery responses");
    }

    // Phase 13: Initial announcement (announce ourselves)
    tracing::info!("Sending initial announcement...");
    let entry = state.current.topology.self_entry.read().await.clone();
    if let Err(e) = crate::announcement::announce(&entry).await {
        tracing::warn!(error = ?e, "Initial announcement failed");
    }

    // Phase 14: Start periodic announcer (30s background task)
    crate::tasks::start_periodic_announcer(state.clone(), shutdown_token.child_token());

    // Phase 16: Pre-install manifest handling
    crate::bootstrap::run::start_preinstall_handler(&state).await;

    // Phase 17: Health monitoring and auto-adoption
    start_health_monitor(state.clone(), shutdown_token.child_token());
    if let Some(cfg) = config {
        start_auto_adoption(
            state.clone(),
            cfg,
            &console_printer,
            shutdown_token.child_token(),
        );
    } else {
        // No config file - use default adoption config (profile-aware)
        start_auto_adoption_with_config(
            state.clone(),
            infra::AdoptionConfig::default(),
            &console_printer,
            shutdown_token.child_token(),
        );
    }

    // Phase 17.5: Cross-platform volume monitor (STORAGE-0014)
    // VolumeMonitor measures disk usage before emitting events — no more "0 B used" on connect.
    // StorageBank classifies volumes and emits StorageChanged; coordinator handles pulse + notifications.
    {
        use crate::infra::storage::monitor::PhysicalStorageEvent;
        use garden_common::notifications::{NotificationTag, NOTIF_SOURCE_CANDIDATES};

        let (vol_tx, mut vol_rx) = tokio::sync::mpsc::channel::<PhysicalStorageEvent>(32);
        let monitor_token = shutdown_token.child_token();
        let bank = crate::domain::StorageBank::new(
            state.current.storage.volumes.clone(),
            state.current.storage.changed.clone(),
            |path: PathBuf| -> Arc<dyn ManagementStoreOps> {
                Arc::new(ContentStore::new(path, None))
            },
        );
        crate::infra::storage::monitor::build_monitor().start(vol_tx, monitor_token.clone());

        let volumes = state.current.storage.volumes.clone();
        let pulse = state.pulse.clone();
        let notifications = state.presence.notifications.clone();
        let mut rescan_rx = volume_rescan_rx;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = monitor_token.cancelled() => break,
                    event = vol_rx.recv() => {
                        let Some(ev) = event else { break };
                        match ev {
                            PhysicalStorageEvent::Connected { device_path, mount_path, label, capacity_bytes, used_bytes, removable } => {
                                let capacity_gb = capacity_bytes / 1_000_000_000;
                                let _ = pulse.send(infra::PulseEvent::Domain(
                                    infra::DomainPulse::storage_event(
                                        "storage_detected",
                                        format!("Volume appeared: {} ({})", mount_path.display(), label.as_deref().unwrap_or("unlabeled")),
                                        "info",
                                        None,
                                        Some(serde_json::json!({
                                            "device_path": device_path,
                                            "mount_path": mount_path,
                                            "label": label,
                                            "capacity_gb": capacity_gb,
                                            "removable": removable,
                                        })),
                                    )
                                ));
                                bank.on_appeared(device_path, mount_path, label, capacity_bytes, used_bytes, removable).await;
                            }
                            PhysicalStorageEvent::Disconnected { path } => {
                                let _ = pulse.send(infra::PulseEvent::Domain(
                                    infra::DomainPulse::storage_event(
                                        "storage_removed",
                                        format!("Volume disappeared: {}", path),
                                        "info",
                                        None,
                                        Some(serde_json::json!({ "path": path })),
                                    )
                                ));
                                bank.on_vanished(path).await;
                            }
                        }

                        // Update candidates notification
                        let candidate_count = {
                            let map = volumes.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable && v.state.is_online())
                                .count()
                        };
                        notifications.set_if(
                            NOTIF_SOURCE_CANDIDATES,
                            NotificationTag::Opportunity,
                            candidate_count > 0,
                        );
                    }
                    _ = rescan_rx.recv() => {
                        // Ad-hoc rescan requested (e.g. after `storage add` wrote a manifest).
                        let snaps = tokio::task::spawn_blocking(
                            crate::infra::storage::platform::scan_volumes
                        )
                        .await
                        .unwrap_or_default();
                                                let make_store = |path: PathBuf| -> Arc<dyn ManagementStoreOps> {
                            Arc::new(ContentStore::new(path, None))
                        };
                        crate::domain::storage::reconcile(&volumes, &snaps, &make_store).await;
                        crate::domain::storage::health_tick_all(&volumes, &OsPlatform).await;

                        let candidate_count = {
                            let map = volumes.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable && v.state.is_online())
                                .count()
                        };
                        notifications.set_if(
                            NOTIF_SOURCE_CANDIDATES,
                            NotificationTag::Opportunity,
                            candidate_count > 0,
                        );
                        tracing::debug!("Ad-hoc volume rescan complete");
                    }
                }
            }
        });
        tracing::info!("Volume monitor started (STORAGE-0014)");
    }

    // Phase 17.5.2: Physical media watcher (STORAGE-0011)
    // Polls physical disks (PowerShell/lsblk) to detect media without partitions.
    // Lower cadence than the volume watcher since physical changes are rarer.
    {
        let media = state.current.storage.media.clone();
        let media_token = shutdown_token.child_token();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            interval.tick().await; // skip first immediate tick (initial scan already done)
            loop {
                tokio::select! {
                    _ = media_token.cancelled() => break,
                    _ = interval.tick() => {
                        let snapshots = tokio::task::spawn_blocking(
                            crate::infra::storage::platform::scan_media
                        )
                        .await
                        .unwrap_or_default();
                        crate::domain::storage::reconcile_media(&media, &snapshots).await;
                    }
                }
            }
        });
        tracing::info!("Media watcher started (STORAGE-0011)");
    }

    // Phase 17.6: Topology + storage cache maintenance
    // Topology: mark stale stones offline, evict old, persist dirty cache to disk
    crate::tasks::start_topology_maintenance(
        state.current.topology.cache.clone(),
        state.current.topology.dirty.clone(),
        state.current.topology.self_entry.clone(),
        shutdown_token.child_token(),
    );

    // Registry maintenance: reap expired gateway entries and broadcast removals
    crate::tasks::start_registry_maintenance(
        state.clone(),
        shutdown_token.child_token(),
    );

    // Storage lifecycle (STORAGE-0011): auto-mount, health, beacon — all platforms.
    // Replaces legacy seed bank resilience + hot-plug detection.
    crate::tasks::coordinator::start_storage_lifecycle(
        state.clone(),
        shutdown_token.child_token(),
    );
    // Storage console task: renders connected/released ribbons to physical console.
    crate::tasks::coordinator::start_storage_console_task(
        state.platform.runtime.clone(),
        state.subscribe_storage_changed(),
        shutdown_token.child_token(),
    );
    // Phase 17.7: Offering orchestration (ORCH-0001)
    // Manages Primary/Dormant/Joining/Degraded lifecycle for replicated offerings.
    // Must run after registry loader, health monitor, and catalog builder are ready.
    {
        let orch_state = state.clone();
        let orch_token = shutdown_token.child_token();
        tokio::spawn(async move {
            if let Err(e) = crate::tasks::offering_orchestration::offering_orchestration_task(
                orch_state, orch_token,
            )
            .await
            {
                tracing::error!(error = ?e, "Offering orchestration task failed");
            }
        });
        tracing::info!("Offering orchestration task started (ORCH-0001)");
    }

    // Phase 17.8: Seed bank orchestration (STORAGE-0006)
    // Assigns Primary/Dormant roles for replicated seed banks.
    {
        let sb_state = state.clone();
        let sb_token = shutdown_token.child_token();
        tokio::spawn(async move {
            if let Err(e) = crate::tasks::storage_orchestration::storage_orchestration_task(
                sb_state, sb_token,
            )
            .await
            {
                tracing::error!(error = ?e, "Seed bank orchestration task failed");
            }
        });
        tracing::info!("Seed bank orchestration task started (STORAGE-0006)");
    }

    // Phase 17.8b: Storage beacon subscriber (STORAGE-0013)
    // Reacts to StorageChanged domain events by broadcasting beacons.
    // Replaces manual beacon spawns from individual handlers.
    {
        let beacon_state = state.clone();
        let beacon_token = shutdown_token.child_token();
        tokio::spawn(async move {
            crate::tasks::storage_orchestration::storage_beacon_subscriber(
                beacon_state, beacon_token,
            )
            .await;
        });
        tracing::info!("Storage beacon subscriber started (STORAGE-0013)");
    }

    // Phase 17.9: Seed bank storage tick aggregator (STORAGE-0006 Phase 4f)
    // Quantizes raw per-write ticks into per-seed-bank aggregated ticks
    // (2s quiet threshold / 10s deadline cap).
    {
        let raw_rx = state.orchestration.storage.tick.raw.subscribe();
        let agg_tx = state.orchestration.storage.tick.debounced.clone();
        let agg_token = shutdown_token.child_token();
        tokio::spawn(async move {
            crate::tasks::storage_tick_aggregator::storage_tick_aggregator_task(
                raw_rx, agg_tx, agg_token,
            )
            .await;
        });
        tracing::info!("Storage tick aggregator task started (STORAGE-0006)");
    }

    // Phase 17.9b: Seed bank replication (STORAGE-0006 Phase 4e)
    // Syncs Dormant seed banks from their Primaries.
    {
        let repl_state = state.clone();
        let repl_token = shutdown_token.child_token();
        tokio::spawn(async move {
            if let Err(e) = crate::tasks::storage_replication::storage_replication_task(
                repl_state, repl_token,
            )
            .await
            {
                tracing::error!(error = ?e, "Seed bank replication task failed");
            }
        });
        tracing::info!("Seed bank replication task started (STORAGE-0006)");
    }

    // Phase 17.9b2: Shell integration (Windows only)
    // Registers "Zen Garden" context menu on drives for storage adoption.
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = crate::infra::shell_integration::register() {
            tracing::warn!(error = %e, "Shell integration failed to register (non-fatal)");
        } else {
            tracing::info!("Shell integration: drive context menu registered");
        }
    }

    // Phase 17.9c: Cloud Filter sync provider (STORAGE-0009 Phase 4, Windows only)
    // Registers a "Zen Garden" sync root in Explorer so storages appear natively.
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = crate::infra::cloud_filter::start(
            state.current.storage.volumes.clone(),
            state.tool.registry.clone(),
            state.current.stone.id.clone(),
            state.orchestration.storage.tick.raw.clone(),
            state.subscribe_storage_changed(),
            state.tool.delta_stream(),
            state.console.clone(),
            shutdown_token.child_token(),
        )
        .await
        {
            tracing::warn!(error = %e, "Cloud Filter provider failed to start (non-fatal)");
        } else {
            tracing::info!("Cloud Filter sync provider started (STORAGE-0009 Phase 4)");
        }
    }

    // Phase 17.9d: Filesystem watcher (STORAGE-0009 Phase 5, event-driven per STORAGE-0013)
    // Detects external writes to managed storage mounts and records changelog
    // entries so replication stays coherent.
    {
        let watcher_set = crate::infra::storage::StorageWatcherSet::new(
            state.current.storage.volumes.clone(),
            state.orchestration.storage.tick.raw.clone(),
            shutdown_token.child_token(),
        );
        // Initial reconciliation — start watchers for already-mounted storages
        watcher_set.reconcile().await;

        // Event-driven + heartbeat reconciliation — react to StorageChanged
        // events immediately, with a 60s fallback heartbeat.
        let watcher_token = shutdown_token.child_token();
        let mut storage_rx = state.subscribe_storage_changed();
        tokio::spawn(async move {
            let heartbeat = tokio::time::Duration::from_secs(60);
            loop {
                tokio::select! {
                    _ = watcher_token.cancelled() => break,
                    result = storage_rx.recv() => {
                        match result {
                            Ok(event) => {
                                tracing::debug!(event = ?event, "fs watcher: storage event");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::debug!(skipped = n, "fs watcher: lagged, reconciling");
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                        watcher_set.reconcile().await;
                    }
                    _ = tokio::time::sleep(heartbeat) => {
                        watcher_set.reconcile().await;
                    }
                }
            }
        });
        tracing::info!("Filesystem watcher started (event-driven, STORAGE-0013)");
    }

    // Initialize tools projection from restored offerings + local seed-banks.
    // This emits initial tool.upsert deltas and announces them garden-wide.
    state.refresh_local_tools_projection().await;

    api_endpoint
}
