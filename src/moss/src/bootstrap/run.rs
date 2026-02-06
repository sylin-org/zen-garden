//! Main daemon orchestration
//!
//! Coordinates all startup phases and background tasks.
//! Extracted from main.rs for cleaner separation of concerns.

use crate::{
    AppState, Job, JobStatus,
    // Task coordination
    start_lantern_registration,
    start_discovery_listener, start_hardware_detection,
    start_registry_loader, start_catalog_builder,
    start_health_monitor, start_auto_adoption, start_auto_adoption_with_config,
    install_batch_task,
    // Network monitoring
    NetworkMonitor, NetworkMonitorConfig, NetworkEvent,
    // Docker monitoring
    DockerMonitor, DockerMonitorConfig,
    // Bootstrap functions
    load_preinstall_manifest,
    router,
    bind_server, run_server, ServerConfig,
    connect_docker, init_capabilities, DockerConfig,
    version_string,
    // mDNS
    mdns,
    // Infrastructure
    infra,
};
#[cfg(target_os = "linux")]
use crate::run_first_boot_initialization;
use garden_common::console;
use garden_common::offerings::parse_offering_fqn;
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
        tags: Vec::new(), // Compiled from NotificationRegistry
    }));
    tracing::debug!("Self topology entry initialized (health=starting)");

    // Phase 1: Start UDP listener EARLY (can now respond to discovery requests)
    // Listener needs minimal dependencies: stone_id, stone_name, topology_cache
    // Self-entry will be progressively updated as boot continues
    let topology_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));
    
    // STORAGE-0003: Create storage cache for seed bank routing
    let storage_cache = crate::domain::storage_cache::new_storage_cache();
    
    // Console is needed for UDP listener, create it early
    let console_printer = Arc::new(console::ConsolePrinter::with_dedup_ttl(
        config.console_mode,
        config.event_dedup_ttl_secs,
    ));

    // Load ManifestRegistry early - needed for infrastructure handlers
    // Uses overlay pattern: embedded assets first, filesystem overlays on top
    let manifests_dir = std::path::PathBuf::from(infra::runtime_manifests_dir());
    let hw_dir = manifests_dir.join("hw");
    let hw_dir_opt = if hw_dir.exists() { Some(hw_dir.as_path()) } else { None };

    let manifest_registry = match infra::load_sw_manifests_with_overlay(&manifests_dir) {
        Ok(sw_manifests) => {
            match infra::ManifestRegistry::from_sw_manifests(sw_manifests, hw_dir_opt) {
                Ok(mut registry) => {
                    // Inject embedded adopted offerings (detection/control rules)
                    // Merge with existing - filesystem takes precedence
                    let embedded_adopted = infra::load_embedded_adopted_offerings();
                    let mut embedded_count = 0;
                    for offering in embedded_adopted {
                        // Only add if not already present from filesystem
                        if !registry.sw.contains(&offering.name) {
                            registry.upsert_offering(offering);
                            embedded_count += 1;
                        } else if let Some(existing) = registry.sw.get_mut(&offering.name) {
                            // Merge adopted config into existing offering
                            if existing.adopted.is_none() && offering.adopted.is_some() {
                                existing.adopted = offering.adopted;
                                embedded_count += 1;
                            }
                        }
                    }

                    tracing::info!(
                        offerings = registry.sw.entries.len(),
                        hw_count = registry.hw.entries.len(),
                        embedded_adopted = embedded_count,
                        "ManifestRegistry loaded"
                    );
                    Arc::new(registry)
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "Failed to create manifest registry, using empty");
                    Arc::new(infra::ManifestRegistry::empty())
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load manifest registry, using empty");
            Arc::new(infra::ManifestRegistry::empty())
        }
    };

    // Load well-known ports catalog (for port conflict remediation)
    let ports_catalog_path = manifests_dir.join("well-known-ports.yaml");
    if ports_catalog_path.exists() {
        // Prefer filesystem version
        if let Err(e) = garden_common::manifests::init_ports_catalog(&ports_catalog_path) {
            tracing::warn!(error = ?e, "Failed to load ports catalog from filesystem");
        } else {
            tracing::debug!("Ports catalog loaded from filesystem");
        }
    } else {
        // Fall back to embedded
        if let Some(content) = infra::EmbeddedManifests::get_file("well-known-ports.yaml") {
            let content_str = String::from_utf8_lossy(&content);
            if let Err(e) = garden_common::manifests::init_ports_catalog_from_str(&content_str) {
                tracing::warn!(error = ?e, "Failed to load embedded ports catalog");
            } else {
                tracing::debug!("Ports catalog loaded from embedded assets");
            }
        } else {
            tracing::warn!("No ports catalog found (filesystem or embedded)");
        }
    }

    // Create infrastructure handlers - wired to UDP pipeline from the start
    let infrastructure_handlers = Arc::new(crate::domain::InfrastructureHandlerRegistry::new(vec![
        Box::new(crate::domain::DockerRegistryHandler::new()),
    ]));

    // Start UDP listener with full infrastructure handler support
    start_discovery_listener(
        stone_id.clone(),
        stone_name.clone(),
        String::new(), // Endpoint not yet known, will be set in Phase 3.5
        topology_cache.clone(),
        storage_cache.clone(),
        self_entry.clone(),
        console_printer.clone(),
        infrastructure_handlers.clone(),
        manifest_registry.clone(),
    )
    .await;
    tracing::info!("UDP listener started (infrastructure handlers wired)");

    // Phase 1.5: First-boot initialization
    // Linux: Uses flag file, sets hostname/hosts/avahi
    // Windows: Uses hardware-id cache existence, sets DNS hostname via registry
    #[cfg(target_os = "linux")]
    if console::is_first_run() {
        start_first_boot_task(&stone_name, port, config.docker_retry_delay_secs());
    }

    #[cfg(target_os = "windows")]
    {
        let is_first_run = infra::is_first_run_windows();
        if is_first_run {
            start_windows_first_boot_task(&stone_name, port);
        } else {
            // Phase 1.6: Windows DNS hostname maintenance (runs every boot)
            // Ensures DNS hostname matches configured stone_name.
            // Handles case where first boot ran without admin rights (DNS failed),
            // then subsequent boot runs with admin rights (DNS should be retried).
            start_windows_dns_maintenance_task(&stone_name);
        }
    }

    // Phase 2: Network monitoring
    // Create subsystems early so network_ready flag is available for NetworkMonitor
    let subsystems = crate::app_state::SubSystems::default();

    // Runs in background, polls every 5s when disconnected, 30s when connected
    // NetworkMonitor manages the subsystems.network.ready flag
    let network_monitor = NetworkMonitor::start_with_config(
        NetworkMonitorConfig::default()
            .with_disconnect_retry(5)
            .with_connected_poll(30),
        subsystems.network.ready.clone(),
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

    // Phase 7.5: Docker monitoring
    // Runs in background, polls every 5s when disconnected, 30s when connected
    // DockerMonitor manages the subsystems.docker.ready flag
    let _docker_monitor = DockerMonitor::start_with_config(
        docker.clone(),
        DockerMonitorConfig::default()
            .with_disconnect_retry(5)
            .with_connected_poll(30),
        subsystems.docker.ready.clone(),
    ).await;
    tracing::debug!("Docker monitor started (5s retry, 30s poll)");

    // Phase 8: Create channels
    let shutdown_tx = Arc::new(tokio::sync::Notify::new());

    // Phase 8.5: Create domain event bus and SSE channels
    let event_bus = infra::EventBus::new();
    let (sse_tx, _) = tokio::sync::broadcast::channel::<infra::SseEvent>(256);
    tracing::debug!("Domain event bus and SSE channels initialized");

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

    // Phase 10.5: Load unified offerings from disk (includes managed, adopted, borrowed)
    let offerings = match infra::load_offerings().await {
        Ok(offerings) => {
            let managed = offerings.iter().filter(|o| o.is_managed()).count();
            let adopted = offerings.iter().filter(|o| o.is_adopted()).count();
            let borrowed = offerings.iter().filter(|o| o.is_borrowed()).count();
            if !offerings.is_empty() {
                tracing::info!(
                    total = offerings.len(),
                    managed = managed,
                    adopted = adopted,
                    borrowed = borrowed,
                    "Restored unified offerings from disk"
                );
            }
            offerings
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load unified offerings, starting fresh");
            Vec::new()
        }
    };

    // Phase 11: Build AppState
    // Note: manifest_registry and infrastructure_handlers already created at Phase 1
    let ceremony_registry = Arc::new(crate::domain::CeremonyRegistry::new());
    let ceremony_journal = Arc::new(infra::CeremonyJournal::default_journal());
    let harvest_store = Arc::new(infra::HarvestStore::default_store());
    let nurturing_store = Arc::new(infra::NurturingStore::new(infra::HarvestStore::default_store()));

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
        offerings: Arc::new(RwLock::new(offerings)),
        manifest_registry: manifest_registry.clone(),
        docker: docker.clone(),
        jobs: Arc::new(RwLock::new(HashMap::new())),
        sse_tx: sse_tx.clone(),
        event_bus: event_bus.clone(),
        shutdown_tx: shutdown_tx.clone(),
        start_time: std::time::Instant::now(),
        offerings_index: Arc::new(RwLock::new(None)),
        console: console_printer.clone(),
        capabilities: capabilities_arc.clone(),
        network_monitor: Arc::new(network_monitor),
        api_port: port,
        topology_cache: topology_cache.clone(),
        storage_cache: storage_cache.clone(),
        self_entry: self_entry.clone(),
        mdns_handle: mdns_handle.clone(),
        ceremony_registry,
        ceremony_journal,
        harvest_store,
        nurturing_store,
        nourishment_jobs: Arc::new(RwLock::new(HashMap::new())),
        election_service: election_service_placeholder,
        system_resources: Arc::new(RwLock::new(None)),
        companion_registry: Arc::new(infra::CompanionRegistry::new().await),
        infrastructure_handlers: infrastructure_handlers.clone(),
        // Cached metrics - populated by background tasks, read-only for endpoints
        seed_bank_cache: Arc::new(RwLock::new(Vec::new())),
        candidates_cache: Arc::new(RwLock::new(Vec::new())),
        network_metrics_cache: Arc::new(RwLock::new(None)),
        // Notification registry - subsystems set/clear, chirp compiles to tags
        notifications: Arc::new(garden_common::NotificationRegistry::new()),
        // Subsystem readiness (network_ready managed by NetworkMonitor)
        subsystems: subsystems.clone(),
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

    // Phase 11.post4: Start offering lifecycle event listeners
    {
        // ChirpListener: Broadcasts topology changes via UDP
        let self_entry_for_chirp = state.self_entry.clone();
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

        // SseListener: Streams domain events to connected clients (Firefly, Cricket, etc.)
        // Uses shared sse_tx from AppState so presence.rs can subscribe directly
        let sse_listener = infra::SseListener::new(sse_tx.clone());
        let _sse_handle = infra::spawn_listener(&event_bus, Arc::new(sse_listener));

        // TimerListener: Manages nurturing schedule timers (stub - no callback yet)
        let timer_listener = Arc::new(infra::TimerListener::new());
        let _timer_handle = infra::spawn_listener(&event_bus, timer_listener);

        // SeedBankCacheListener: Updates seed bank cache on storage events
        // This ensures portrait endpoint reads from cache without I/O
        let seed_bank_cache_listener = Arc::new(infra::SeedBankCacheListener::new(
            state.seed_bank_cache.clone()
        ));
        let _seed_bank_handle = infra::spawn_listener(&event_bus, seed_bank_cache_listener);

        tracing::info!("Domain event listeners started (chirp, sse, timer, seed_bank_cache)");
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
    start_hardware_detection(stone_name.clone(), capabilities_arc.clone(), console_printer.clone(), state.clone());
    start_registry_loader(state.clone());
    start_catalog_builder(state.clone(), console_printer.clone());

    // System metrics collector (feeds presence protocol and health monitors)
    tracing::info!("Starting system metrics collector");
    let metrics_collector_state = state.clone();
    tokio::spawn(async move {
        crate::tasks::run_metrics_collector(metrics_collector_state).await;
    });

    // Companion registry scan and auto-start (discover and start Companions)
    tracing::info!("Scanning Companion registry");
    let companion_scan_state = state.clone();
    tokio::spawn(async move {
        // Get endpoint for Companion communication
        let endpoint = companion_scan_state.self_entry.read().await.endpoint.clone();
        match companion_scan_state.companion_registry.scan_and_autostart(&endpoint).await {
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
                                tags: vec![], // mDNS doesn't provide tags
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
                tags: vec![], // Will be populated by later chirps
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

    // Phase 15: Subscribe to domain events for immediate announcements
    // Note: ChirpListener already handles topology announcements via EventBus
    // This additional subscription ensures immediate sync for service changes
    let state_for_events = state.clone();
    let mut event_rx = state.event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    // Check if this is a service-related event that needs immediate sync
                    if matches!(&event, crate::domain::DomainEvent::Offering(_)) {
                        tracing::debug!(event_type = event.event_type(), "Service event detected, syncing");
                        state_for_events.sync_self_services(true).await;
                    }
                }
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
    } else {
        // No config file - use default adoption config (profile-aware)
        start_auto_adoption_with_config(state.clone(), infra::AdoptionConfig::default(), &console_printer);
    }

    // Phase 17.5: Storage monitoring (Linux only)
    #[cfg(target_os = "linux")]
    {
        tracing::info!("Starting storage monitor");
        let storage_monitor = crate::infra::storage::StorageMonitor::new(state.event_bus.clone());
        if let Err(e) = storage_monitor.start() {
            tracing::error!("Failed to start storage monitor: {}", e);
        } else {
            // Scan for existing devices at startup
            match storage_monitor.scan_existing().await {
                Ok(devices) => {
                    tracing::info!("Scanned existing storage devices, found {} eligible", devices.len());
                    for device_info in devices {
                        tracing::info!("Found existing device: {} ({} GB)", device_info.device, device_info.capacity_bytes / 1_000_000_000);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to scan existing storage devices: {}", e);
                }
            }
        }
    }

    // Phase 17.6: Seed bank resilience + storage cache hygiene
    // Ensures hot-plugged prepared devices are auto-mounted and cache stays fresh.
    #[cfg(target_os = "linux")]
    {
        crate::tasks::coordinator::start_seedbank_resilient_mount_system(state.clone());
    }
    crate::tasks::start_storage_maintenance(state.storage_cache.clone(), state.topology_cache.clone());
    
    // Populate storage_cache with local seed banks (cross-platform)
    // This makes storage_cache the unified view for both local and remote storage
    let endpoint = state.self_entry.read().await.endpoint.clone();
    if let Err(e) = crate::infra::storage::update_local_storage_cache(
        &state.storage_cache,
        &state.stone_id,
        &state.stone_name,
        &endpoint,
    ).await {
        tracing::warn!("Failed to populate local storage cache: {}", e);
    } else {
        let cache = state.storage_cache.read().await;
        tracing::info!(
            "Storage cache initialized with {} local seed banks",
            cache.get_beacon(&state.stone_id).map(|b| b.seed_banks.len()).unwrap_or(0)
        );
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
#[cfg(target_os = "linux")]
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
                            let _ = console::display_success(&format!("? Stone configured as: {}", new_name));
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

    // Validate all offerings exist before creating job (supports FQN)
    let mut invalid_offerings = Vec::new();
    for offering in &manifest.offerings {
        match parse_offering_fqn(offering) {
            Ok(fqn) => {
                if state.manifest_registry.sw.get(&fqn.offering).is_none() {
                    invalid_offerings.push(offering.clone());
                }
            }
            Err(_) => {
                invalid_offerings.push(offering.clone());
            }
        }
    }

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

/// Start first-boot initialization task (Windows)
///
/// Sets DNS hostname to match the configured stone_name.
/// Note: Name generation happens via generate_unique_name_windows() if config
/// doesn't have a stone_name set.
/// This task handles DNS setup which may require elevation.
#[cfg(target_os = "windows")]
fn start_windows_first_boot_task(stone_name: &str, _port: u16) {
    tracing::info!("First run detected on Windows, setting up DNS hostname");

    let configured_name = stone_name.to_string();

    tokio::spawn(async move {
        // Set DNS hostname (not NetBIOS) via registry
        // Note: Requires elevation - will warn gracefully if running without admin rights
        if let Err(e) = set_windows_dns_hostname(&configured_name).await {
            tracing::warn!(
                error = ?e,
                name = %configured_name,
                "Failed to set Windows DNS hostname (requires elevation). \
                 Stone will work but won't be discoverable by DNS name until manually set."
            );
        } else {
            tracing::info!(
                name = %configured_name,
                "Windows DNS hostname set (reboot required for full effect)"
            );
        }

        tracing::info!(
            stone_name = %configured_name,
            "Windows first-boot complete."
        );
    });
}

/// Windows DNS hostname maintenance task
///
/// Runs on every boot to ensure DNS hostname matches the configured stone_name.
/// This handles the case where first boot ran without admin rights (DNS failed),
/// and subsequent boot runs with admin rights (DNS should be retried).
///
/// Key invariant: We NEVER pick a new name here - only ensure DNS matches config.
#[cfg(target_os = "windows")]
fn start_windows_dns_maintenance_task(stone_name: &str) {
    let configured_name = stone_name.to_string();

    tokio::spawn(async move {
        // Give network time to settle
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Get current DNS hostname
        let current_hostname = get_windows_dns_hostname().await;

        match &current_hostname {
            Some(hostname) if hostname.eq_ignore_ascii_case(&configured_name) => {
                // DNS hostname already matches config - nothing to do
                tracing::debug!(
                    configured = %configured_name,
                    current = %hostname,
                    "DNS hostname matches configuration"
                );
            }
            Some(hostname) => {
                // DNS hostname differs from config - attempt to fix
                tracing::info!(
                    configured = %configured_name,
                    current = %hostname,
                    "DNS hostname mismatch detected, attempting to fix"
                );

                if let Err(e) = set_windows_dns_hostname(&configured_name).await {
                    tracing::warn!(
                        error = ?e,
                        configured = %configured_name,
                        "Failed to set DNS hostname (may require elevation). \
                         Stone will work but DNS discovery may not work correctly."
                    );
                } else {
                    tracing::info!(
                        name = %configured_name,
                        "DNS hostname updated to match configuration (reboot required for full effect)"
                    );
                }
            }
            None => {
                // Couldn't read current hostname - try to set anyway
                tracing::debug!("Could not read current DNS hostname, attempting to set");

                if let Err(e) = set_windows_dns_hostname(&configured_name).await {
                    tracing::debug!(
                        error = ?e,
                        "Failed to set DNS hostname (may require elevation)"
                    );
                }
            }
        }
    });
}

/// Get current Windows DNS hostname from registry
#[cfg(target_os = "windows")]
async fn get_windows_dns_hostname() -> Option<String> {
    let output = tokio::process::Command::new("reg")
        .args(["query", r"HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters", "/v", "Hostname"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("Hostname") && line.contains("REG_SZ") {
            // Line format: "    Hostname    REG_SZ    value"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(hostname) = parts.last() {
                return Some(hostname.to_string());
            }
        }
    }
    None
}

/// Set Windows DNS hostname without changing NetBIOS name
///
/// Writes to registry keys that control DNS hostname.
/// Requires elevation. Requires reboot to take full effect.
#[cfg(target_os = "windows")]
async fn set_windows_dns_hostname(name: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    // Use reg.exe to set DNS hostname (more reliable than winreg crate)
    let tcpip_path = r"HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";

    // Set Hostname
    let output = tokio::process::Command::new("reg")
        .args(["add", tcpip_path, "/v", "Hostname", "/t", "REG_SZ", "/d", name, "/f"])
        .output()
        .await
        .context("Failed to execute reg.exe for Hostname")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(error = %stderr, "Failed to set Hostname registry key");
    }

    // Set NV Hostname (non-volatile, persists across boots)
    let output = tokio::process::Command::new("reg")
        .args(["add", tcpip_path, "/v", "NV Hostname", "/t", "REG_SZ", "/d", name, "/f"])
        .output()
        .await
        .context("Failed to execute reg.exe for NV Hostname")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(error = %stderr, "Failed to set NV Hostname registry key");
    }

    tracing::info!(name = %name, "Set Windows DNS hostname (reboot required)");
    Ok(())
}
