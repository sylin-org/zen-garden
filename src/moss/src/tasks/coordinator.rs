//! Background task coordination (ARCH-0015).
//!
//! Sequential boot steps + task registry + supervisor.
//!
//! The `start_background_tasks` function performs all sequential boot steps
//! (volume scans, event wiring, ceremony recovery, peer discovery, etc.)
//! then hands off to the task registry and supervisor for all long-running
//! background tasks.

use crate::{Moss, infra, mdns};
use garden_common::console::{ConsoleEvent, EventCategory, EventStatus};
use std::path::PathBuf;
use std::sync::Arc;

use crate::infra::storage::{ContentStore, OsPlatform};

// Re-export `start_lantern_registration` — called from the sequential boot section.
pub use super::lantern::start_lantern_registration;

// ============================================================================
// Task supervisor
// ============================================================================

/// Spawn all background tasks under a structured supervisor.
///
/// Called once after `build_state` completes. Takes ownership of
/// `BuildArtifacts` (consuming `volume_rescan_rx`) and returns the API
/// endpoint string for `serve` plus a `TaskSupervisor` that the caller
/// must `.run()` to monitor tasks and handle panics.
pub(crate) async fn start_background_tasks(
    state: Moss,
    artifacts: crate::bootstrap::BuildArtifacts,
    config: Option<infra::MossConfig>,
) -> (String, super::supervisor::TaskSupervisor) {
    use garden_common::console;

    // Rebind Stage-1 locals from Moss so the task-wiring body is verbatim.
    let stone_id = state.current.stone.id.clone();
    let stone_name = state.current.stone.name.clone();
    let shutdown_token = state.shutdown_token.clone();
    let console_printer = state.console.clone();
    let event_bus = state.event_bus.clone();
    let pulse = state.pulse.clone();
    let koi_handle = state.discovery.koi().clone();
    let api_endpoint = artifacts.api_endpoint;
    let volume_rescan_rx = artifacts.volume_rescan_rx;
    // bool: true when ZG_STONE_HOST was set (gates IP-change handler variant)
    let use_static_host = artifacts.use_static_host;

    // ====================================================================
    // Sequential boot steps (order matters — not parallelizable)
    // ====================================================================

    // Phase 11.post4a: Initial volume scan (STORAGE-0011)
    // Populates the unified Volumes map with all currently attached volumes.
    // Cross-platform: uses platform::scan_volumes() (Linux: /proc/mounts, Windows: GetLogicalDrives).
    {
        let volumes = state.current.storage.volumes.clone();
        let platform: Arc<OsPlatform> = Arc::new(OsPlatform);
        let make_store =
            |path: PathBuf| -> Arc<ContentStore> { Arc::new(ContentStore::new(path, None)) };
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
        let state_for_chirp = state.clone();
        let chirp_listener = Arc::new(infra::ChirpListener::new(Arc::new(move || {
            let state = state_for_chirp.clone();
            tokio::spawn(async move {
                let topology_entry =
                    crate::domain::topology::composition::build_self_entry(&state).await;
                if let Err(e) = state.topology.chirp(&topology_entry).await {
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
    match state.security.recover_ceremonies().await {
        Ok(0) => tracing::debug!("No incomplete ceremonies to recover"),
        Ok(n) => tracing::warn!(count = n, "Recovered incomplete ceremonies"),
        Err(e) => tracing::error!(error = ?e, "Failed to recover ceremonies"),
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
                    state.topology.upsert_from_chirp(entry).await;
                }
            }
        }
    } else {
        tracing::warn!("Failed to subscribe to discovery responses");
    }

    // Phase 13: Initial announcement (announce ourselves)
    tracing::info!("Sending initial announcement...");
    let entry = crate::domain::topology::composition::build_self_entry(&state).await;
    if let Err(e) = state.topology.chirp(&entry).await {
        tracing::warn!(error = ?e, "Initial announcement failed");
    }

    // Phase 15: Lantern registration (now has Moss for service list)
    {
        let network = state.platform.network.clone();
        start_lantern_registration(
            &stone_id,
            &stone_name,
            &api_endpoint,
            state.current.api_port,
            use_static_host,
            &network,
            Some(&console_printer),
            state.clone(),
            shutdown_token.child_token(),
        )
        .await;
    }

    // Phase 16: Pre-install manifest handling
    crate::bootstrap::run::start_preinstall_handler(&state).await;

    // ====================================================================
    // Channel / resource creation for Pattern-C tasks
    // ====================================================================

    // Volume monitor channels (STORAGE-0014)
    let (vol_tx, vol_rx) =
        tokio::sync::mpsc::channel::<crate::infra::storage::monitor::PhysicalStorageEvent>(32);
    let monitor_token = shutdown_token.child_token();
    let bank = crate::domain::VolumeIngestor::new(
        state.current.storage.volumes.clone(),
        state.current.storage.changed.clone(),
        |path: PathBuf| -> Arc<ContentStore> { Arc::new(ContentStore::new(path, None)) },
    );
    crate::infra::storage::monitor::build_monitor().start(vol_tx, monitor_token.clone());

    // mDNS lurk listener (passive topology discovery)
    let mdns_lurk_rx = mdns::start_mdns_lurk_listener(koi_handle.clone(), stone_name.clone())
        .await
        .ok();
    if mdns_lurk_rx.is_some() {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Discovery,
            console::EventStatus::MdnsActive,
            "Lurk-listener active (passive topology discovery)".to_string(),
        ));
    }

    // StorageWatcherSet (STORAGE-0009 Phase 5, STORAGE-0013)
    let watcher_set = crate::infra::storage::StorageWatcherSet::new(
        state.current.storage.volumes.clone(),
        state.current.storage.coordination.tick.raw.clone(),
        shutdown_token.child_token(),
    );
    // Initial reconciliation — start watchers for already-mounted storages
    watcher_set.reconcile().await;

    // ====================================================================
    // Windows platform setup (non-task, sequential)
    // ====================================================================

    // Shell integration (Windows only)
    // Registers "Zen Garden" context menu on drives for storage adoption.
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = crate::infra::shell_integration::register() {
            tracing::warn!(error = %e, "Shell integration failed to register (non-fatal)");
        } else {
            tracing::info!("Shell integration: drive context menu registered");
        }
    }

    // Cloud Filter sync provider (STORAGE-0009 Phase 4, Windows only)
    // Registers a "Zen Garden" sync root in Explorer so storages appear natively.
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = crate::infra::cloud_filter::start(
            state.current.storage.volumes.clone(),
            state.tool.registry.clone(),
            state.current.stone.id.clone(),
            state.current.storage.coordination.tick.raw.clone(),
            state.current.storage.changed.subscribe(),
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

    // ====================================================================
    // Build task config, channels, registry, and supervisor
    // ====================================================================

    let adoption_config = config.as_ref().map(|c| c.adoption()).unwrap_or_default();

    if adoption_config.is_enabled() {
        tracing::info!("Auto-adoption enabled, starting adoption background task");
        console_printer.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption enabled",
        ));
    } else {
        tracing::info!("Auto-adoption disabled (deployment profile or configuration)");
        console_printer.emit(ConsoleEvent::new(
            EventCategory::Config,
            EventStatus::Loaded,
            "Auto-adoption disabled",
        ));
    }

    let task_config = super::task_registry::TaskConfig {
        adoption_config,
        use_static_host,
        mdns_available: state.discovery.has_mdns(),
    };

    let channels = super::task_registry::TaskChannels {
        vol_rx,
        rescan_rx: volume_rescan_rx,
        bank,
        volumes: state.current.storage.volumes.clone(),
        pulse: state.pulse.clone(),
        notifications: state.presence.notifications.clone(),
        monitor_token,
        watcher_set,
        mdns_lurk_rx,
        self_stone_name: stone_name.clone(),
    };

    let tasks = super::task_registry::build_task_registry(task_config, channels);

    let supervisor =
        super::supervisor::TaskSupervisor::build(tasks, state.clone(), shutdown_token.clone())
            .expect("Invalid task dependency graph — startup aborted");

    // Note: the initial tools projection is now seeded by the
    // `offerings-projection` background task at startup (before it
    // signals ready). The coordinator no longer needs to call it
    // explicitly here — that was a Book I-era leftover.

    (api_endpoint, supervisor)
}
