//! Main daemon orchestration
//!
//! Coordinates all startup phases and background tasks.
//! Extracted from main.rs for cleaner separation of concerns.

use crate::{
    AppState, MossEvent, Job, JobStatus,
    // Task coordination
    start_lantern_registration,
    start_discovery_listener, start_hardware_detection,
    start_registry_loader, start_catalog_builder,
    start_health_monitor, start_auto_adoption,
    install_batch_task,
    // Network monitoring
    NetworkMonitor, NetworkMonitorConfig, NetworkEvent,
    // Bootstrap functions
    load_preinstall_manifest,
    run_first_boot_initialization,
    router,
    bind_server, run_server, ServerConfig,
    connect_docker, init_capabilities, DockerConfig,
    version_string,
    // mDNS
    mdns,
    // Infrastructure
    infra,
};
use garden_common::console;
use super::config::DaemonConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Run the Moss daemon with the given configuration
///
/// This is the main entry point after CLI parsing and config loading.
/// Handles all startup phases and background task coordination.
pub async fn run(config: DaemonConfig) -> anyhow::Result<()> {
    let stone_name = config.stone_name.clone();
    let port = config.port;

    // Phase 0: Load or generate stone_id (persistent GUID v7)
    // This must happen early as many components need it
    let stone_id = infra::load_or_generate_stone_id().await;
    tracing::info!(stone_id = %stone_id, stone_name = %stone_name, "Stone identity loaded");

    // Phase 0.5: Initialize self topology entry
    // Create with minimal identity, will be progressively enriched during boot
    let self_entry = Arc::new(RwLock::new(crate::domain::TopologyEntry {
        stone_id: stone_id.clone(),
        stone_name: stone_name.clone(),
        endpoint: String::new(), // Will be set in Phase 3
        moss_version: version_string(),
        services: Vec::new(),
        mac: None, // Will be set in Phase 2
        health: garden_common::constants::STONE_STARTING.to_string(),
        capabilities: None, // Will be set in Phase 9
        status: garden_common::StoneStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    }));
    tracing::debug!("Self topology entry initialized (health=starting)");

    // Phase 1: Start UDP listener EARLY (can now respond to discovery requests)
    // Listener needs minimal dependencies: stone_id, stone_name, topology_cache
    // Self-entry will be progressively updated as boot continues
    let topology_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));
    
    // Console is needed for UDP listener, create it early
    let console_printer = Arc::new(console::ConsolePrinter::with_dedup_ttl(
        config.console_mode,
        config.event_dedup_ttl_secs,
    ));
    
    // Start UDP listener - can now respond to announcement requests with current self_entry state
    start_discovery_listener(
        stone_id.clone(),
        stone_name.clone(),
        String::new(), // Endpoint not yet known, will be set in Phase 3.5
        topology_cache.clone(),
        self_entry.clone(),
        console_printer.clone(),
    )
    .await;
    tracing::info!("UDP listener started at Phase 1 (can respond to discovery requests)");

    // Phase 1.5: First-boot initialization (Linux only)
    // Windows/dev environments don't need hostname/hosts/avahi setup
    if cfg!(target_os = "linux") && console::is_first_run() {
        start_first_boot_task(&stone_name, port, config.docker_retry_delay_secs());
    }

    // Phase 2: Network monitoring
    // Runs in background, polls every 5s when disconnected, 30s when connected
    let network_monitor = NetworkMonitor::start_with_config(
        NetworkMonitorConfig::default()
            .with_disconnect_retry(5)
            .with_connected_poll(30)
    ).await;

    // Phase 2.5: Get MAC address for self entry
    let (_, mac_address) = garden_common::infra::network::get_local_ip_and_mac();

    // Phase 3: Resolve API endpoint
    // Prefer explicit STONE_HOST, otherwise use monitored network IP
    let use_static_host = std::env::var(garden_common::ENV_STONE_HOST)
        .ok()
        .filter(|h| !h.trim().is_empty());

    let api_endpoint = if let Some(host) = &use_static_host {
        format!("http://{}:{}", host.trim(), port)
    } else {
        format!("http://{}:{}", network_monitor.get_ip().await, port)
    };

    // Phase 3.5: Update self entry with network configuration
    {
        let mut entry = self_entry.write().await;
        entry.endpoint = api_endpoint.clone();
        entry.mac = mac_address.clone();
        entry.health = garden_common::constants::STONE_INITIALIZING.to_string();
        entry.last_seen = chrono::Utc::now();
    }
    tracing::debug!(endpoint = %api_endpoint, mac = ?mac_address, "Self entry updated (health=initializing)");
    
    // Auto-chirp: Network configuration complete
    {
        let entry = self_entry.read().await.clone();
        if let Err(e) = crate::announcement::announce(&entry).await {
            tracing::warn!(error = ?e, "Failed to auto-chirp after network config");
        } else {
            tracing::debug!("Auto-chirp sent after network configuration");
        }
    }

    // Phase 4: mDNS announcement (Linux only) - includes stone_id and MAC in TXT records
    // Must happen before IP change handler so we can pass the handle
    // Note: If current IP is loopback, registration is deferred until valid IP is available
    let current_ip = network_monitor.get_ip().await;
    let (_, mac_for_mdns) = garden_common::infra::network::get_local_ip_and_mac();
    let mdns_handle: Option<Arc<mdns::MdnsHandle>> = match mdns::announce_moss(
        Some(stone_id.as_str()),
        &stone_name,
        port,
        mac_for_mdns.as_deref(),
        &current_ip,  // Gate: won't register if loopback
    ) {
        Ok(handle) => Some(Arc::new(handle)),
        Err(e) => {
            tracing::warn!(error = ?e, "mDNS announcement failed");
            None
        }
    };

    // Phase 4.5: Start mDNS lurk-listener (moved to Phase 11 after state creation)
    // Note: IP change handler moved to Phase 11 to use AppState.announce_resolution_change()

    // Phase 6: Lantern registration (console already created in Phase 1)
    start_lantern_registration(
        &stone_id,
        &stone_name,
        &api_endpoint,
        port,
        use_static_host.is_some(),
        &network_monitor,
        Some(&console_printer),
    ).await;

    emit_startup_events(&console_printer, &config);

    // Phase 7: Docker connection
    let docker = connect_docker(&console_printer, DockerConfig::default()).await?;

    // Phase 8: Create channels
    let (event_tx, _) = tokio::sync::broadcast::channel::<MossEvent>(100);
    let shutdown_tx = Arc::new(tokio::sync::Notify::new());

    // Phase 9: Capabilities loading
    let capabilities_arc = init_capabilities(&stone_id, &stone_name, &console_printer).await;

    // Phase 9.5: Update self entry with capabilities and set health to thriving
    {
        let mut entry = self_entry.write().await;
        entry.capabilities = capabilities_arc.read().await.clone();
        entry.health = garden_common::constants::STONE_THRIVING.to_string();
        entry.last_seen = chrono::Utc::now();
    }
    tracing::debug!("Self entry updated with capabilities (health=thriving)");
    
    // Auto-chirp: Capabilities complete
    {
        let entry = self_entry.read().await.clone();
        if let Err(e) = crate::announcement::announce(&entry).await {
            tracing::warn!(error = ?e, "Failed to auto-chirp after capabilities update");
        } else {
            tracing::debug!("Auto-chirp sent after capabilities detection");
        }
    }

    // Phase 10: Load ManifestRegistry (single source of truth for all manifests)
    let sw_dir = std::path::PathBuf::from(infra::runtime_manifests_dir()).join("sw");
    let hw_dir = std::path::PathBuf::from(infra::runtime_manifests_dir()).join("hw");
    let hw_dir_opt = if hw_dir.exists() { Some(hw_dir.as_path()) } else { None };

    let manifest_registry = match infra::ManifestRegistry::load(&sw_dir, hw_dir_opt) {
        Ok(registry) => {
            console_printer.emit(console::ConsoleEvent::new(
                console::EventCategory::Manifests,
                console::EventStatus::Loaded,
                format!("{} sw, {} hw manifests", registry.sw.entries.len(), registry.hw.entries.len()),
            ));
            Arc::new(registry)
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load manifest registry, using empty");
            console_printer.emit(console::ConsoleEvent::new(
                console::EventCategory::Manifests,
                console::EventStatus::Invalid,
                "Using empty registry".to_string(),
            ));
            Arc::new(infra::ManifestRegistry::empty())
        }
    };

    // Phase 11: Build AppState
    let ceremony_registry = Arc::new(crate::domain::CeremonyRegistry::new());
    let ceremony_journal = Arc::new(infra::CeremonyJournal::default_journal());
    let harvest_store = Arc::new(infra::HarvestStore::default_store());

    // Phase 11.pre: Create election service (placeholder for now, will be updated after AppState)
    // Note: No longer async - no socket binding (uses p2p transport)
    let election_service_placeholder = Arc::new(
        crate::tasks::election_service::ElectionService::new(
            stone_id.clone(),
            stone_name.clone(),
            Box::new(crate::tasks::state_provider::PlaceholderStateProvider),
        )
    );

    let state = AppState {
        stone_id: stone_id.clone(),
        stone_name: stone_name.clone(),
        registry: Arc::new(RwLock::new(Vec::new())),
        adopted_offerings: Arc::new(RwLock::new(Vec::new())),
        borrowed_offerings: Arc::new(RwLock::new(Vec::new())),
        manifest_registry: manifest_registry.clone(),
        docker: docker.clone(),
        jobs: Arc::new(RwLock::new(HashMap::new())),
        event_tx,
        shutdown_tx: shutdown_tx.clone(),
        start_time: std::time::Instant::now(),
        offerings_index: Arc::new(RwLock::new(None)),
        console: console_printer.clone(),
        capabilities: capabilities_arc.clone(),
        network_monitor: Arc::new(network_monitor),
        api_port: port,
        topology_cache: topology_cache.clone(),
        self_entry: self_entry.clone(),
        mdns_handle: mdns_handle.clone(),
        ceremony_registry,
        ceremony_journal,
        harvest_store,
        nourishment_jobs: Arc::new(RwLock::new(HashMap::new())),
        election_service: election_service_placeholder,
    };

    // Phase 11.post: Update election service with proper state provider now that AppState exists
    // Note: No longer async - no socket binding (uses p2p transport)
    let state_for_election = Arc::new(state.clone());
    let election_service_final = Arc::new(
        crate::tasks::election_service::ElectionService::new(
            stone_id.clone(),
            stone_name.clone(),
            Box::new(crate::tasks::state_provider::MossStateProvider::new(state_for_election)),
        )
    );
    
    // Update the state's election_service
    let state = AppState {
        election_service: election_service_final.clone(),
        ..state
    };
    
    tracing::info!("Election service initialized (using p2p transport)");

    // Phase 11.post2: Start election service listener (subscribes to p2p events)
    tokio::spawn(async move {
        if let Err(e) = election_service_final.run_listener().await {
            tracing::error!(error = ?e, "Election service listener failed");
        }
    });

    // Phase 11.post3: Start discovery handler (responds to discovery requests)
    let self_entry_for_discovery = state.self_entry.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::tasks::discovery_handler::start_discovery_handler(self_entry_for_discovery).await {
            tracing::error!(error = ?e, "Discovery handler failed");
        }
    });
    tracing::info!("Discovery handler initialized (using p2p transport)");

    // Phase 11.0.5: Ceremony recovery (detect incomplete ceremonies from previous run)
    match state.recover_ceremonies().await {
        Ok(0) => tracing::debug!("No incomplete ceremonies to recover"),
        Ok(n) => tracing::warn!(count = n, "Recovered incomplete ceremonies"),
        Err(e) => tracing::error!(error = ?e, "Failed to recover ceremonies"),
    }

    // Phase 12: Start background tasks
    // UDP listener already started in Phase 1
    // ManifestRegistry already loaded in Phase 10
    start_hardware_detection(stone_name.clone(), capabilities_arc.clone(), console_printer.clone(), state.clone());
    start_registry_loader(state.clone());
    start_catalog_builder(state.clone(), console_printer.clone());

    // Presence monitoring (PRESENCE-0001)
    tracing::info!("Starting presence load monitor");
    let load_monitor_state = state.clone();
    tokio::spawn(async move {
        crate::tasks::presence_monitor::run_load_monitor_task(load_monitor_state).await;
    });

    tracing::info!("Starting presence health monitor");
    let health_monitor_state = state.clone();
    tokio::spawn(async move {
        crate::tasks::presence_monitor::run_health_monitor_task(health_monitor_state).await;
    });

    // Phase 11.1: IP change handler (resolution announcements)
    // Uses AppState.announce_resolution_change() for proper SoC
    if use_static_host.is_none() {
        let state_for_ip = state.clone();
        let mut network_rx = state.network_monitor.subscribe();

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

    // Phase 11.3: Sync self_entry services after registry loads
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
    let topology_cache_for_mdns = state.topology_cache.clone();
    if let Ok(mut mdns_rx) = mdns::start_mdns_lurk_listener(stone_name.clone()) {
        tokio::spawn(async move {
            loop {
                match mdns_rx.recv().await {
                    Ok(discovered) => {
                        tracing::debug!(
                            stone_id = ?discovered.stone_id,
                            stone_name = %discovered.stone_name,
                            endpoint = %discovered.endpoint,
                            mac = ?discovered.mac,
                            "mDNS: Neighbor stone discovered and cached"
                        );
                        // Add to topology cache (only if stone_id is present)
                        if let Some(sid) = discovered.stone_id {
                            let entry = crate::domain::TopologyEntry {
                                stone_id: sid,
                                stone_name: discovered.stone_name,
                                endpoint: discovered.endpoint,
                                moss_version: "unknown".to_string(), // mDNS doesn't provide version
                                services: vec![], // mDNS doesn't provide services
                                mac: discovered.mac,
                                health: garden_common::constants::STONE_INITIALIZING.to_string(), // mDNS = early discovery
                                capabilities: None, // mDNS doesn't provide capabilities
                                status: garden_common::StoneStatus::Online,
                                discovered_at: chrono::Utc::now(),
                                last_seen: chrono::Utc::now(),
                            };
                            crate::domain::topology::upsert_from_chirp(
                                &topology_cache_for_mdns,
                                entry,
                            ).await;
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
    if let Ok(mut discovery_rx) = garden_common::infra::communications::p2p::subscribe_to_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE
    ).await {
        // Send discovery request
        let request = garden_common::DiscoveryRequest {
            discover: "moss".to_string(),
            request_id: garden_common::ids::generate_guidv7(),
            requester: stone_id.clone(),
        };
        
        if let Err(e) = garden_common::infra::communications::p2p::send_announcement(
            garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
            &request
        ).await {
            tracing::warn!(error = ?e, "Failed to send discovery request");
        } else {
            // Collect responses for 3 seconds
            let timeout_fut = tokio::time::timeout(
                std::time::Duration::from_secs(3),
                async {
                    let mut responses = Vec::new();
                    while let Some((payload, _from_addr)) = discovery_rx.recv().await {
                        if let Ok(response) = serde_json::from_value::<garden_common::DiscoveryResponse>(payload) {
                            responses.push(response);
                        }
                    }
                    responses
                }
            );
            
            let discovered_peers = timeout_fut.await.unwrap_or_else(|_| Vec::new());
            
            for peer in discovered_peers {
                if let Some(peer_id) = peer.stone_id {
                    let entry = crate::domain::TopologyEntry {
                        stone_id: peer_id,
                        stone_name: peer.stone_name,
                endpoint: peer.stone_endpoint,
                moss_version: peer.moss_version,
                services: vec![], // Discovery response doesn't include services yet
                mac: None, // Will be populated by later chirps
                health: garden_common::constants::STONE_INITIALIZING.to_string(),
                capabilities: None,
                status: garden_common::StoneStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            };
            crate::domain::topology::upsert_from_chirp(
                &state.topology_cache,
                entry,
            ).await;
        }
    }
        }
    } else {
        tracing::warn!("Failed to subscribe to discovery responses");
    }

    // Phase 13: Initial announcement (announce ourselves)
    tracing::info!("Sending initial announcement...");
    let entry = state.self_entry.read().await.clone();
    if let Err(e) = crate::announcement::announce(&entry).await {
        tracing::warn!(error = ?e, "Initial announcement failed");
    }

    // Phase 14: Start periodic announcer (30s background task)
    crate::tasks::start_periodic_announcer(state.clone());

    // Phase 15: Subscribe to service change events for immediate announcements
    let state_for_events = state.clone();
    let mut event_rx = state.event_tx.subscribe();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) if event.message.contains("service") || event.message.contains("offering") => {
                    tracing::debug!(message = %event.message, "Service-related event detected, announcing");
                    
                    // Sync services and chirp
                    state_for_events.sync_self_services(true).await;
                }
                Ok(_) => {},
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "Event subscription lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Phase 16: Pre-install manifest handling
    start_preinstall_handler(&state).await;

    // Phase 17: Health monitoring and auto-adoption
    start_health_monitor(state.clone());
    if let Some(cfg) = config.file_config.clone() {
        start_auto_adoption(state.clone(), cfg, &console_printer);
    }

    // Phase 18: HTTP server
    tracing::info!("Setting up HTTP router with 200 MB body limit");
    let app = router::configure(state.clone());
    let listener = bind_server(port, &console_printer).await?;

    // Create shutdown callback to send goodbye announcement
    let goodbye_state = state.clone();
    let shutdown_callback: crate::bootstrap::server::ShutdownCallback = Box::new(move || {
        Box::pin(async move {
            if let Err(e) = crate::announcement::send_goodbye(&goodbye_state).await {
                tracing::warn!(error = ?e, "Failed to send goodbye announcement");
            }
        })
    });

    // Prepare boot banner info
    let current_ip = state.network_monitor.get_ip().await;
    let manifests_count = state.manifest_registry.sw.entries.len();
    let boot_banner = Some(console::BootBannerInfo {
        stone_name: stone_name.clone(),
        version: version_string(),
        ip: current_ip,
        port,
        manifests_count,
    });

    // Prepare shutdown banner info (start_time used for uptime at shutdown)
    let shutdown_banner = Some(console::ShutdownBannerInfo {
        stone_name: stone_name.clone(),
        start_time: state.start_time,
    });

    run_server(
        listener,
        app,
        &api_endpoint,
        console_printer,
        shutdown_tx,
        ServerConfig::default(),
        Some(shutdown_callback),
        boot_banner,
        shutdown_banner,
    ).await
}

/// Start first-boot initialization task (Linux only)
///
/// Waits for filesystem to become writable, then runs initialization.
/// Exits process after completion so systemd restarts with new config.
fn start_first_boot_task(stone_name: &str, port: u16, retry_delay_secs: u64) {
    tracing::info!("First run detected on Linux, spawning background initialization task");
    tracing::info!("First boot detected - will initialize console after Docker connection");

    let init_stone_name = stone_name.to_string();
    let init_port = port;

    tokio::spawn(async move {
        const MAX_ATTEMPTS: u32 = 20;

        let _ = console::tty_write("");
        let _ = console::display_wait("First-boot setup: Waiting for filesystem to become writable");

        for attempt in 1..=MAX_ATTEMPTS {
            match console::ensure_etc_writable().await {
                Ok(true) => {
                    tracing::info!(attempt, "Filesystem is writable, proceeding with first boot initialization");
                    let _ = console::display_success("Filesystem ready, starting configuration");

                    match run_first_boot_initialization(&init_stone_name, init_port).await {
                        Ok(new_name) => {
                            if let Err(e) = console::mark_first_run_complete().await {
                                tracing::error!(error = ?e, "Failed to mark first-run complete");
                            }

                            tracing::info!(new_name = %new_name, "First boot initialization completed successfully");
                            let _ = console::tty_write("");
                            let _ = console::display_success(&format!("✓ Stone configured as: {}", new_name));
                            let _ = console::display_wait("Restarting to apply new configuration...");
                            let _ = console::tty_write("");

                            // Exit so systemd restarts us with the new configuration
                            std::process::exit(0);
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "First boot initialization failed");
                            let _ = console::display_error(&format!("Setup failed: {}", e));
                            if attempt < MAX_ATTEMPTS {
                                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs)).await;
                            }
                        }
                    }
                }
                Ok(false) | Err(_) => {
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs)).await;
                    } else {
                        tracing::error!("First boot initialization abandoned - filesystem never became writable");
                        let _ = console::display_error("Setup abandoned - filesystem remained read-only");
                    }
                }
            }
        }
    });
}

/// Emit startup console events
fn emit_startup_events(console_printer: &console::ConsolePrinter, config: &DaemonConfig) {
    console_printer.emit(console::ConsoleEvent::new(
        console::EventCategory::System,
        console::EventStatus::Starting,
        format!("Moss v{}", version_string())
    ));

    if config.file_config.is_some() {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::Loaded,
            "Configuration file".to_string()
        ));

        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::Merged,
            "Priority: CLI > Env > Config > Defaults".to_string()
        ));
    } else {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::NotFound,
            "Using defaults".to_string()
        ));
    }
}

/// Handle pre-install manifest on first boot
///
/// Validates offerings, creates installation job, and spawns background task.
async fn start_preinstall_handler(state: &AppState) {
    let manifest = match load_preinstall_manifest().await {
        Some(m) if m.auto_install => m,
        _ => return,
    };

    tracing::info!(
        "Starting auto-installation of {} services from manifest",
        manifest.offerings.len()
    );

    // Validate all offerings exist before creating job
    let invalid_offerings: Vec<_> = manifest.offerings.iter()
        .filter(|o| state.manifest_registry.sw.get(o).is_none())
        .cloned()
        .collect();

    if !invalid_offerings.is_empty() {
        tracing::error!(
            offerings = ?invalid_offerings,
            "Pre-install manifest contains invalid offerings - skipping auto-install"
        );
        return;
    }

    let job_id = uuid::Uuid::now_v7().to_string();
    let job = Job {
        id: job_id.clone(),
        offerings: manifest.offerings.clone(),
        status: JobStatus::Pending,
        completed: vec![],
        failed: HashMap::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
    };

    state.jobs.write().await.insert(job_id.clone(), job);

    // Spawn background installation + cleanup task
    let install_state = state.clone();
    let install_job_id = job_id.clone();
    let install_offerings = manifest.offerings.clone();

    tokio::spawn(async move {
        install_batch_task(&install_state, &install_job_id, install_offerings).await;

        // Wait for job completion, then remove manifest
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            let jobs = install_state.jobs.read().await;
            if let Some(job) = jobs.get(&install_job_id) {
                match job.status {
                    JobStatus::Completed | JobStatus::Failed => {
                        drop(jobs); // Release lock
                        tracing::info!("Pre-install job finished, removing manifest");
                        if let Err(e) = tokio::fs::remove_file("/home/stone/garden-moss-preinstall.json").await {
                            tracing::warn!(error = ?e, "Failed to remove pre-install manifest");
                        } else {
                            tracing::info!("Pre-install manifest removed - system ready");
                        }
                        break;
                    }
                    _ => continue,
                }
            } else {
                break;
            }
        }
    });

    tracing::info!("Pre-install job started: {} (check /api/jobs/{})", job_id, job_id);
}
