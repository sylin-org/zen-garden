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
use crate::tasks::network_monitor::{NetworkEvent, NetworkMonitor};
use crate::tasks::task_scheduler::{backfill_missing_tasks, start_task_scheduler};
use crate::{
    adopt_existing_containers, auto_adoption_task, detect_capabilities_background,
    ensure_offerings_index, health_monitor_task, infra, lantern_registration_loop, AppState,
};
use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use garden_common::infra::communications::p2p;
use garden_common::storage::SeedBankInfo;
use garden_common::{HardwareCapabilities, ServiceHealthStatus};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Start topology maintenance task (TOPO-0002: with persistence)
///
/// Periodically marks stale stones as offline, evicts old offline stones,
/// and flushes dirty topology cache to disk.
/// Runs every 30 seconds (aligns with stone chirp interval).
pub fn start_topology_maintenance(
    topology_cache: TopologyCache,
    topology_dirty: TopologyDirtyFlag,
    self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        interval.tick().await; // Skip first immediate tick

        loop {
            interval.tick().await;
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

/// Start storage cache maintenance task (STORAGE-0003)
///
/// Periodically prunes stale entries from storage cache.
/// Runs every 60 seconds (2x topology interval).
pub fn start_storage_maintenance(
    storage_cache: crate::domain::storage_cache::StorageCache,
    topology_cache: TopologyCache,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        interval.tick().await; // Skip first immediate tick

        loop {
            interval.tick().await;
            let pruned =
                crate::domain::storage_cache::prune_stale(&storage_cache, &topology_cache).await;
            if pruned > 0 {
                tracing::debug!(
                    pruned = pruned,
                    "Storage cache maintenance: pruned stale entries"
                );
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
    storage_cache: crate::domain::storage_cache::StorageCache,
    tools_cache: crate::domain::tools::ToolsCache,
    tools_tx: tokio::sync::broadcast::Sender<garden_common::tools::ToolDelta>,
    self_entry: Arc<RwLock<crate::domain::TopologyEntry>>,
    console: Arc<ConsolePrinter>,
    infrastructure_handlers: Arc<crate::domain::InfrastructureHandlerRegistry>,
    manifest_registry: Arc<crate::infra::ManifestRegistry>,
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
                        let local_tools_cache = tools_cache.clone();
                        tokio::spawn(async move {
                            let resolved_endpoint = {
                                let current = local_entry.read().await.address.http_base();
                                if current.contains("0.0.0.0") {
                                    local_endpoint
                                } else {
                                    current
                                }
                            };

                            match crate::infra::storage::broadcast_if_has_storage(
                                &local_stone_id,
                                &local_stone_name,
                                &resolved_endpoint,
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

                            // TOOLS-0001: Broadcast current local tools snapshot for new stone.
                            let snapshot_deltas = {
                                let cache = local_tools_cache.read().await;
                                cache.local_snapshot_for_beacon(&local_stone_id)
                            };
                            if !snapshot_deltas.is_empty() {
                                if let Err(e) = crate::infra::broadcast_tools_beacon(
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
                                        "Failed to broadcast tools beacon for new stone"
                                    );
                                }
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

                    // STORAGE-0003: Remove from storage cache
                    crate::domain::storage_cache::remove_stone(&storage_cache, &goodbye.stone_id)
                        .await;

                    // TOOLS-0001: Remove tool projections for offline stone.
                    let removed = {
                        let mut cache = tools_cache.write().await;
                        cache.remove_stone_tools(&goodbye.stone_id)
                    };
                    for delta in removed {
                        let _ = tools_tx.send(delta);
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
                        seed_banks = beacon.seed_banks.len(),
                        from = %from_addr,
                        "Storage beacon received, updating storage cache"
                    );

                    // Update storage cache
                    crate::domain::storage_cache::update_from_beacon(&storage_cache, beacon).await;
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

                    let applied = {
                        let mut cache = tools_cache.write().await;
                        cache.apply_remote_beacon(&beacon)
                    };
                    for delta in applied {
                        let _ = tools_tx.send(delta);
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
        {
            let mut offerings = state.offerings.write().await;
            for offering in offerings.iter_mut().filter(|o| o.is_managed()) {
                if !state
                    .docker
                    .zen_container_exists(&offering.name)
                    .await
                    .unwrap_or(false)
                {
                    offering.status = garden_common::OfferingStatus::Stopped;
                    offering.health = ServiceHealthStatus::Offline;
                }
            }
        }
        let _ = state.persist_offerings().await;

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
pub fn start_health_monitor(state: AppState) {
    tokio::spawn(async move {
        health_monitor_task(state).await;
    });
}

/// Start caretaking maintenance sweep (hourly background task)
///
/// Runs all domain sweepers sequentially every hour (5 min delay after boot).
/// Persists results to disk for API consumption.
pub fn start_maintenance_sweep(state: AppState) {
    tokio::spawn(async move {
        // Wait 5 minutes after boot before first sweep
        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;

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
pub fn start_auto_adoption(state: AppState, config: infra::MossConfig, console: &ConsolePrinter) {
    let adoption_config = config.adoption();
    start_auto_adoption_with_config(state, adoption_config, console);
}

/// Start auto-adoption task with explicit AdoptionConfig
///
/// Use this variant when no MossConfig file is available - it will use
/// deployment profile detection to determine if adoption should be enabled.
pub fn start_auto_adoption_with_config(
    state: AppState,
    adoption_config: infra::AdoptionConfig,
    console: &ConsolePrinter,
) {
    if adoption_config.is_enabled() {
        tracing::info!("Auto-adoption enabled, starting adoption background task");
        console.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption enabled",
        ));

        tokio::spawn(async move {
            auto_adoption_task(state, adoption_config).await;
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
pub async fn start_lantern_registration(
    stone_id: &str,
    stone_name: &str,
    api_endpoint: &str,
    port: u16,
    use_static_host: bool,
    network_monitor: &NetworkMonitor,
    console: Option<&ConsolePrinter>,
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
        if let Err(e) =
            lantern_registration_loop(reg_stone_id, reg_stone_name, reg_endpoint, lantern_url).await
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
        let mut network_rx = network_monitor.subscribe();

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
///    - Verifies all tracked mounts are still active
///    - Automatically re-mounts devices that have unexpectedly become unmounted
///    - Handles race conditions with udisks2 or other system processes
///    - Continues retrying indefinitely (devices can come back)
///
/// 2. **Hot-plug Detection Task** (every 10 seconds):
///    - Scans for new zen-seed devices that may have been plugged in
///    - Auto-mounts unmounted devices
///    - Updates storage cache and broadcasts beacon
///
/// Both tasks share a MountTracker to maintain state about expected mounts.
#[cfg(target_os = "linux")]
pub fn start_seedbank_resilient_mount_system(state: AppState) {
    use crate::infra::storage::{create_mount_tracker, SeedBankRegistry};

    // Create shared mount tracker
    let tracker = create_mount_tracker();
    let tracker_persistence = tracker.clone();
    let tracker_hotplug = tracker.clone();

    let state_persistence = state.clone();
    let state_hotplug = state;

    // Task 1: Mount persistence verification (every 5 seconds)
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        interval.tick().await; // Skip first immediate tick

        tracing::info!("Seed bank mount persistence task started (5s interval)");

        loop {
            interval.tick().await;

            // Verify and recover any mounts that disappeared
            let recovered = SeedBankRegistry::verify_and_recover_mounts(&tracker_persistence).await;

            if recovered > 0 {
                tracing::info!(
                    recovered = recovered,
                    "Mount persistence: recovered disappeared mounts"
                );

                // Update storage cache since mounts changed
                let endpoint = state_persistence.self_entry.read().await.address.http_base();
                if let Err(e) = crate::infra::storage::update_and_broadcast(
                    &state_persistence.storage_cache,
                    &state_persistence.stone_id,
                    &state_persistence.stone_name,
                    &endpoint,
                )
                .await
                {
                    tracing::debug!(
                        error = %e,
                        "Failed to update storage cache after mount recovery"
                    );
                } else {
                    state_persistence.refresh_local_tools_projection().await;
                }

                // Refresh seed bank cache for portrait/UI
                match SeedBankRegistry::scan().await {
                    Ok(registry) => {
                        let banks: Vec<SeedBankInfo> =
                            registry.list().into_iter().cloned().collect();
                        let mut cache = state_persistence.seed_bank_cache.write().await;
                        *cache = banks;
                    }
                    Err(e) => {
                        tracing::debug!(
                            error = %e,
                            "Failed to refresh seed bank cache after mount recovery"
                        );
                    }
                }
            }
        }
    });

    // Task 2: Hot-plug detection (every 10 seconds)
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        interval.tick().await; // Skip first immediate tick

        tracing::info!("Seed bank hot-plug detection task started (10s interval)");

        loop {
            interval.tick().await;

            // Scan triggers auto-mount for any new zen-seed devices
            // Use the tracker so new mounts are monitored for persistence
            // Pass event_bus to emit storage.detected events for Companions
            match SeedBankRegistry::auto_mount_seed_banks_with_tracker(
                Some(&tracker_hotplug),
                Some(&state_hotplug.event_bus),
            )
            .await
            {
                Ok(()) => {}
                Err(e) => {
                    tracing::trace!(
                        error = %e,
                        "Hot-plug auto-mount scan failed"
                    );
                }
            }

            // Scan to get registry state (also triggers auto-mount internally, but we need the result)
            match SeedBankRegistry::scan().await {
                Ok(registry) => {
                    let count = registry.list().len();
                    if count > 0 {
                        tracing::trace!(seed_banks = count, "Hot-plug scan: seed banks detected");
                    }

                    // Refresh seed bank cache for portrait/UI
                    {
                        let banks: Vec<SeedBankInfo> =
                            registry.list().into_iter().cloned().collect();
                        let mut cache = state_hotplug.seed_bank_cache.write().await;
                        *cache = banks;
                    }

                    // Update storage cache and broadcast if we have storage
                    let endpoint = state_hotplug.self_entry.read().await.address.http_base();
                    if let Err(e) = crate::infra::storage::update_and_broadcast(
                        &state_hotplug.storage_cache,
                        &state_hotplug.stone_id,
                        &state_hotplug.stone_name,
                        &endpoint,
                    )
                    .await
                    {
                        tracing::trace!(
                            error = %e,
                            "Failed to update storage cache during hot-plug scan"
                        );
                    } else {
                        state_hotplug.refresh_local_tools_projection().await;
                    }
                }
                Err(e) => {
                    tracing::trace!(
                        error = %e,
                        "Hot-plug scan failed"
                    );
                }
            }
        }
    });
}

/// Start seed bank hot-plug detection task (non-Linux fallback)
///
/// On non-Linux platforms, just runs the basic scan without mount tracking.
#[cfg(not(target_os = "linux"))]
pub fn start_seedbank_resilient_mount_system(state: AppState) {
    start_seedbank_hotplug_detection_basic(state);
}

/// Basic hot-plug detection without mount tracking (used on non-Linux)
#[cfg(not(target_os = "linux"))]
fn start_seedbank_hotplug_detection_basic(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        interval.tick().await; // Skip first immediate tick

        loop {
            interval.tick().await;

            // Scan triggers auto-mount for any new zen-seed devices
            match crate::infra::storage::SeedBankRegistry::scan().await {
                Ok(registry) => {
                    let count = registry.list().len();
                    if count > 0 {
                        tracing::trace!(seed_banks = count, "Hot-plug scan: seed banks detected");
                    }

                    {
                        let banks: Vec<SeedBankInfo> =
                            registry.list().into_iter().cloned().collect();
                        let mut cache = state.seed_bank_cache.write().await;
                        *cache = banks;
                    }

                    // Update storage cache and broadcast if we have storage
                    let endpoint = state.self_entry.read().await.address.http_base();
                    if let Err(e) = crate::infra::storage::update_and_broadcast(
                        &state.storage_cache,
                        &state.stone_id,
                        &state.stone_name,
                        &endpoint,
                    )
                    .await
                    {
                        tracing::trace!(
                            error = %e,
                            "Failed to update storage cache during hot-plug scan"
                        );
                    } else {
                        state.refresh_local_tools_projection().await;
                    }
                }
                Err(e) => {
                    tracing::trace!(
                        error = %e,
                        "Hot-plug scan failed"
                    );
                }
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
) {
    let console = state.console.clone();

    // Start topology maintenance (mark stale offline, evict old, persist if dirty)
    start_topology_maintenance(
        state.topology_cache.clone(),
        state.topology_dirty.clone(),
        state.self_entry.clone(),
    );

    // Start storage cache maintenance (STORAGE-0003: prune stale entries)
    start_storage_maintenance(state.storage_cache.clone(), state.topology_cache.clone());

    // Start resilient seed bank mount system (STORAGE-0004: mount persistence + hot-plug)
    start_seedbank_resilient_mount_system(state.clone());

    // Start UDP discovery (immediate - critical for stone visibility)
    start_discovery_listener(
        state.stone_id.clone(),
        stone_name.to_string(),
        api_endpoint.to_string(),
        state.topology_cache.clone(),
        state.topology_dirty.clone(),
        state.storage_cache.clone(),
        state.tools_cache.clone(),
        state.tools_tx.clone(),
        state.self_entry.clone(),
        console.clone(),
        state.infrastructure_handlers.clone(),
        state.manifest_registry.clone(),
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
    start_health_monitor(state.clone());

    // Start scheduled task scheduler
    start_task_scheduler(state.clone());
    tracing::info!("Started scheduled task scheduler");

    // Start caretaking sweep (hourly maintenance)
    start_maintenance_sweep(state.clone());

    // Start auto-adoption if configured
    if let Some(cfg) = config {
        start_auto_adoption(state.clone(), cfg, &console);
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
