//! Main daemon orchestration
//!
//! Coordinates all startup phases and background tasks.
//! Extracted from main.rs for cleaner separation of concerns.

use super::config::DaemonConfig;
#[cfg(target_os = "linux")]
use crate::run_first_boot_initialization;
use crate::tasks::discovery::start_discovery_listener;
use crate::{
    Moss,
    DockerConfig,
    // Docker monitoring
    DockerMonitor,
    DockerMonitorConfig,
    // Network monitoring
    Network,
    NetworkConfig,
    ServerConfig,
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
    version_string,
};
use garden_common::console;
use garden_common::offerings::OfferingFqn;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Bootstrap artifacts
// ============================================================================

/// Values produced by `build_state` that cannot live in `Moss` and are
/// consumed exactly once by the task supervisor or the HTTP server.
pub(crate) struct BuildArtifacts {
    /// Receive-end of the volume-rescan channel; consumed once by the volume watcher.
    pub volume_rescan_rx: tokio::sync::mpsc::Receiver<()>,
    /// True when `ZG_STONE_HOST` is set; gates the IP-change handler variant.
    pub use_static_host: bool,
    /// Resolved HTTP endpoint for this stone, e.g. `"http://192.168.1.100:7185"`.
    pub api_endpoint: String,
}

// ============================================================================
// Entry point
// ============================================================================

/// Run the Moss daemon.
///
/// Three-stage orchestration:
/// 1. `build_state` -- sequential init, builds `Moss`, fallible
/// 2. `start_background_tasks` -- spawns all concurrent workers
/// 3. `serve` -- binds HTTP and blocks until shutdown
pub async fn run(
    config: DaemonConfig,
    log: tokio::sync::broadcast::Sender<String>,
) -> anyhow::Result<()> {
    let file_config = config.file_config.clone();
    let (state, artifacts) = build_state(config, log).await?;

    // Write MOTD on every startup with whatever is known at this point.
    // Hardware may not be fully detected yet (that happens in background), so
    // cpu/ram/gpu may be None -- the MOTD writer handles that gracefully.
    // The hardware detection task will overwrite with full info once it completes.
    #[cfg(target_os = "linux")]
    {
        use garden_common::console::{BankSummary, MotdInfo, StorageSetSummary, write_motd};
        use garden_common::storage::DEFAULT_REPLICA_SET_DISPLAY;

        let caps = state.current.capabilities.read().await.clone();
        let stone_name = state.current.stone.name.clone();
        let ip = state.current.address.read().await.ip_str();
        let port = state.current.api_port;
        let version = version_string();
        let pond_name = state.security.pond_name().await;

        let (cpu_cores, ram_mb, gpu) = match &caps {
            Some(c) => {
                let cores = Some(c.hardware.cpu.cores);
                let ram = Some(c.hardware.memory.total_mb);
                let first_gpu = c
                    .hardware
                    .gpus
                    .first()
                    .map(|g| (g.model.clone(), g.vram_mb));
                (cores, ram, first_gpu)
            }
            None => (None, None, None),
        };

        let storage_sets = {
            let volumes = state.current.storage.volumes.read().await;
            let mut sets: std::collections::BTreeMap<String, Vec<BankSummary>> =
                std::collections::BTreeMap::new();
            for volume in volumes.values() {
                if *volume.state() != crate::domain::storage::VolumeState::Online {
                    continue;
                }
                if let Some(mgmt) = volume.management() {
                    let set_name = if mgmt.replica_set_name.is_empty() {
                        DEFAULT_REPLICA_SET_DISPLAY.to_string()
                    } else {
                        mgmt.replica_set_name.clone()
                    };
                    sets.entry(set_name).or_default().push(BankSummary {
                        name: mgmt.name.clone(),
                        used_bytes: volume.used_bytes(),
                        capacity_bytes: volume.capacity_bytes(),
                    });
                }
            }
            sets.into_iter()
                .map(|(replica_set_name, banks)| StorageSetSummary {
                    replica_set_name,
                    banks,
                })
                .collect::<Vec<_>>()
        };

        let info = MotdInfo {
            stone_name,
            ip,
            port,
            version,
            pond_name,
            cpu_cores,
            ram_mb,
            gpu,
            storage_sets,
        };
        if let Err(e) = write_motd(&info) {
            tracing::warn!(error = %e, "Failed to write startup MOTD");
        }
    }

    let (api_endpoint, supervisor) =
        crate::tasks::coordinator::start_background_tasks(state.clone(), artifacts, file_config)
            .await;

    // Extract supervisor handle for the /tasks API before run() consumes it
    {
        let mut guard = state.task_supervisor.write().await;
        *guard = Some(supervisor.handle());
    }

    // Run the task supervisor in the background -- it monitors all spawned tasks
    // for panics and handles clean shutdown when the cancellation token fires.
    let shutdown_token = state.shutdown_token.clone();
    tokio::spawn(supervisor.run(shutdown_token));

    // Periodic snapshot scheduler (ORCH-0039 §"Snapshot frequency").
    // Runs forever; aborts cleanly when Moss exits because the
    // tokio runtime tears down all tasks.
    let _ = crate::infra::snapshot_scheduler::spawn_periodic_snapshot_scheduler(state.clone());

    serve(state, &api_endpoint).await
}

// ============================================================================
// Stage 1: Sequential initialization
// ============================================================================

/// Build a skeleton `TopologyEntry` from the bootstrap source-of-truth fields.
///
/// Used during early boot before `Moss` exists. After Moss construction,
/// `Moss::build_self_entry()` supersedes this.
async fn build_boot_entry(
    stone_id: &str,
    stone_name: &str,
    address: &Arc<RwLock<garden_common::PeerAddress>>,
    health: &Arc<RwLock<String>>,
    mac: &Arc<RwLock<Option<String>>>,
    capabilities: Option<&Arc<RwLock<Option<garden_common::HardwareCapabilities>>>>,
) -> garden_common::TopologyEntry {
    let address = address.read().await.clone();
    let health = health.read().await.clone();
    let mac = mac.read().await.clone();
    let caps = match capabilities {
        Some(c) => c.read().await.clone(),
        None => None,
    };
    garden_common::TopologyEntry {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        address,
        moss_version: version_string(),
        services: Vec::new(),
        mac,
        health,
        capabilities: caps,
        status: garden_common::StoneStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        tags: Vec::new(),
        gateways: Vec::new(),
    }
}

/// Sequential daemon initialization.
///
/// Builds `Moss` from configuration. Strictly sequential and fallible
/// -- any error here exits the daemon cleanly before any tasks are spawned.
async fn build_state(
    config: DaemonConfig,
    log: tokio::sync::broadcast::Sender<String>,
) -> anyhow::Result<(Moss, BuildArtifacts)> {
    let stone_name = config.stone_name.clone();
    let port = config.port;

    // MOSS-0004: Create shutdown token early so all phases can receive child tokens.
    // The token is cancelled in server.rs when SIGTERM/Ctrl-C is received.
    let shutdown_token = tokio_util::sync::CancellationToken::new();

    // Phase 0: Load or generate stone_id (persistent GUID v7)
    // This must happen early as many components need it
    let stone_id = infra::load_or_generate_stone_id().await;
    tracing::info!(stone_id = %stone_id, stone_name = %stone_name, "Stone identity loaded");

    // Phase 0.1: Self-heal systemd unit file if stale (legacy migration).
    // On stones that still have the old `moss-update-helper.sh` as ExecStartPre,
    // `garden-moss pre-start` never runs. This bootstrap check regenerates the
    // unit file so the NEXT restart uses the modern pre-start binary.
    // Fast no-op once the unit file is current.
    #[cfg(target_os = "linux")]
    {
        ensure_modern_unit_file();
    }

    // Phase 0.5: Source-of-truth fields for this stone's mutable state.
    //
    // These are progressively enriched during bootstrap, then shared with
    // Moss. After construction, `build_self_entry()` reads from them
    // on demand -- no mutable self_entry cache.
    let current_address: Arc<RwLock<garden_common::PeerAddress>> =
        Arc::new(RwLock::new(garden_common::PeerAddress::new(
            std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            garden_common::constants::MOSS_HTTP,
        )));
    let current_health: Arc<RwLock<String>> = Arc::new(RwLock::new(
        garden_common::constants::STONE_STARTING.to_string(),
    ));
    let current_mac: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

    tracing::debug!("Self topology entry initialized (health=starting)");

    // Phase 1: Ensure topology directory exists before the aggregate
    // constructor attempts its first flush.
    if let Err(e) = tokio::fs::create_dir_all(garden_common::constants::paths::topology_dir()).await
    {
        tracing::warn!(error = %e, "Failed to create topology directory (will retry on first write)");
    }

    // Metrics aggregate (ARCH-0018) is constructed here -- before Tool so
    // `Tool::new` can register the `tool` domain with Metrics, and before
    // Offerings so `Offerings::new` at Phase 10.75 can do the same.
    let metrics_aggregate = Arc::new(crate::domain::Metrics::new());

    let (tool_delta, _) = tokio::sync::broadcast::channel::<garden_common::tools::ToolDelta>(
        garden_common::constants::channels::TOOL_DELTA,
    );
    let tools_transport: Arc<dyn crate::domain::tool::ToolsBeaconTransport> =
        Arc::new(crate::infra::tools::P2pBeaconTransport);
    let tool = Arc::new(
        crate::domain::Tool::new(metrics_aggregate.clone(), tool_delta, tools_transport).await,
    );

    // Topology aggregate (ARCH-0020). The aggregate owns its internal
    // cache + dirty flag; no external handles to thread through.
    let topology_aggregate: Arc<crate::domain::topology::Topology> = {
        let chirp: Arc<dyn crate::domain::topology::ChirpTransport> =
            Arc::new(crate::domain::topology::P2pChirpTransport);
        let store: Arc<dyn crate::domain::topology::TopologyStore> =
            Arc::new(crate::domain::topology::FileTopologyStore);
        Arc::new(
            crate::domain::topology::Topology::new(chirp, store, metrics_aggregate.clone()).await,
        )
    };

    // Write the initial topology file immediately (self entry only,
    // no peers yet). Don't wait for the 30s maintenance cycle --
    // containers may start before then and need the file for
    // cold-start seeding.
    let boot_entry = build_boot_entry(
        &stone_id,
        &stone_name,
        &current_address,
        &current_health,
        &current_mac,
        None,
    )
    .await;
    topology_aggregate.flush(&boot_entry).await;

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
            Box::new(crate::domain::DockerRegistry::new(std::sync::Arc::new(
                crate::infra::OsDockerConfig,
            ))),
        ]));

    // Create orchestration nudge early --" shared between discovery listener and Moss
    let orchestration_nudge = Arc::new(tokio::sync::Notify::new());

    // Unified volume collection (STORAGE-0011) --" created empty, populated after Moss
    let volumes = crate::domain::new_volumes();
    // Volume rescan channel --" API handlers poke tx, watcher loop consumes rx
    let (volume_rescan, volume_rescan_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Start UDP listener with full infrastructure handler support
    start_discovery_listener(
        stone_id.clone(),
        stone_name.clone(),
        String::new(), // Endpoint not yet known, will be set in Phase 3.5
        topology_aggregate.clone(),
        tool.clone(),
        current_address.clone(),
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
        start_first_boot_task(
            &stone_name,
            port,
            config.docker_retry_delay_secs(),
            runtime.clone(),
        );
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
    // Create Subsystems aggregate (ARCH-0023 Book VI) -- register subsystems
    // before handing the aggregate to monitor tasks.
    let mut subsystems = crate::domain::Subsystems::new(metrics_aggregate.clone()).await;
    subsystems.register("network");
    subsystems.register("docker");
    let subsystems = std::sync::Arc::new(subsystems);

    // Runs in background, polls every 5s when disconnected, 30s when connected
    // Network manages the subsystems "network" readiness
    let network = Network::start_with_config(
        NetworkConfig::default()
            .with_disconnect_retry(crate::tasks::network_monitor::DEFAULT_DISCONNECT_RETRY_SECS)
            .with_connected_poll(crate::tasks::network_monitor::DEFAULT_CONNECTED_POLL_SECS),
        subsystems.clone(),
    )
    .await;

    // Phase 2.5: Get MAC address for self entry
    let (_, mac_address) = garden_common::infra::network::get_local_ip_and_mac();

    // Phase 3: Resolve API endpoint
    // Prefer explicit STONE_HOST, otherwise use monitored network IP
    let use_static_host = std::env::var(garden_common::constants::ENV_STONE_HOST)
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

    // Phase 3.5: Update source fields with network configuration
    {
        let new_addr = garden_common::PeerAddress::new(resolved_ip, port);
        *current_address.write().await = new_addr;
        *current_mac.write().await = mac_address.clone();
        *current_health.write().await = garden_common::constants::STONE_INITIALIZING.to_string();
    }
    tracing::debug!(ip = %resolved_ip, port = port, mac = ?mac_address, "Self entry updated (health=initializing)");

    // Derive the plain HTTP endpoint string for legacy consumers
    let api_endpoint = format!("http://{}:{}", resolved_ip, port);

    // Auto-chirp: Network configuration complete
    {
        let entry = build_boot_entry(
            &stone_id,
            &stone_name,
            &current_address,
            &current_health,
            &current_mac,
            None,
        )
        .await;
        // Pre-Moss bootstrap phase: construct a local transport
        // instead of going through the Topology aggregate, which
        // doesn't exist yet at this point in the bootstrap sequence.
        let pre_state_chirp = crate::domain::topology::P2pChirpTransport;
        if let Err(e) =
            <crate::domain::topology::P2pChirpTransport as crate::domain::topology::ChirpTransport>
                ::chirp(&pre_state_chirp, &entry)
                .await
        {
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

    // Shared pond state flag --" created before mDNS so both MdnsHandle and Moss
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

        tracing::info!(
            "Koi embedded started (mDNS + certmesh + HTTP + DNS + UDP + dashboard + browser active)"
        );
        Arc::new(handle)
    };

    // Phase 4.0.1: Seed pond_active from persisted certmesh state
    // Two cases: (a) cornerstone with CA initialized + unlocked, or
    // (b) enrolled member with cert files from a prior enrollment.
    // Seeds pond_active flag; Security aggregate is seeded later at Phase 11.
    //
    // Auto-unlock now happens inside koi-embedded's init_certmesh_core(),
    // so by this point the CA is already unlocked if the key file exists.
    // We just read the status and seed the application state.
    if let Ok(cm) = koi_handle.certmesh()
        && let Ok(core) = cm.core()
    {
        let status = core.certmesh_status().await;
        if status.ca_initialized && !status.ca_locked {
            pond_active.store(true, std::sync::atomic::Ordering::Relaxed);
            tracing::info!("Pond active -- CA initialized and unlocked");

            // Register _certmesh._tcp mDNS so Rake clients can discover us
            crate::mdns::register_certmesh_service(
                &koi_handle,
                garden_common::constants::MOSS_HTTP,
            )
            .await;
        } else if status.ca_initialized {
            // CA is initialized but still locked --" no auto-unlock key
            // existed, or decryption failed.  Report available methods.
            let slot_table_path = koi_certmesh::CertmeshPaths::default().slot_table_path();
            if slot_table_path.exists() {
                if let Ok(table) = koi_crypto::unlock_slots::SlotTable::load(&slot_table_path) {
                    let methods = table.available_methods();
                    if methods.contains(&"totp") {
                        tracing::info!(
                            "Pond CA locked -- unlock with TOTP code via 'POST /api/v1/pond/unlock' or 'garden-rake pond unlock --totp'"
                        );
                    } else if methods.contains(&"fido2") {
                        tracing::info!("Pond CA locked -- unlock with security key via pond UI");
                    } else {
                        tracing::info!(
                            "Pond CA locked -- run 'garden-rake pond unlock' with passphrase"
                        );
                    }
                } else {
                    tracing::info!("Pond CA exists but is locked -- run 'garden-rake pond unlock'");
                }
            } else {
                tracing::info!("Pond CA exists but is locked -- run 'garden-rake pond unlock'");
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
            tracing::info!("Pond active -- enrolled member with certs from previous enrollment");
        }
    }

    // Phase 4.0.1b: Propagate pond state into address
    if pond_active.load(std::sync::atomic::Ordering::Relaxed) {
        let mut addr = current_address.write().await;
        *addr = addr.clone().with_tls(garden_common::constants::MOSS_HTTPS);
        tracing::debug!(
            "Self entry updated with TLS port {}",
            garden_common::constants::MOSS_HTTPS
        );
    }

    // Phase 4.0.2--"4.0.3: Chirp signing + verification
    // Deferred to Phase 18 boot (activate_pond_security) and the
    // enrollment-change listener (Phase 11.3). No duplicated code here.

    // Phase 4.1: mDNS announcement --" includes stone_id and MAC in TXT records
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
    // Note: IP change handler moved to Phase 11 and delegates to
    // crate::domain::topology::composition::announce_resolution_change

    // Phase 6: Lantern registration -- deferred to Phase 11.post2 (needs Moss for service list)

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
        tracing::warn!("could not determine Docker bridge gateway -- container DNS may not work");
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
    // DockerMonitor manages the subsystems "docker" readiness
    let _docker_monitor = DockerMonitor::start_with_config(
        docker.clone(),
        DockerMonitorConfig::default()
            .with_disconnect_retry(crate::tasks::docker::DEFAULT_DISCONNECT_RETRY_SECS)
            .with_connected_poll(crate::tasks::docker::DEFAULT_CONNECTED_POLL_SECS),
        subsystems.clone(),
    )
    .await;
    tracing::debug!(
        "Docker monitor started ({}s retry, {}s poll)",
        crate::tasks::docker::DEFAULT_DISCONNECT_RETRY_SECS,
        crate::tasks::docker::DEFAULT_CONNECTED_POLL_SECS
    );

    // Phase 8: Create domain event bus and pulse channel
    let event_bus = infra::EventBus::new();
    let (pulse, _) = tokio::sync::broadcast::channel::<infra::PulseEvent>(
        garden_common::constants::channels::PULSE,
    );
    tracing::debug!("Domain event bus and pulse channel initialized");

    // Phase 9: Capabilities loading
    let capabilities = init_capabilities(&stone_id, &stone_name, &console_printer).await;

    // Phase 9.5: Set health to thriving (capabilities are already in their Arc<RwLock>)
    *current_health.write().await = garden_common::constants::STONE_THRIVING.to_string();
    tracing::debug!("Self entry updated with capabilities (health=thriving)");

    // Auto-chirp: Capabilities complete
    {
        let entry = build_boot_entry(
            &stone_id,
            &stone_name,
            &current_address,
            &current_health,
            &current_mac,
            Some(&capabilities),
        )
        .await;
        // Pre-Moss: construct a local transport instance.
        let pre_state_chirp = crate::domain::topology::P2pChirpTransport;
        if let Err(e) =
            <crate::domain::topology::P2pChirpTransport as crate::domain::topology::ChirpTransport>
                ::chirp(&pre_state_chirp, &entry)
                .await
        {
            tracing::warn!(error = ?e, "Failed to auto-chirp after capabilities update");
        } else {
            tracing::debug!("Auto-chirp sent after capabilities detection");
        }
    }

    // Phase 10.5: Construct the Offerings aggregate (ARCH-0016) from disk.
    //
    // Loaded offerings are split into two pools by `Offerings::split_loaded`:
    //  - Managed and borrowed → active pool (Docker manages their lifecycle).
    //  - Adopted → candidates pool (must pass detection before becoming active).
    //
    // The split prevents ghost services: an adopted offering persisted from a
    // previous run only appears in topology after the auto-adoption task
    // confirms it's actually running.
    //
    // The aggregate owns both pools privately and persists through an
    // `OfferingStore` port -- every mutation is persisted and publishes an
    // `OfferingsChanged` event consumed by `OfferingsProjectionTask`.
    let offering_store: Arc<dyn crate::domain::OfferingStore> =
        Arc::new(crate::domain::FileOfferingStore);

    let (active, candidates) = match offering_store.load().await {
        Ok(all) => {
            let (active, candidates) = crate::domain::Offerings::split_loaded(all);
            let managed = active.iter().filter(|o| o.is_managed()).count();
            let borrowed = active.iter().filter(|o| o.is_borrowed()).count();
            if !active.is_empty() || !candidates.is_empty() {
                tracing::info!(
                    active = active.len(),
                    managed = managed,
                    borrowed = borrowed,
                    adopted_candidates = candidates.len(),
                    "Loaded offerings: active={}, adopted candidates={} (pending detection)",
                    active.len(),
                    candidates.len(),
                );
            }
            (active, candidates)
        }
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to load offerings, starting fresh");
            (Vec::new(), Vec::new())
        }
    };

    // Phase 10.75: The Metrics aggregate was constructed early (just before
    // Tool) so both `Tool::new` and `Offerings::new` can register their
    // domains with it from their first call onward. Here we just reuse it.

    let offerings_aggregate = Arc::new(
        crate::domain::Offerings::new(
            active,
            candidates,
            offering_store,
            metrics_aggregate.clone(),
        )
        .await,
    );

    // Phase 11: Build Moss
    // Note: manifest_registry and infrastructure_handlers already created at Phase 1
    let ceremony_registry = Arc::new(crate::domain::CeremonyRegistry::new());
    let ceremony_journal: Arc<dyn crate::domain::CeremonyPersistence + Send + Sync> =
        Arc::new(infra::CeremonyJournal::default_journal());

    // Security aggregate -- ARCH-0027 (Book IX)
    let security_aggregate = Arc::new(
        crate::domain::Security::new(
            pond_active.clone(),
            Arc::new(infra::stone_client::StoneClient::new(&stone_name)),
            Arc::new(koi_common::ceremony::CeremonyHost::new(
                koi_certmesh::pond_ceremony::PondCeremonyRules,
            )),
            ceremony_registry,
            ceremony_journal,
            metrics_aggregate.clone(),
        )
        .await,
    );

    // Seed Security aggregate with boot-time enrollment state
    {
        let enrolled = pond_active.load(std::sync::atomic::Ordering::Relaxed);
        let pond_metadata = crate::domain::load_pond_metadata();
        security_aggregate
            .seed_state(enrolled, None, pond_metadata.name)
            .await;
    }
    let harvest_store = Arc::new(infra::HarvestStore::default_store());
    let harvest_ops = Arc::new(crate::infra::harvest::OsHarvestOps::new(
        docker.clone(),
        Arc::clone(&harvest_store),
    ));
    let nurturing_store = Arc::new(infra::NurturingStore::new(
        infra::HarvestStore::default_store(),
        docker.clone(),
    ));

    // Storage and orchestration channels (ARCH-0004)
    let (storage_tick_raw, _) = tokio::sync::broadcast::channel::<
        garden_common::storage::StorageTick,
    >(garden_common::constants::channels::STORAGE_EVENT);
    let (storage_tick_debounced, _) = tokio::sync::broadcast::channel::<
        garden_common::storage::StorageTick,
    >(garden_common::constants::channels::STORAGE_EVENT);
    let (storage_changed, _) = tokio::sync::broadcast::channel::<
        garden_common::storage::StorageChanged,
    >(garden_common::constants::channels::STORAGE_EVENT);
    let nourishment_map = Arc::new(RwLock::new(HashMap::<
        String,
        tokio::sync::broadcast::Sender<String>,
    >::new()));
    let media = crate::domain::storage::new_media();

    // Phase 11.pre: Create election service (placeholder for now, will be updated after Moss)
    // Note: No longer async - no socket binding (uses p2p transport)
    let election_service_placeholder = Arc::new(crate::tasks::election_service::Elections::new(
        stone_id.clone(),
        stone_name.clone(),
        Box::new(crate::tasks::state_provider::PlaceholderStateProvider),
    ));

    // Jobs aggregate (ARCH-0021 Book IV Ch5). Ephemeral -- no
    // persistence, state starts empty and is swept periodically by
    // `JobsReaperTask` after the terminal TTL.
    let jobs = Arc::new(
        crate::domain::Jobs::with_shared_state(
            Arc::new(RwLock::new(HashMap::new())),
            metrics_aggregate.clone(),
            event_bus.clone(),
        )
        .await,
    );

    // Catalog aggregate (ARCH-0022 Book V). Persistent -- compiled
    // offerings index is cached to disk via FileCatalogCache. The
    // aggregate shares `manifest_registry` (frozen, read-only) and
    // `capabilities` with the rest of Moss.
    let catalog_aggregate = Arc::new(
        crate::domain::Catalog::new(
            manifest_registry.clone(),
            capabilities.clone(),
            Arc::new(crate::domain::FileCatalogCache),
            metrics_aggregate.clone(),
        )
        .await,
    );

    // Health aggregate (ARCH-0024 Book VII)
    let health_aggregate = Arc::new(
        crate::domain::Health::new(
            metrics_aggregate.clone(),
            Arc::new(crate::domain::DockerHealthProbe::new(docker.clone())),
        )
        .await,
    );

    let state = Moss {
        current: Arc::new(crate::domain::Current {
            stone: Arc::new(crate::domain::current::Stone {
                id: stone_id.clone(),
                name: stone_name.clone(),
            }),
            storage: Arc::new(crate::domain::Storage {
                volumes: volumes.clone(),
                media: media.clone(),
                changed: storage_changed.clone(),
                coordination: crate::domain::storage::Coordination {
                    tick: crate::domain::storage::Tick {
                        raw: storage_tick_raw.clone(),
                        debounced: storage_tick_debounced.clone(),
                    },
                    nudge: orchestration_nudge.clone(),
                    rescan: volume_rescan.clone(),
                    s3_listeners: Arc::new(crate::infra::storage::S3Listeners::new(
                        shutdown_token.clone(),
                    )),
                },
            }),
            capabilities: capabilities.clone(),
            hardware_topology: Arc::new(RwLock::new(None)),
            address: current_address.clone(),
            health: current_health.clone(),
            mac: current_mac.clone(),
            api_port: port,
            resources: Arc::new(crate::domain::current::Resources {
                system: Arc::new(RwLock::new(None)),
                network: Arc::new(RwLock::new(None)),
                gpu: Arc::new(RwLock::new(None)),
            }),
        }),
        offerings: offerings_aggregate,
        metrics: metrics_aggregate.clone(),
        catalog: catalog_aggregate,
        platform: Arc::new(crate::domain::Platform {
            container: docker.clone(),
            runtime: runtime.clone(),
            network: Arc::new(network),
            handlers: infrastructure_handlers.clone(),
        }),
        jobs,
        pulse: pulse.clone(),
        event_bus: event_bus.clone(),
        shutdown_token: shutdown_token.clone(),
        start_time: std::time::Instant::now(),
        console: console_printer.clone(),
        tool: tool.clone(),
        topology: topology_aggregate.clone(),
        discovery: Arc::new(
            crate::domain::Discovery::new(
                koi_handle.clone(),
                mdns_handle.clone(),
                None, // lurk_tx — lurk-listener is started in coordinator
                metrics_aggregate.clone(),
            )
            .await,
        ),
        security: security_aggregate.clone(),
        presence: Arc::new(crate::domain::Presence {
            elections: election_service_placeholder,
            notifications: Arc::new(garden_common::notifications::NotificationRegistry::new()),
        }),
        companion: Arc::new(crate::domain::Companion {
            registry: Arc::new(infra::CompanionRegistry::new().await),
        }),
        // Log broadcast channel (for live SSE log streaming)
        log: log.clone(),
        // Health aggregate -- ARCH-0024 (Book VII)
        health: health_aggregate,
        // Subsystem readiness -- ARCH-0023 aggregate (Book VI)
        subsystems: subsystems.clone(),
        // Nurturing + nourishment (ARCH-0029: dissolved from Orchestration).
        nurturing: Arc::new(crate::domain::NurturingOrchestration {
            harvest_ops: Arc::clone(&harvest_ops),
            store: Arc::clone(&nurturing_store),
        }),
        nourishment: Arc::new(crate::domain::NourishmentOrchestration {
            jobs: nourishment_map.clone(),
        }),

        // ARCH-0015: supervisor handle set after supervisor is built
        task_supervisor: Arc::new(RwLock::new(None)),
    };

    // Phase 11.post: Update election service with proper state provider now that Moss exists
    // Note: No longer async - no socket binding (uses p2p transport)
    let state_for_election = Arc::new(state.clone());
    let election_service_final = Arc::new(crate::tasks::election_service::Elections::new(
        stone_id.clone(),
        stone_name.clone(),
        Box::new(crate::tasks::state_provider::MossStateProvider::new(
            state_for_election,
        )),
    ));

    // Update the state's election service (presence domain re-wrap)
    let state = Moss {
        presence: Arc::new(crate::domain::Presence {
            elections: election_service_final.clone(),
            notifications: Arc::clone(&state.presence.notifications),
        }),
        ..state
    };

    tracing::info!("Election service initialized (using p2p transport)");

    // Phase 11.post1.5: Inject fitness provider for ORCH-0001 elections
    {
        let state_for_fitness = Arc::new(state.clone());
        state
            .presence
            .elections
            .set_fitness_provider(Box::new(
                crate::tasks::state_provider::MossFitnessProvider::new(state_for_fitness),
            ))
            .await;
        tracing::info!("Fitness provider injected into election service (ORCH-0001)");
    }

    Ok((
        state,
        BuildArtifacts {
            volume_rescan_rx,
            use_static_host: use_static_host.is_some(),
            api_endpoint: api_endpoint.clone(),
        },
    ))
}

// ============================================================================
// Stage 3: HTTP server
// ============================================================================

/// Bind the HTTP server and run until shutdown.
async fn serve(state: Moss, api_endpoint: &str) -> anyhow::Result<()> {
    let stone_name = state.current.stone.name.clone();
    let port = state.current.api_port;
    let shutdown_token = state.shutdown_token.clone();
    let console_printer = state.console.clone();
    let runtime = state.platform.runtime.clone();

    // Phase 18: HTTP server
    tracing::info!("Setting up HTTP router with 200 MB body limit");

    // When pond security is active, split routes across two listeners:
    // - HTTP :7185 â†’ public lobby (health, discovery, pond join/status)
    // - HTTPS :7183 â†’ all routes (authenticated, full API)
    let pond_is_active = state.security.pond_active();

    // If already enrolled at boot, activate HTTPS + chirp signing/verification
    if pond_is_active {
        activate_pond_security(&state, &console_printer).await;
    }

    let app = if pond_is_active && state.security.https_started() {
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
            let self_entry =
                crate::domain::topology::composition::build_self_entry(&goodbye_state).await;
            goodbye_state.topology.flush(&self_entry).await;

            if let Err(e) = crate::announcement::send_goodbye(&goodbye_state).await {
                tracing::warn!(error = ?e, "Failed to send goodbye announcement");
            }
        })
    });

    // Prepare boot banner info
    let current_ip = state.platform.network.get_ip().await;
    let manifests_count = state.catalog.manifest_count();
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
        api_endpoint,
        console_printer,
        runtime,
        shutdown_token,
        ServerConfig::default(),
        Some(shutdown_callback),
        boot_banner,
        shutdown_banner,
        Some(state.companion.registry.clone()),
    )
    .await
}

/// Configure systemd-resolved for Zen Garden container DNS.
///
/// 1. `DNSStubListenerExtra=<bridge_gw>` --" resolved listens on Docker bridge
///    gateway so containers can use it for DNS.
/// 2. `resolvectl dns docker0 <koi_dns>` + `resolvectl domain docker0 ~zengarden`
///    --" routes `.zengarden` queries to Koi DNS (port 5642).
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
        tracing::warn!("docker0 interface not found after 5s -- .zengarden DNS routing skipped");
        return Ok(());
    }

    let dns_output = tokio::process::Command::new("resolvectl")
        .args(["dns", "docker0", &koi_dns])
        .output()
        .await
        .context("resolvectl dns")?;
    if !dns_output.status.success() {
        let stderr = String::from_utf8_lossy(&dns_output.stderr);
        tracing::warn!(%stderr, "resolvectl dns docker0 failed -- .zengarden routing unavailable");
        return Ok(());
    }

    let domain_output = tokio::process::Command::new("resolvectl")
        .args(["domain", "docker0", "~zengarden"])
        .output()
        .await
        .context("resolvectl domain")?;
    if !domain_output.status.success() {
        let stderr = String::from_utf8_lossy(&domain_output.stderr);
        tracing::warn!(%stderr, "resolvectl domain docker0 failed -- .zengarden routing unavailable");
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
/// Runs initialization in a background task.
/// Exits process after completion so systemd restarts with new config.
#[cfg(target_os = "linux")]
fn start_first_boot_task(
    stone_name: &str,
    port: u16,
    retry_delay_secs: u64,
    runtime: std::sync::Arc<dyn garden_common::PlatformRuntime>,
) {
    tracing::info!("First run detected on Linux, spawning background initialization task");

    let init_stone_name = stone_name.to_string();
    let init_port = port;

    tokio::spawn(async move {
        const MAX_ATTEMPTS: u32 = 20;

        match run_first_boot_initialization(&*runtime, &init_stone_name, init_port).await {
            Ok(new_name) => {
                if let Err(e) = console::mark_first_run_complete().await {
                    tracing::error!(error = ?e, "Failed to mark first-run complete");
                }

                tracing::info!(new_name = %new_name, "First boot initialization completed successfully");
                runtime.write_line("");
                runtime.display_success(&format!("Stone configured as: {}", new_name));
                runtime.display_wait("Restarting to apply new configuration...");
                runtime.write_line("");

                // Exit so systemd restarts us with the new configuration
                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!(error = ?e, "First boot initialization failed");
                runtime.display_error(&format!("Setup failed: {}", e));

                // Retry loop for transient failures (e.g. network not yet up for mDNS)
                for attempt in 2..=MAX_ATTEMPTS {
                    tracing::info!(attempt, "Retrying first boot initialization");
                    tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_secs)).await;

                    match run_first_boot_initialization(&*runtime, &init_stone_name, init_port)
                        .await
                    {
                        Ok(new_name) => {
                            if let Err(e) = console::mark_first_run_complete().await {
                                tracing::error!(error = ?e, "Failed to mark first-run complete");
                            }
                            tracing::info!(new_name = %new_name, "First boot initialization completed");
                            std::process::exit(0);
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, attempt, "First boot retry failed");
                            if attempt == MAX_ATTEMPTS {
                                runtime.display_error("First boot setup failed after all retries");
                            }
                        }
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
pub(crate) async fn start_preinstall_handler(state: &Moss) {
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
                if state.catalog.get_manifest(&fqn.offering).is_none() {
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
    state
        .jobs
        .submit(job_id.clone(), "install-batch", manifest.offerings.clone())
        .await;

    // Spawn background installation + cleanup task.
    //
    // `install_batch_task` always reaches a terminal state
    // (`Completed` or `Failed`) before returning -- the per-item
    // `record_item_*` + final `complete`/`fail` calls all await
    // inline. Once the task's `.await` returns, the manifest can
    // be deleted. The previous 5-second poll loop that watched the
    // raw jobs map for terminal status was redundant.
    let install_state = state.clone();
    let install_job_id = job_id.clone();
    let install_offerings = manifest.offerings.clone();

    tokio::spawn(async move {
        install_batch_task(&install_state, &install_job_id, install_offerings).await;

        tracing::info!("Pre-install job finished, removing manifest");
        if let Err(e) = tokio::fs::remove_file("/home/stone/garden-moss-preinstall.json").await {
            tracing::warn!(error = ?e, "Failed to remove pre-install manifest");
        } else {
            tracing::info!("Pre-install manifest removed - system ready");
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
pub(crate) async fn activate_pond_security(
    state: &Moss,
    console: &garden_common::console::ConsolePrinter,
) {
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(&state.current.stone.name);
    let key_path = certs_dir.join("key.pem");
    let cert_path = certs_dir.join("cert.pem");

    // --- Chirp signing ---
    if key_path.exists()
        && cert_path.exists()
        && let Ok(key_pem) = std::fs::read_to_string(&key_path)
        && let Ok(keypair) = koi_crypto::keys::ca_keypair_from_pem(&key_pem)
    {
        use base64::Engine;
        match keypair.public_key_pem() {
            Ok(public_key_pem) => {
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
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to extract public key PEM, chirp signing disabled");
            }
        }
    }

    // --- Chirp verification ---
    let ca_cert_path = koi_certmesh::CertmeshPaths::default().ca_cert_path();
    if ca_cert_path.exists()
        && let Ok(_ca_pem) = std::fs::read_to_string(&ca_cert_path)
    {
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

                let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(sig_b64) {
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

    // --- HTTPS listener ---
    if state.security.try_set_https_started() {
        let handle = tls::try_start_https(
            garden_common::constants::MOSS_HTTPS,
            &state.current.stone.name,
            router::configure(state.clone()),
            console,
            state.shutdown_token.clone(),
        )
        .await;

        if handle.is_none() {
            state.security.clear_https_started();
            tracing::warn!("HTTPS listener not started (certs may not be ready)");
        } else {
            tracing::info!(
                port = garden_common::constants::MOSS_HTTPS,
                "HTTPS listener started (pond security)"
            );
        }
    }
}

// ── Legacy self-healing ─────────────────────────────────────────────

/// Regenerate the systemd unit file if it contains stale directives.
///
/// This bootstraps the transition from the old `moss-update-helper.sh`
/// to `garden-moss pre-start`. The daemon runs even with the old unit
/// file, so this check ensures the NEXT restart uses the modern pre-start.
/// Also removes legacy shell scripts that the pre-start would remove,
/// since pre-start may not have run yet under the old unit file.
#[cfg(target_os = "linux")]
fn ensure_modern_unit_file() {
    use crate::infra::installer::{linux, pre_start};
    use std::path::Path;

    let unit_path = Path::new(linux::UNIT_FILE_PATH);
    if !unit_path.exists() {
        return;
    }

    let current = match std::fs::read_to_string(unit_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "Could not read unit file for migration check");
            return;
        }
    };

    if !pre_start::unit_file_needs_regeneration(&current) {
        return;
    }

    let new_contents = linux::generate_unit_file();
    if let Err(e) = std::fs::write(unit_path, &new_contents) {
        tracing::error!(error = %e, "Failed to regenerate systemd unit file");
        return;
    }

    tracing::info!("Regenerated systemd unit file (legacy migration)");

    // daemon-reload so the new unit takes effect on next restart
    let _ = std::process::Command::new("systemctl")
        .args(["daemon-reload"])
        .output();

    // Also remove legacy scripts (pre-start may not have run yet)
    for path_str in linux::LEGACY_SCRIPTS {
        let path = Path::new(path_str);
        if path.exists() {
            match std::fs::remove_file(path) {
                Ok(()) => tracing::info!(path = path_str, "Removed legacy script"),
                Err(e) => {
                    tracing::warn!(path = path_str, error = %e, "Could not remove legacy script")
                }
            }
        }
    }
}
