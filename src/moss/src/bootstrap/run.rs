//! Main daemon orchestration
//!
//! Coordinates all startup phases and background tasks.
//! Extracted from main.rs for cleaner separation of concerns.

use super::config::DaemonConfig;
#[cfg(target_os = "linux")]
use crate::run_first_boot_initialization;
use crate::{
    bind_server,
    bootstrap::tls,
    connect_docker,
    // Infrastructure
    infra,
    init_capabilities,
    install_batch_task,
    // Bootstrap functions
    load_preinstall_manifest,
    // mDNS
    mdns,
    router,
    run_server,
    start_auto_adoption,
    start_auto_adoption_with_config,
    start_catalog_builder,
    start_discovery_listener,
    start_hardware_detection,
    start_health_monitor,
    // Task coordination
    start_lantern_registration,
    start_registry_loader,
    version_string,
    AppState,
    DockerConfig,
    // Docker monitoring
    DockerMonitor,
    DockerMonitorConfig,
    Job,
    JobStatus,
    NetworkEvent,
    // Network monitoring
    Network,
    NetworkConfig,
    ServerConfig,
};
use garden_common::console;
use garden_common::offerings::OfferingFqn;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Run the Moss daemon with the given configuration
///
/// This is the main entry point after CLI parsing and config loading.
/// Handles all startup phases and background task coordination.
pub async fn run(
    config: DaemonConfig,
    log: tokio::sync::broadcast::Sender<String>,
) -> anyhow::Result<()> {
    let stone_name = config.stone_name.clone();
    let port = config.port;

    // MOSS-0004: Create shutdown token early so all phases can receive child tokens.
    // The token is cancelled in server.rs when SIGTERM/Ctrl-C is received.
    let shutdown_token = tokio_util::sync::CancellationToken::new();

    // Phase 0: Load or generate stone_id (persistent GUID v7)
    // This must happen early as many components need it
    let stone_id = infra::load_or_generate_stone_id().await;
    tracing::info!(stone_id = %stone_id, stone_name = %stone_name, "Stone identity loaded");

    // Phase 0.5: Initialize self topology entry
    // Create with minimal identity, will be progressively enriched during boot
    let self_entry = Arc::new(RwLock::new(crate::domain::TopologyEntry {
        stone_id: stone_id.clone(),
        stone_name: stone_name.clone(),
        address: garden_common::PeerAddress::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            garden_common::constants::MOSS_HTTP,
        ), // Will be set in Phase 3
        moss_version: version_string(),
        services: Vec::new(),
        mac: None, // Will be set in Phase 2
        health: garden_common::constants::STONE_STARTING.to_string(),
        capabilities: None, // Will be set in Phase 9
        status: garden_common::StoneStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        tags: Vec::new(), // Compiled from NotificationRegistry
        gateways: Vec::new(), // ORCH-0004: populated by gateway API
    }));
    tracing::debug!("Self topology entry initialized (health=starting)");

    // Phase 1: Start UDP listener EARLY (can now respond to discovery requests)
    // Listener needs minimal dependencies: stone_id, stone_name, topology_cache
    // Self-entry will be progressively updated as boot continues
    let topology_cache = Arc::new(RwLock::new(std::collections::HashMap::new()));

    // TOPO-0002: Dirty flag for topology persistence + ensure directory exists
    let topology_dirty = crate::domain::topology::new_dirty_flag();
    if let Err(e) = tokio::fs::create_dir_all(garden_common::constants::paths::topology_dir()).await
    {
        tracing::warn!(error = %e, "Failed to create topology directory (will retry on first write)");
    }

    // Write initial topology file immediately (self entry only, no peers yet).
    // Don't wait for the 30s maintenance cycle — containers may start before then.
    if let Err(e) = crate::domain::topology::persist_topology(&topology_cache, &self_entry).await {
        tracing::warn!(error = %e, "Failed to write initial topology file");
    } else {
        topology_dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        tracing::debug!("Initial topology file written");
    }

    let (tools, _) = tokio::sync::broadcast::channel::<garden_common::tools::ToolDelta>(512);

    // TOOLS-0003: Unified garden registry (replaces tools_cache, storage_cache, gateways)
    let registry = crate::domain::garden_registry::new_registry();

    // Console is needed for UDP listener, create it early
    let console_printer = Arc::new(console::ConsolePrinter::with_dedup_ttl(
        config.console_mode,
        config.event_dedup_ttl_secs,
    ));

    // Create platform runtime for physical console output (ARCH-0002)
    let runtime = crate::infra::platform::create_runtime();

    // Load ManifestRegistry early - needed for infrastructure handlers
    // Uses overlay pattern: embedded assets first, filesystem overlays on top
    let manifests_dir = std::path::PathBuf::from(infra::runtime_manifests_dir());
    let hw_dir = manifests_dir.join("hw");
    let hw_dir_opt = if hw_dir.exists() {
        Some(hw_dir.as_path())
    } else {
        None
    };

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
    let infrastructure_handlers =
        Arc::new(crate::domain::InfrastructureHandlerRegistry::new(vec![
            Box::new(crate::domain::DockerRegistry::new()),
        ]));

    // Create orchestration nudge early — shared between discovery listener and AppState
    let orchestration_nudge = Arc::new(tokio::sync::Notify::new());

    // Unified volume collection (STORAGE-0011) — created empty, populated after AppState
    let volumes = crate::domain::new_volumes();
    // Volume rescan channel — API handlers poke tx, watcher loop consumes rx
    let (volume_rescan, volume_rescan_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Start UDP listener with full infrastructure handler support
    start_discovery_listener(
        stone_id.clone(),
        stone_name.clone(),
        String::new(), // Endpoint not yet known, will be set in Phase 3.5
        topology_cache.clone(),
        topology_dirty.clone(),
        tools.clone(),
        registry.clone(),
        self_entry.clone(),
        console_printer.clone(),
        infrastructure_handlers.clone(),
        manifest_registry.clone(),
        orchestration_nudge.clone(),
        volumes.clone(),
        shutdown_token.child_token(),
    )
    .await;
    tracing::info!("UDP listener started (infrastructure handlers wired)");

    // Phase 1.5: First-boot initialization
    // Linux: Uses flag file, sets hostname/hosts/avahi
    // Windows: Uses hardware-id cache existence, sets DNS hostname via registry
    #[cfg(target_os = "linux")]
    if console::is_first_run() {
        start_first_boot_task(&stone_name, port, config.docker_retry_delay_secs(), runtime.clone());
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
    // Create subsystems early so network_ready flag is available for Network
    let subsystems = crate::app_state::SubSystems::default();

    // Runs in background, polls every 5s when disconnected, 30s when connected
    // Network manages the subsystems.network.ready flag
    let network = Network::start_with_config(
        NetworkConfig::default()
            .with_disconnect_retry(5)
            .with_connected_poll(30),
        subsystems.network.ready.clone(),
    )
    .await;

    // Phase 2.5: Get MAC address for self entry
    let (_, mac_address) = garden_common::infra::network::get_local_ip_and_mac();

    // Phase 3: Resolve API endpoint
    // Prefer explicit STONE_HOST, otherwise use monitored network IP
    let use_static_host = std::env::var(garden_common::ENV_STONE_HOST)
        .ok()
        .filter(|h| !h.trim().is_empty());

    let resolved_ip: std::net::IpAddr = if let Some(host) = &use_static_host {
        host.trim()
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    } else {
        network
            .get_ip()
            .await
            .parse()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
    };

    // Phase 3.5: Update self entry with network configuration
    {
        let mut entry = self_entry.write().await;
        entry.address = garden_common::PeerAddress::new(resolved_ip, port);
        entry.mac = mac_address.clone();
        entry.health = garden_common::constants::STONE_INITIALIZING.to_string();
        entry.last_seen = chrono::Utc::now();
    }
    tracing::debug!(ip = %resolved_ip, port = port, mac = ?mac_address, "Self entry updated (health=initializing)");

    // Derive the plain HTTP endpoint string for legacy consumers
    let api_endpoint = format!("http://{}:{}", resolved_ip, port);

    // Auto-chirp: Network configuration complete
    {
        let entry = self_entry.read().await.clone();
        if let Err(e) = crate::announcement::announce(&entry).await {
            tracing::warn!(error = ?e, "Failed to auto-chirp after network config");
        } else {
            tracing::debug!("Auto-chirp sent after network configuration");
        }
    }

    // Phase 4: Initialize Koi embedded (mDNS + certmesh + capabilities)
    // Certmesh is now enabled for Pond security (CA lifecycle, enrollment, certs).
    // Other capabilities (dns, proxy, health) remain dormant until explicitly started.
    let koi_data_dir =
        std::path::PathBuf::from(garden_common::constants::paths::data_dir()).join("koi");

    // Shared pond state flag — created before mDNS so both MdnsHandle and AppState
    // observe the same value. Handlers flip this after init/unlock/destroy.
    let pond_active = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let koi_handle = {
        let koi = koi_embedded::Builder::new()
            .data_dir(koi_data_dir)
            .service_mode(koi_embedded::ServiceMode::EmbeddedOnly)
            .http(true)
            .http_port(garden_common::constants::KOI_HTTP)
            .mdns(true)
            .dns_enabled(true)
            .dns_auto_start(true)
            .dns(|cfg| {
                cfg.port(garden_common::constants::KOI_DNS)
                    .zone("zengarden")
                    .local_zone(true)
            })
            .health(false)
            .certmesh(true)
            .proxy(false)
            .udp(true)
            .dashboard(true)
            .mdns_browser(true)
            .events(|event| {
                tracing::debug!(?event, "koi event");
            })
            .extra_firewall_ports(vec![
                koi_embedded::FirewallPort::new(
                    "Discovery",
                    koi_embedded::FirewallProtocol::Udp,
                    garden_common::constants::DISCOVERY_UDP,
                ),
                koi_embedded::FirewallPort::new(
                    "HTTP API",
                    koi_embedded::FirewallProtocol::Tcp,
                    garden_common::constants::MOSS_HTTP,
                ),
            ])
            .ensure_firewall_rules("Zen Garden")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build Koi embedded: {}", e))?;

        let handle = koi
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start Koi embedded: {}", e))?;

        tracing::info!("Koi embedded started (mDNS + certmesh + HTTP + DNS + UDP + dashboard + browser active)");
        Arc::new(handle)
    };

    // Phase 4.0.1: Seed pond_active from persisted certmesh state
    // Two cases: (a) cornerstone with CA initialized + unlocked, or
    // (b) enrolled member with cert files from a prior enrollment.
    // Also seeds the PondState domain surface (no event emitted at boot).
    //
    // Auto-unlock now happens inside koi-embedded's init_certmesh_core(),
    // so by this point the CA is already unlocked if the key file exists.
    // We just read the status and seed the application state.
    let pond_state = crate::domain::PondState::new();
    if let Ok(cm) = koi_handle.certmesh() {
        if let Ok(core) = cm.core() {
            let status = core.certmesh_status().await;
            if status.ca_initialized && !status.ca_locked {
                pond_active.store(true, std::sync::atomic::Ordering::Relaxed);
                pond_state.seed_enrolled(true);
                tracing::info!("Pond active — CA initialized and unlocked");

                // Register _certmesh._tcp mDNS so Rake clients can discover us
                crate::mdns::register_certmesh_service(
                    &koi_handle,
                    garden_common::constants::MOSS_HTTP,
                )
                .await;
            } else if status.ca_initialized {
                // CA is initialized but still locked — no auto-unlock key
                // existed, or decryption failed.  Report available methods.
                let slot_table_path = koi_certmesh::ca::slot_table_path();
                if slot_table_path.exists() {
                    if let Ok(table) = koi_crypto::unlock_slots::SlotTable::load(&slot_table_path) {
                        let methods = table.available_methods();
                        if methods.contains(&"totp") {
                            tracing::info!(
                                "Pond CA locked — unlock with TOTP code via 'POST /api/v1/pond/unlock' or 'garden-rake pond unlock --totp'"
                            );
                        } else if methods.contains(&"fido2") {
                            tracing::info!("Pond CA locked — unlock with security key via pond UI");
                        } else {
                            tracing::info!(
                                "Pond CA locked — run 'garden-rake pond unlock' with passphrase"
                            );
                        }
                    } else {
                        tracing::info!(
                            "Pond CA exists but is locked — run 'garden-rake pond unlock'"
                        );
                    }
                } else {
                    tracing::info!("Pond CA exists but is locked — run 'garden-rake pond unlock'");
                }
            }
        }
    }
    // Enrolled member fallback: check for enrollment certs on disk
    if !pond_active.load(std::sync::atomic::Ordering::Relaxed) {
        let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
            .join("koi")
            .join("certs")
            .join(&stone_name);
        if certs_dir.join("cert.pem").exists() && certs_dir.join("key.pem").exists() {
            pond_active.store(true, std::sync::atomic::Ordering::Relaxed);
            pond_state.seed_enrolled(true);
            tracing::info!("Pond active — enrolled member with certs from previous enrollment");
        }
    }

    // Seed pond name from persisted metadata
    let pond_metadata = crate::domain::load_pond_metadata();
    pond_state.seed_name(pond_metadata.name).await;

    // Phase 4.0.1b: Propagate pond state into self topology entry
    if pond_active.load(std::sync::atomic::Ordering::Relaxed) {
        let mut entry = self_entry.write().await;
        entry.address = entry
            .address
            .clone()
            .with_tls(garden_common::constants::MOSS_HTTPS);
        tracing::debug!(
            "Self entry updated with TLS port {}",
            garden_common::constants::MOSS_HTTPS
        );
    }

    // Phase 4.0.2–4.0.3: Chirp signing + verification
    // Deferred to Phase 18 boot (activate_pond_security) and the
    // enrollment-change listener (Phase 11.3). No duplicated code here.

    // Phase 4.1: mDNS announcement — includes stone_id and MAC in TXT records
    // Must happen before IP change handler so we can pass the handle
    // Note: If current IP is loopback, registration is deferred until valid IP is available
    let current_ip = network.get_ip().await;
    let (_, mac_for_mdns) = garden_common::infra::network::get_local_ip_and_mac();
    let mdns_handle: Option<Arc<mdns::MdnsHandle>> = match mdns::announce_moss(
        koi_handle.clone(),
        Some(stone_id.as_str()),
        &stone_name,
        port,
        mac_for_mdns.as_deref(),
        &current_ip, // Gate: won't register if loopback
        crate::cli::VERSION,
        pond_active.clone(),
    )
    .await
    {
        Ok(handle) => {
            console_printer.emit(console::ConsoleEvent::new(
                console::EventCategory::Discovery,
                console::EventStatus::MdnsActive,
                format!("mDNS backend: {}", handle.status_label()),
            ));
            Some(Arc::new(handle))
        }
        Err(e) => {
            console_printer.emit(console::ConsoleEvent::new(
                console::EventCategory::Discovery,
                console::EventStatus::MdnsError,
                format!("mDNS announcement failed: {}", e),
            ));
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
        &network,
        Some(&console_printer),
        shutdown_token.child_token(),
    )
    .await;

    emit_startup_events(&console_printer, &config);

    // Phase 7: Docker connection
    let docker = connect_docker(&console_printer, &*runtime, DockerConfig::default()).await?;

    // Phase 7.1: Configure systemd-resolved for container DNS
    // resolved owns port 53; Koi DNS serves .zengarden on port 5642.
    // We configure resolved to:
    //   1. Listen on the Docker bridge gateway (so containers can reach it)
    //   2. Route .zengarden queries to Koi DNS
    if let Some(gw) = docker.bridge_gateway().await {
        if let Err(e) = configure_resolved_for_containers(&gw).await {
            tracing::warn!(error = %e, "could not configure systemd-resolved (containers may not resolve stone names)");
        }
    } else {
        tracing::warn!("could not determine Docker bridge gateway — container DNS may not work");
    }

    // Phase 7.2: Reconcile DNS on existing managed containers
    // Containers created before systemd-resolved integration still point at the
    // router. Patch their /etc/resolv.conf in-place (no restart needed).
    match docker.list_zen_containers().await {
        Ok(containers) => {
            for name in &containers {
                if let Err(e) = docker.reconcile_container_dns(name).await {
                    tracing::debug!(service = %name, error = %e, "DNS reconciliation skipped");
                }
            }
        }
        Err(e) => {
            tracing::debug!(error = %e, "could not list containers for DNS reconciliation");
        }
    }

    // Phase 7.5: Docker monitoring
    // Runs in background, polls every 5s when disconnected, 30s when connected
    // DockerMonitor manages the subsystems.docker.ready flag
    let _docker_monitor = DockerMonitor::start_with_config(
        docker.clone(),
        DockerMonitorConfig::default()
            .with_disconnect_retry(5)
            .with_connected_poll(30),
        subsystems.docker.ready.clone(),
    )
    .await;
    tracing::debug!("Docker monitor started (5s retry, 30s poll)");

    // Phase 8: Create domain event bus and pulse channel
    let event_bus = infra::EventBus::new();
    let (pulse, _) = tokio::sync::broadcast::channel::<infra::PulseEvent>(512);
    tracing::debug!("Domain event bus and pulse channel initialized");

    // Phase 9: Capabilities loading
    let capabilities = init_capabilities(&stone_id, &stone_name, &console_printer).await;

    // Phase 9.5: Update self entry with capabilities and set health to thriving
    {
        let mut entry = self_entry.write().await;
        entry.capabilities = capabilities.read().await.clone();
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
    let nurturing_store = Arc::new(infra::NurturingStore::new(
        infra::HarvestStore::default_store(),
    ));

    // Storage channels (ARCH-0004) — created here so they can be shared between
    // state.storage (domain context) and the flat AppState fields being migrated.
    let (storage_tick_tx, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageTick>(64);
    let (storage_agg_tx, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageTick>(64);
    let (storage_changed_tx, _) =
        tokio::sync::broadcast::channel::<garden_common::storage::StorageChanged>(64);
    let nourishment_map = Arc::new(RwLock::new(
        HashMap::<String, tokio::sync::broadcast::Sender<String>>::new(),
    ));
    let media = crate::domain::storage::new_media();

    // Pre-clone for storage context (values are moved into flat fields earlier in the literal)
    let harvest_for_storage = Arc::clone(&harvest_store);
    let nurturing_for_storage = Arc::clone(&nurturing_store);

    // Phase 11.pre: Create election service (placeholder for now, will be updated after AppState)
    // Note: No longer async - no socket binding (uses p2p transport)
    let election_service_placeholder =
        Arc::new(crate::tasks::election_service::Elections::new(
            stone_id.clone(),
            stone_name.clone(),
            Box::new(crate::tasks::state_provider::PlaceholderStateProvider),
        ));

    let state = AppState {
        stone_id: stone_id.clone(),
        stone_name: stone_name.clone(),
        offerings: Arc::new(RwLock::new(offerings)),
        manifest_registry: manifest_registry.clone(),
        docker: docker.clone(),
        jobs: Arc::new(RwLock::new(HashMap::new())),
        pulse: pulse.clone(),
        event_bus: event_bus.clone(),
        shutdown_token: shutdown_token.clone(),
        start_time: std::time::Instant::now(),
        offerings_index: Arc::new(RwLock::new(None)),
        console: console_printer.clone(),
        runtime: runtime.clone(),
        capabilities: capabilities.clone(),
        network: Arc::new(network),
        api_port: port,
        topology_cache: topology_cache.clone(),
        topology_dirty: topology_dirty.clone(),
        tools: tools.clone(),
        registry: registry.clone(),
        self_entry: self_entry.clone(),
        mdns_handle: mdns_handle.clone(),
        koi_handle: koi_handle.clone(),
        pond: pond_state,
        pond_active: pond_active.clone(),
        https_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        stone_client: Arc::new(infra::stone_client::StoneClient::new(&stone_name)),
        ceremony_registry,
        ceremony_journal,
        pond_ceremony_host: Arc::new(koi_common::ceremony::CeremonyHost::new(
            koi_certmesh::pond_ceremony::PondCeremonyRules,
        )),
        harvest_store,
        nurturing_store,
        nourishment_jobs: Arc::new(RwLock::new(HashMap::new())),
        elections: election_service_placeholder,
        system_resources: Arc::new(RwLock::new(None)),
        companion_registry: Arc::new(infra::CompanionRegistry::new().await),
        infrastructure_handlers: infrastructure_handlers.clone(),
        // Cached metrics - populated by background tasks, read-only for endpoints
        network_metrics_cache: Arc::new(RwLock::new(None)),
        // FIREFLY-0003: GPU utilization cache
        gpu_utilization: Arc::new(RwLock::new(None)),
        // Notification registry - subsystems set/clear, chirp compiles to tags
        notifications: Arc::new(garden_common::NotificationRegistry::new()),
        // Log broadcast channel (for live SSE log streaming)
        log: log.clone(),
        // Subsystem readiness (network_ready managed by Network)
        subsystems: subsystems.clone(),
        // Storage domain context (ARCH-0004) — groups all storage runtime state.
        // Flat fields below are being migrated here incrementally.
        storage: Arc::new(crate::domain::Storage {
            orchestration: crate::domain::storage::Orchestration {
                tick: storage_tick_tx.clone(),
                agg: storage_agg_tx.clone(),
                nudge: orchestration_nudge.clone(),
                rescan: volume_rescan.clone(),
            },
            volumes: volumes.clone(),
            media: media.clone(),
            changed: storage_changed_tx.clone(),
            harvest: harvest_for_storage,
            nurturing: nurturing_for_storage,
            nourishment: nourishment_map.clone(),
        }),
    };

    // Phase 11.post: Update election service with proper state provider now that AppState exists
    // Note: No longer async - no socket binding (uses p2p transport)
    let state_for_election = Arc::new(state.clone());
    let election_service_final = Arc::new(crate::tasks::election_service::Elections::new(
        stone_id.clone(),
        stone_name.clone(),
        Box::new(crate::tasks::state_provider::MossStateProvider::new(
            state_for_election,
        )),
    ));

    // Update the state's elections
    let state = AppState {
        elections: election_service_final.clone(),
        ..state
    };

    tracing::info!("Election service initialized (using p2p transport)");

    // Phase 11.post1.5: Inject fitness provider for ORCH-0001 elections
    {
        let state_for_fitness = Arc::new(state.clone());
        state
            .elections
            .set_fitness_provider(Box::new(
                crate::tasks::state_provider::MossFitnessProvider::new(state_for_fitness),
            ))
            .await;
        tracing::info!("Fitness provider injected into election service (ORCH-0001)");
    }

    // Phase 11.post2: Start election service listener (subscribes to p2p events)
    tokio::spawn(async move {
        if let Err(e) = election_service_final.run_listener().await {
            tracing::error!(error = ?e, "Election service listener failed");
        }
    });

    // Phase 11.post3: Start discovery handler (responds to discovery requests)
    let self_entry_for_discovery = state.self_entry.clone();
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
        let volumes = state.storage.volumes.clone();
        crate::domain::storage::initial_scan(&volumes).await;
    }

    // Phase 11.post4b: Initial media scan (STORAGE-0011)
    // Detects physical disks including those without partitions or drive letters.
    // Uses PowerShell Get-Disk (Windows) or lsblk (Linux).
    {
        let media = state.storage.media.clone();
        let snapshots = tokio::task::spawn_blocking(crate::infra::storage::platform::scan_media)
            .await
            .unwrap_or_default();
        crate::domain::storage::reconcile_media(&media, &snapshots).await;
    }

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
            .self_entry
            .read()
            .await
            .address
            .http_base();
        match companion_scan_state
            .companion_registry
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
    if use_static_host.is_none() {
        let state_for_ip = state.clone();
        let mut network_rx = state.network.subscribe();

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
    if let Some(ref mdns) = state.mdns_handle {
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
                        state_for_pond.stone_client.reload_tls();

                        if enrolled {
                            activate_pond_security(&state_for_pond, &console_for_pond).await;
                        } else {
                            // HTTPS shutdown is not implemented yet (Phase 3+).
                            // For now, just update the flag so new connections see the change.
                            state_for_pond
                                .https_started
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
    let topology_cache_for_mdns = state.topology_cache.clone();
    let topology_dirty_for_mdns = state.topology_dirty.clone();
    let self_stone_name_for_mdns = stone_name.clone();
    if let Ok(mut mdns_rx) =
        mdns::start_mdns_lurk_listener(koi_handle.clone(), stone_name.clone()).await
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
                            let entry = crate::domain::TopologyEntry {
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
            request_id: garden_common::ids::generate_guidv7(),
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
                    let entry = crate::domain::TopologyEntry {
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
                        &state.topology_cache,
                        entry,
                        &state.topology_dirty,
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
    let entry = state.self_entry.read().await.clone();
    if let Err(e) = crate::announcement::announce(&entry).await {
        tracing::warn!(error = ?e, "Initial announcement failed");
    }

    // Phase 14: Start periodic announcer (30s background task)
    crate::tasks::start_periodic_announcer(state.clone(), shutdown_token.child_token());

    // Phase 16: Pre-install manifest handling
    start_preinstall_handler(&state).await;

    // Phase 17: Health monitoring and auto-adoption
    start_health_monitor(state.clone(), shutdown_token.child_token());
    if let Some(cfg) = config.file_config.clone() {
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

    // Phase 17.5: Cross-platform volume watcher (STORAGE-0011)
    // Detects volume hotplug/removal events and feeds them into the Volumes domain.
    // Also emits DomainPulse events so presence SSE (ribbon notifications) and
    // the candidates notification stay current.
    {
        use crate::infra::storage::platform::VolumeEvent;
        use garden_common::{NotificationTag, NOTIF_SOURCE_CANDIDATES};

        let (vol_tx, mut vol_rx) = tokio::sync::mpsc::channel(32);
        crate::infra::storage::platform::start_volume_watcher(vol_tx);

        let volumes_for_watcher = state.storage.volumes.clone();
        let pulse_tx_for_watcher = state.pulse.clone();
        let storage_changed_tx_for_watcher = state.storage.changed.clone();
        let notifications = state.notifications.clone();
        let watcher_token = shutdown_token.child_token();
        let mut rescan_rx = volume_rescan_rx; // move rx into the watcher task
        tokio::spawn(async move {
            /// Process a single volume event: classify, emit pulse, update notifications.
            async fn handle_volume_event(
                ev: VolumeEvent,
                volumes: &crate::domain::Volumes,
                pulse: &tokio::sync::broadcast::Sender<infra::PulseEvent>,
                storage_changed: &tokio::sync::broadcast::Sender<garden_common::storage::StorageChanged>,
                notifications: &garden_common::NotificationRegistry,
            ) {
                // Build pulse before ingest consumes the event info
                let event_pulse = match &ev {
                    VolumeEvent::Appeared(snap) => {
                        let capacity_gb = snap.capacity_bytes / 1_000_000_000;
                        Some(infra::DomainPulse::storage_event(
                            "storage_detected",
                            format!("Volume appeared: {} ({})", snap.mount_path, snap.label.as_deref().unwrap_or("unlabeled")),
                            "info",
                            None,
                            Some(serde_json::json!({
                                "device": snap.path,
                                "mount_path": snap.mount_path,
                                "label": snap.label,
                                "capacity_gb": capacity_gb,
                                "removable": snap.removable,
                            })),
                        ))
                    }
                    VolumeEvent::Disappeared { path } => {
                        Some(infra::DomainPulse::storage_event(
                            "storage_removed",
                            format!("Volume disappeared: {}", path),
                            "info",
                            None,
                            Some(serde_json::json!({ "device": path })),
                        ))
                    }
                };

                // Ingest into Volumes domain; returns domain events to broadcast
                let storage_events = crate::domain::storage::ingest_event(volumes, ev).await;

                // Emit pulse for presence SSE / ribbon notifications
                if let Some(p) = event_pulse {
                    let _ = pulse.send(infra::PulseEvent::Domain(p));
                }

                // Broadcast all domain events immediately so cloud filter,
                // WebDAV router, and TTY ribbon display react without waiting
                // for the next heartbeat.
                for event in storage_events {
                    tracing::debug!(event = ?event, "storage watcher: broadcasting domain event");
                    let _ = storage_changed.send(event);
                }

                // Update candidates notification
                let candidate_count = {
                    let map = volumes.read().await;
                    map.values()
                        .filter(|v| !v.is_managed() && v.removable && v.online)
                        .count()
                };
                notifications.set_if(
                    NOTIF_SOURCE_CANDIDATES,
                    NotificationTag::Opportunity,
                    candidate_count > 0,
                );
            }

            loop {
                tokio::select! {
                    _ = watcher_token.cancelled() => break,
                    event = vol_rx.recv() => {
                        let Some(ev) = event else { break };
                        handle_volume_event(
                            ev,
                            &volumes_for_watcher,
                            &pulse_tx_for_watcher,
                            &storage_changed_tx_for_watcher,
                            &notifications,
                        ).await;
                    }
                    _ = rescan_rx.recv() => {
                        // Ad-hoc rescan requested (e.g. after `storage add` wrote a manifest).
                        // Re-scan all volumes through the standard pipeline.
                        let snaps = tokio::task::spawn_blocking(
                            crate::infra::storage::platform::scan_volumes
                        )
                        .await
                        .unwrap_or_default();
                        crate::domain::storage::reconcile(&volumes_for_watcher, &snaps).await;
                        crate::domain::storage::health_tick_all(&volumes_for_watcher).await;

                        // Update candidates notification after rescan
                        let candidate_count = {
                            let map = volumes_for_watcher.read().await;
                            map.values()
                                .filter(|v| !v.is_managed() && v.removable && v.online)
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
        tracing::info!("Volume watcher started (STORAGE-0011)");
    }

    // Phase 17.5.2: Physical media watcher (STORAGE-0011)
    // Polls physical disks (PowerShell/lsblk) to detect media without partitions.
    // Lower cadence than the volume watcher since physical changes are rarer.
    {
        let media = state.storage.media.clone();
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
        state.topology_cache.clone(),
        state.topology_dirty.clone(),
        state.self_entry.clone(),
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
        let raw_rx = state.storage.orchestration.tick.subscribe();
        let agg_tx = state.storage.orchestration.agg.clone();
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
        let cf_endpoint = {
            let entry = state.self_entry.read().await;
            Arc::new(RwLock::new(entry.address.http_base()))
        };
        if let Err(e) = crate::infra::cloud_filter::start(
            state.storage.volumes.clone(),
            state.registry.clone(),
            state.stone_id.clone(),
            state.storage.orchestration.tick.clone(),
            state.subscribe_storage_changed(),
            cf_endpoint,
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
            state.storage.volumes.clone(),
            state.storage.orchestration.tick.clone(),
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

    // Phase 18: HTTP server
    tracing::info!("Setting up HTTP router with 200 MB body limit");

    // When pond security is active, split routes across two listeners:
    // - HTTP :7185 → public lobby (health, discovery, pond join/status)
    // - HTTPS :7183 → all routes (authenticated, full API)
    let pond_is_active = state.pond_active.load(std::sync::atomic::Ordering::Relaxed);

    // If already enrolled at boot, activate HTTPS + chirp signing/verification
    if pond_is_active {
        activate_pond_security(&state, &console_printer).await;
    }

    let app = if pond_is_active
        && state
            .https_started
            .load(std::sync::atomic::Ordering::Relaxed)
    {
        tracing::info!(
            "Pond active: HTTP :{} serves public lobby, HTTPS :{} serves all routes",
            port,
            garden_common::constants::MOSS_HTTPS
        );
        router::configure_public(state.clone())
    } else {
        router::configure(state.clone())
    };

    let listener = bind_server(port, &console_printer).await?;

    // Create shutdown callback to flush topology and send goodbye announcement
    let goodbye_state = state.clone();
    let shutdown_callback: crate::bootstrap::server::ShutdownCallback = Box::new(move || {
        Box::pin(async move {
            // TOPO-0002: Flush topology to disk before shutdown
            crate::domain::topology::flush_topology(
                &goodbye_state.topology_cache,
                &goodbye_state.topology_dirty,
                &goodbye_state.self_entry,
            )
            .await;

            if let Err(e) = crate::announcement::send_goodbye(&goodbye_state).await {
                tracing::warn!(error = ?e, "Failed to send goodbye announcement");
            }
        })
    });

    // Prepare boot banner info
    let current_ip = state.network.get_ip().await;
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
        runtime,
        shutdown_token,
        ServerConfig::default(),
        Some(shutdown_callback),
        boot_banner,
        shutdown_banner,
        Some(state.companion_registry.clone()),
    )
    .await
}

/// Configure systemd-resolved for Zen Garden container DNS.
///
/// 1. `DNSStubListenerExtra=<bridge_gw>` — resolved listens on Docker bridge
///    gateway so containers can use it for DNS.
/// 2. `resolvectl dns docker0 <koi_dns>` + `resolvectl domain docker0 ~zengarden`
///    — routes `.zengarden` queries to Koi DNS (port 5642).
///    Uses `docker0` because `lo` (loopback) is rejected by resolvectl.
#[cfg(target_os = "linux")]
async fn configure_resolved_for_containers(bridge_gw: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    // 1. Ensure resolved listens on Docker bridge gateway
    let conf_path = "/etc/systemd/resolved.conf.d/zen-garden.conf";
    let desired = format!(
        "[Resolve]\nMulticastDNS=resolve\nDNSStubListenerExtra={}\n",
        bridge_gw
    );

    let needs_restart = match tokio::fs::read_to_string(conf_path).await {
        Ok(existing) => existing != desired,
        Err(_) => true,
    };

    if needs_restart {
        tokio::fs::create_dir_all("/etc/systemd/resolved.conf.d").await?;
        tokio::fs::write(conf_path, &desired).await?;

        let output = tokio::process::Command::new("systemctl")
            .args(["restart", "systemd-resolved"])
            .output()
            .await
            .context("restart systemd-resolved")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(%stderr, "systemctl restart systemd-resolved returned non-zero");
        } else {
            tracing::info!(bridge_gw = %bridge_gw, "configured resolved bridge listener");
        }
    }

    // 2. Route .zengarden queries to Koi DNS via docker0 interface.
    //    resolvectl rejects loopback (lo), so we use docker0 which is the
    //    Docker bridge interface. Wait for it to appear (Docker may still
    //    be initializing the bridge network on slow hardware).
    let koi_dns = format!("127.0.0.1:{}", garden_common::constants::KOI_DNS);

    // Wait for docker0 to exist (up to 5s)
    let mut docker0_ready = false;
    for attempt in 1..=10 {
        let check = tokio::process::Command::new("ip")
            .args(["link", "show", "docker0"])
            .output()
            .await;
        if check.map(|o| o.status.success()).unwrap_or(false) {
            docker0_ready = true;
            break;
        }
        if attempt < 10 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    if !docker0_ready {
        tracing::warn!("docker0 interface not found after 5s — .zengarden DNS routing skipped");
        return Ok(());
    }

    let dns_output = tokio::process::Command::new("resolvectl")
        .args(["dns", "docker0", &koi_dns])
        .output()
        .await
        .context("resolvectl dns")?;
    if !dns_output.status.success() {
        let stderr = String::from_utf8_lossy(&dns_output.stderr);
        tracing::warn!(%stderr, "resolvectl dns docker0 failed — .zengarden routing unavailable");
        return Ok(());
    }

    let domain_output = tokio::process::Command::new("resolvectl")
        .args(["domain", "docker0", "~zengarden"])
        .output()
        .await
        .context("resolvectl domain")?;
    if !domain_output.status.success() {
        let stderr = String::from_utf8_lossy(&domain_output.stderr);
        tracing::warn!(%stderr, "resolvectl domain docker0 failed — .zengarden routing unavailable");
        return Ok(());
    }

    tracing::info!(
        port = garden_common::constants::KOI_DNS,
        "configured .zengarden routing to Koi DNS"
    );
    Ok(())
}

/// No-op on non-Linux platforms.
#[cfg(not(target_os = "linux"))]
async fn configure_resolved_for_containers(_bridge_gw: &str) -> anyhow::Result<()> {
    Ok(())
}

/// Start first-boot initialization task (Linux only)
///
/// Waits for filesystem to become writable, then runs initialization.
/// Exits process after completion so systemd restarts with new config.
#[cfg(target_os = "linux")]
fn start_first_boot_task(
    stone_name: &str,
    port: u16,
    retry_delay_secs: u64,
    runtime: std::sync::Arc<dyn garden_common::PlatformRuntime>,
) {
    tracing::info!("First run detected on Linux, spawning background initialization task");
    tracing::info!("First boot detected - will initialize console after Docker connection");

    let init_stone_name = stone_name.to_string();
    let init_port = port;

    tokio::spawn(async move {
        const MAX_ATTEMPTS: u32 = 20;

        runtime.write_line("");
        runtime.display_wait("First-boot setup: Waiting for filesystem to become writable");

        for attempt in 1..=MAX_ATTEMPTS {
            match console::ensure_etc_writable().await {
                Ok(true) => {
                    tracing::info!(
                        attempt,
                        "Filesystem is writable, proceeding with first boot initialization"
                    );
                    runtime.display_success("Filesystem ready, starting configuration");

                    match run_first_boot_initialization(&*runtime, &init_stone_name, init_port).await {
                        Ok(new_name) => {
                            if let Err(e) = console::mark_first_run_complete().await {
                                tracing::error!(error = ?e, "Failed to mark first-run complete");
                            }

                            tracing::info!(new_name = %new_name, "First boot initialization completed successfully");
                            runtime.write_line("");
                            runtime.display_success(&format!(
                                "Stone configured as: {}",
                                new_name
                            ));
                            runtime.display_wait("Restarting to apply new configuration...");
                            runtime.write_line("");

                            // Exit so systemd restarts us with the new configuration
                            std::process::exit(0);
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "First boot initialization failed");
                            runtime.display_error(&format!("Setup failed: {}", e));
                            if attempt < MAX_ATTEMPTS {
                                tokio::time::sleep(tokio::time::Duration::from_secs(
                                    retry_delay_secs,
                                ))
                                .await;
                            }
                        }
                    }
                }
                Ok(false) | Err(_) => {
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs))
                            .await;
                    } else {
                        tracing::error!("First boot initialization abandoned - filesystem never became writable");
                        runtime.display_error(
                            "Setup abandoned - filesystem remained read-only",
                        );
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
        format!("Moss v{}", version_string()),
    ));

    if config.file_config.is_some() {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::Loaded,
            "Configuration file".to_string(),
        ));

        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::Merged,
            "Priority: CLI > Env > Config > Defaults".to_string(),
        ));
    } else {
        console_printer.emit(console::ConsoleEvent::new(
            console::EventCategory::Config,
            console::EventStatus::NotFound,
            "Using defaults".to_string(),
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
        match OfferingFqn::parse(offering) {
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
                        if let Err(e) =
                            tokio::fs::remove_file("/home/stone/garden-moss-preinstall.json").await
                        {
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

    tracing::info!(
        "Pre-install job started: {} (check /api/jobs/{})",
        job_id,
        job_id
    );
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
        if let Err(e) = set_windows_dns_hostname(&configured_name) {
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
        let current_hostname = get_windows_dns_hostname();

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

                if let Err(e) = set_windows_dns_hostname(&configured_name) {
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

                if let Err(e) = set_windows_dns_hostname(&configured_name) {
                    tracing::debug!(
                        error = ?e,
                        "Failed to set DNS hostname (may require elevation)"
                    );
                }
            }
        }
    });
}

/// Get current Windows DNS hostname from registry.
#[cfg(target_os = "windows")]
fn get_windows_dns_hostname() -> Option<String> {
    crate::infra::platform::registry::get_dns_hostname()
}

/// Set Windows DNS hostname without changing NetBIOS name.
/// Requires elevation. Requires reboot to take full effect.
#[cfg(target_os = "windows")]
fn set_windows_dns_hostname(name: &str) -> anyhow::Result<()> {
    crate::infra::platform::registry::set_dns_hostname(name)
}

/// Activate pond security features (HTTPS + chirp signing/verification).
///
/// Called reactively from the enrollment-change listener or at boot.
/// Idempotent: HTTPS binding guarded by `https_started`; chirp enricher/verifier
/// use `OnceLock` which silently ignores second calls.
async fn activate_pond_security(
    state: &AppState,
    console: &garden_common::console::ConsolePrinter,
) {
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(&state.stone_name);
    let key_path = certs_dir.join("key.pem");
    let cert_path = certs_dir.join("cert.pem");

    // --- Chirp signing ---
    if key_path.exists() && cert_path.exists() {
        if let Ok(key_pem) = std::fs::read_to_string(&key_path) {
            if let Ok(keypair) = koi_crypto::keys::ca_keypair_from_pem(&key_pem) {
                use base64::Engine;
                let public_key_pem = keypair.public_key_pem();
                let _ = garden_common::infra::communications::p2p::set_envelope_enricher(Box::new(
                    move |announcement| {
                        if let Ok(data_bytes) = serde_json::to_vec(&announcement.data) {
                            let sig = koi_crypto::signing::sign_bytes(&keypair, &data_bytes);
                            announcement.signature =
                                Some(base64::engine::general_purpose::STANDARD.encode(&sig));
                            announcement.sender_cert = Some(public_key_pem.clone());
                        }
                    },
                ));
                tracing::info!("Chirp signing enabled");
            }
        }
    }

    // --- Chirp verification ---
    let ca_cert_path = koi_certmesh::ca::ca_cert_path();
    if ca_cert_path.exists() {
        if let Ok(_ca_pem) = std::fs::read_to_string(&ca_cert_path) {
            let _ = garden_common::infra::communications::p2p::set_envelope_verifier(Box::new(
                move |announcement| {
                    use base64::Engine;

                    let (sig_b64, _sender_cert) = match (
                        announcement.signature.as_deref(),
                        announcement.sender_cert.as_deref(),
                    ) {
                        (Some(s), Some(c)) => (s, c),
                        _ => return true, // Accept unsigned during transition
                    };

                    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(sig_b64)
                    {
                        Ok(b) => b,
                        Err(_) => return false,
                    };

                    let data_bytes = match serde_json::to_vec(&announcement.data) {
                        Ok(b) => b,
                        Err(_) => return false,
                    };

                    let sender_cert_pem = announcement.sender_cert.as_deref().unwrap_or_default();

                    koi_crypto::signing::verify_signature(sender_cert_pem, &data_bytes, &sig_bytes)
                },
            ));
            tracing::info!("Chirp verification enabled");
        }
    }

    // --- HTTPS listener ---
    if state
        .https_started
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_ok()
    {
        let handle = tls::try_start_https(
            garden_common::constants::MOSS_HTTPS,
            &state.stone_name,
            router::configure(state.clone()),
            console,
            state.shutdown_token.clone(),
        )
        .await;

        if handle.is_none() {
            state
                .https_started
                .store(false, std::sync::atomic::Ordering::Relaxed);
            tracing::warn!("HTTPS listener not started (certs may not be ready)");
        } else {
            tracing::info!(
                port = garden_common::constants::MOSS_HTTPS,
                "HTTPS listener started (pond security)"
            );
        }
    }
}
