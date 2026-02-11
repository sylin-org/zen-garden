// ============================================================================
// Linux mDNS — uses mdns-sd crate directly
// ============================================================================

/// mDNS service handle for re-registration on IP/MAC changes
#[cfg(not(target_os = "windows"))]
pub struct MdnsHandle {
    daemon: mdns_sd::ServiceDaemon,
    stone_id: Option<String>,
    stone_name: String,
    port: u16,
    /// Moss version string (static for process lifetime)
    version: String,
    /// Current health status (updated on transitions, guarded by RwLock)
    health: std::sync::RwLock<String>,
    /// Whether we've registered the service (guards against advertising bad IPs)
    registered: std::sync::atomic::AtomicBool,
}

#[cfg(not(target_os = "windows"))]
impl MdnsHandle {
    /// Register or re-register the mDNS service with an explicit IP address
    ///
    /// Called when:
    /// - Initial registration (if IP was valid at startup)
    /// - IP/MAC changes (to update resolution info)
    ///
    /// The IP address is passed explicitly rather than relying on mdns-sd's
    /// auto-detection ("0.0.0.0"), which does not reliably populate the A
    /// record on all platforms — resulting in the service type appearing in
    /// browse but instances never resolving.
    ///
    /// Safe to call multiple times - mdns_sd handles dedup internally.
    #[allow(clippy::unused_async)]
    pub async fn reregister(&self, ip: &str, mac: Option<&str>) -> anyhow::Result<()> {
        use mdns_sd::ServiceInfo;
        use std::collections::HashMap;

        let service_type = garden_common::constants::MDNS_SERVICE_TYPE_LOCAL;
        let host_name = format!("{}.local.", self.stone_name);

        // Build TXT record properties
        let mut properties: HashMap<String, String> = HashMap::new();
        if let Some(id) = &self.stone_id {
            properties.insert("stone_id".to_string(), id.clone());
        }
        properties.insert("stone_name".to_string(), self.stone_name.clone());
        properties.insert("version".to_string(), self.version.clone());
        properties.insert("api_port".to_string(), self.port.to_string());
        {
            let health = self.health.read().unwrap_or_else(|e| e.into_inner());
            properties.insert("health".to_string(), health.clone());
        }
        if let Some(mac_addr) = mac {
            properties.insert("mac".to_string(), mac_addr.to_string());
        }

        let service = ServiceInfo::new(
            service_type,
            &self.stone_name,
            &host_name,
            ip,
            self.port,
            properties,
        )?;

        self.daemon.register(service)?;

        let was_registered = self
            .registered
            .swap(true, std::sync::atomic::Ordering::SeqCst);
        if was_registered {
            tracing::info!(
                stone_name = %self.stone_name,
                ip = %ip,
                mac = ?mac,
                "mDNS service re-registered after resolution change"
            );
        } else {
            tracing::info!(
                stone_name = %self.stone_name,
                ip = %ip,
                mac = ?mac,
                "mDNS service registered"
            );
        }

        Ok(())
    }

    /// Check if service is currently registered
    pub fn is_registered(&self) -> bool {
        self.registered.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Human-readable backend description for console output
    pub fn status_label(&self) -> &'static str {
        "mdns-sd (native)"
    }

    /// Update health status and re-register mDNS TXT record
    ///
    /// Called when stone health transitions (e.g. thriving → withering).
    /// Only re-registers if the service is currently registered (has a valid IP).
    pub async fn update_health(&self, new_health: &str) {
        {
            let mut health = self.health.write().unwrap_or_else(|e| e.into_inner());
            *health = new_health.to_string();
        }

        if !self.is_registered() {
            tracing::debug!(
                health = %new_health,
                "mDNS health updated (deferred - not yet registered)"
            );
            return;
        }

        // Re-register with updated TXT. We need the current IP, but mdns-sd
        // handles dedup internally — re-registering with the same instance name
        // updates the TXT record in-place.
        // We use "0.0.0.0" as a sentinel which mdns-sd resolves to the current IP.
        // However, since we pin IPs explicitly, we need to get the current one.
        // The caller (AppState) should call reregister() with the actual IP if needed.
        // For health-only updates, we re-register with the hostname trick —
        // mdns-sd will update the TXT record for the existing registration.
        let (current_ip, current_mac) = garden_common::infra::network::get_local_ip_and_mac();
        if current_ip == "127.0.0.1" || current_ip.is_empty() {
            tracing::debug!("mDNS health update skipped - no valid IP");
            return;
        }

        if let Err(e) = self.reregister(&current_ip, current_mac.as_deref()).await {
            tracing::warn!(error = ?e, health = %new_health, "Failed to re-register mDNS after health change");
        } else {
            tracing::info!(health = %new_health, "mDNS TXT record updated with new health status");
        }
    }
}

/// Create mDNS handle, optionally registering immediately
///
/// If `current_ip` is a loopback address, the service daemon is created but
/// registration is deferred until a valid IP is available (via `reregister()`).
/// This prevents advertising bad resolution info to the network.
#[cfg(not(target_os = "windows"))]
#[allow(clippy::unused_async)]
pub async fn announce_moss(
    stone_id: Option<&str>,
    stone_name: &str,
    port: u16,
    mac: Option<&str>,
    current_ip: &str,
    version: &str,
) -> anyhow::Result<MdnsHandle> {
    use mdns_sd::ServiceDaemon;

    let mdns = ServiceDaemon::new()?;

    let handle = MdnsHandle {
        daemon: mdns,
        stone_id: stone_id.map(|s| s.to_string()),
        stone_name: stone_name.to_string(),
        port,
        version: version.to_string(),
        health: std::sync::RwLock::new("healthy".to_string()),
        registered: std::sync::atomic::AtomicBool::new(false),
    };

    // Gate: Don't advertise if we have a loopback IP
    if current_ip == "127.0.0.1" || current_ip.is_empty() {
        tracing::warn!(
            stone_name = %stone_name,
            current_ip = %current_ip,
            "mDNS registration deferred - detected loopback/invalid IP"
        );
        // Return handle without registering - will register on valid IP
        return Ok(handle);
    }

    // Valid IP - register immediately with the actual address
    handle.reregister(current_ip, mac).await?;

    Ok(handle)
}

// ============================================================================
// Windows mDNS — delegates to Koi mDNS proxy via HTTP/SSE
// ============================================================================

/// Koi-backed mDNS handle for Windows
///
/// If Koi is available at boot, provides full mDNS feature parity with Linux:
/// - Service registration with IP pinning
/// - Heartbeat-based lease renewal
/// - Re-registration on IP/MAC changes
/// - Automatic unregistration on shutdown
///
/// If Koi is unavailable, all operations are no-ops (zero regression from current behavior).
#[cfg(target_os = "windows")]
pub struct MdnsHandle {
    /// Koi client (None = Koi not available, degraded to no-op mode)
    koi: Option<std::sync::Arc<garden_common::infra::koi_client::KoiClient>>,
    /// Current registration ID (shared with heartbeat task)
    reg_id: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    /// Stone metadata for re-registration
    stone_id: Option<String>,
    stone_name: String,
    port: u16,
    /// Moss version string (static for process lifetime)
    version: String,
    /// Current health status (updated on transitions, guarded by RwLock)
    health: std::sync::Arc<std::sync::RwLock<String>>,
    /// Shutdown signal for heartbeat task
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
}

#[cfg(target_os = "windows")]
impl MdnsHandle {
    /// Re-register mDNS service with updated IP/MAC via Koi
    ///
    /// Performs DELETE of old registration + POST of new one.
    /// No-op if Koi is not available.
    pub async fn reregister(&self, ip: &str, mac: Option<&str>) -> anyhow::Result<()> {
        let koi = match &self.koi {
            Some(k) => k,
            None => return Ok(()), // No-op if Koi not available
        };

        // Unregister old (best-effort, ignore errors)
        let old_id = self.reg_id.read().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(old_id) = old_id {
            let _ = koi.unregister(&old_id).await;
        }

        // Register with updated IP
        let health = self.health.read().unwrap_or_else(|e| e.into_inner()).clone();
        let txt = garden_common::infra::koi_client::build_txt_properties(
            self.stone_id.as_deref(),
            &self.stone_name,
            mac,
            &self.version,
            &health,
            self.port,
        );

        match koi
            .register(
                &self.stone_name,
                garden_common::constants::MDNS_SERVICE_TYPE,
                self.port,
                ip,
                txt,
                garden_common::infra::koi_client::KoiClient::registration_lease_secs(),
            )
            .await
        {
            Ok(new_id) => {
                *self.reg_id.write().unwrap_or_else(|e| e.into_inner()) = Some(new_id);
                tracing::info!(
                    stone_name = %self.stone_name,
                    ip = %ip,
                    mac = ?mac,
                    "mDNS service re-registered via Koi"
                );
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to re-register mDNS via Koi");
            }
        }

        Ok(())
    }

    /// Check if service is currently registered with Koi
    pub fn is_registered(&self) -> bool {
        self.koi.is_some() && self.reg_id.read().unwrap_or_else(|e| e.into_inner()).is_some()
    }

    /// Human-readable backend description for console output
    pub fn status_label(&self) -> &str {
        if self.koi.is_some() {
            "Koi mDNS proxy"
        } else {
            "unavailable (Koi not running)"
        }
    }

    /// Update health status and re-register mDNS TXT record via Koi
    ///
    /// Called when stone health transitions (e.g. thriving → withering).
    /// No-op if Koi is not available or service is not registered.
    pub async fn update_health(&self, new_health: &str) {
        {
            let mut health = self.health.write().unwrap_or_else(|e| e.into_inner());
            *health = new_health.to_string();
        }

        if !self.is_registered() {
            tracing::debug!(
                health = %new_health,
                "mDNS health updated (deferred - not yet registered)"
            );
            return;
        }

        let (current_ip, current_mac) = garden_common::infra::network::get_local_ip_and_mac();
        if current_ip == "127.0.0.1" || current_ip.is_empty() {
            tracing::debug!("mDNS health update skipped - no valid IP");
            return;
        }

        if let Err(e) = self.reregister(&current_ip, current_mac.as_deref()).await {
            tracing::warn!(error = ?e, health = %new_health, "Failed to re-register mDNS via Koi after health change");
        } else {
            tracing::info!(health = %new_health, "mDNS TXT record updated with new health status via Koi");
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for MdnsHandle {
    fn drop(&mut self) {
        // Signal heartbeat task to stop
        if let Some(ref tx) = self.shutdown_tx {
            let _ = tx.send(true);
        }

        // Best-effort unregister (fire-and-forget, lease backup expires in 120s)
        if let Some(ref koi) = self.koi {
            if let Some(id) = self.reg_id.read().unwrap_or_else(|e| e.into_inner()).clone() {
                let client = koi.clone();
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        let _ = client.unregister(&id).await;
                    });
                }
            }
        }
    }
}

/// Create Koi-backed mDNS handle for Windows
///
/// Probes Koi health on localhost. If available, registers the service and
/// spawns a heartbeat task for lease renewal. If unavailable, returns a
/// no-op handle (identical behavior to the previous Windows stub).
#[cfg(target_os = "windows")]
pub async fn announce_moss(
    stone_id: Option<&str>,
    stone_name: &str,
    port: u16,
    mac: Option<&str>,
    current_ip: &str,
    version: &str,
) -> anyhow::Result<MdnsHandle> {
    use garden_common::infra::koi_client::{self, KoiClient};
    use std::sync::{Arc, RwLock};

    let koi = KoiClient::try_connect().await;
    let initial_health = "healthy".to_string();

    match koi {
        None => {
            tracing::debug!("Koi mDNS proxy not available, mDNS features disabled on Windows");
            Ok(MdnsHandle {
                koi: None,
                reg_id: Arc::new(RwLock::new(None)),
                stone_id: stone_id.map(|s| s.to_string()),
                stone_name: stone_name.to_string(),
                port,
                version: version.to_string(),
                health: Arc::new(RwLock::new(initial_health)),
                shutdown_tx: None,
            })
        }
        Some(client) => {
            let client = Arc::new(client);
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

            // Register if IP is valid (defer if loopback)
            let reg_id = if current_ip != "127.0.0.1" && !current_ip.is_empty() {
                let txt = koi_client::build_txt_properties(stone_id, stone_name, mac, version, &initial_health, port);

                match client
                    .register(
                        stone_name,
                        garden_common::constants::MDNS_SERVICE_TYPE,
                        port,
                        current_ip,
                        txt,
                        KoiClient::registration_lease_secs(),
                    )
                    .await
                {
                    Ok(id) => {
                        tracing::info!(
                            stone_name = %stone_name,
                            ip = %current_ip,
                            mac = ?mac,
                            "mDNS service registered via Koi"
                        );
                        Some(id)
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "Failed to register mDNS via Koi");
                        None
                    }
                }
            } else {
                tracing::warn!(
                    stone_name = %stone_name,
                    current_ip = %current_ip,
                    "mDNS registration deferred via Koi - loopback/invalid IP"
                );
                None
            };

            let reg_id_lock = Arc::new(RwLock::new(reg_id));

            let health = Arc::new(RwLock::new(initial_health));

            // Spawn heartbeat task for lease renewal
            tokio::spawn(koi_heartbeat_loop(
                client.clone(),
                reg_id_lock.clone(),
                shutdown_rx,
                stone_name.to_string(),
                stone_id.map(|s| s.to_string()),
                port,
                mac.map(|s| s.to_string()),
                version.to_string(),
                health.clone(),
            ));

            Ok(MdnsHandle {
                koi: Some(client),
                reg_id: reg_id_lock,
                stone_id: stone_id.map(|s| s.to_string()),
                stone_name: stone_name.to_string(),
                port,
                version: version.to_string(),
                health,
                shutdown_tx: Some(shutdown_tx),
            })
        }
    }
}

/// Heartbeat loop — renews Koi registration, re-registers on expiry
#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
async fn koi_heartbeat_loop(
    koi: std::sync::Arc<garden_common::infra::koi_client::KoiClient>,
    reg_id: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    stone_name: String,
    stone_id: Option<String>,
    port: u16,
    mac: Option<String>,
    version: String,
    health: std::sync::Arc<std::sync::RwLock<String>>,
) {
    use garden_common::infra::koi_client::{self, KoiClient};

    loop {
        tokio::select! {
            _ = tokio::time::sleep(KoiClient::heartbeat_interval()) => {},
            _ = shutdown_rx.changed() => {
                tracing::debug!("Koi heartbeat task shutting down");
                return;
            }
        }

        let current_id = reg_id.read().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(id) = current_id else { continue };

        match koi.heartbeat(&id).await {
            Ok(true) => {
                tracing::trace!("Koi heartbeat renewed");
            }
            Ok(false) => {
                // 404 — registration expired, re-register
                tracing::warn!("Koi registration expired (heartbeat 404), re-registering");

                let (ip, mac_fresh) = garden_common::infra::network::get_local_ip_and_mac();

                // Don't re-register with loopback (network might be down)
                if ip == "127.0.0.1" || ip.is_empty() {
                    tracing::warn!("Koi re-registration skipped - no valid IP available");
                    *reg_id.write().unwrap_or_else(|e| e.into_inner()) = None;
                    continue;
                }

                let current_health = health.read().unwrap_or_else(|e| e.into_inner()).clone();
                let txt = koi_client::build_txt_properties(
                    stone_id.as_deref(),
                    &stone_name,
                    mac_fresh.as_deref().or(mac.as_deref()),
                    &version,
                    &current_health,
                    port,
                );

                match koi
                    .register(
                        &stone_name,
                        garden_common::constants::MDNS_SERVICE_TYPE,
                        port,
                        &ip,
                        txt,
                        KoiClient::registration_lease_secs(),
                    )
                    .await
                {
                    Ok(new_id) => {
                        *reg_id.write().unwrap_or_else(|e| e.into_inner()) = Some(new_id);
                        tracing::info!("Koi re-registration successful after heartbeat 404");
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "Koi re-registration failed");
                        *reg_id.write().unwrap_or_else(|e| e.into_inner()) = None;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Koi heartbeat connection error (will retry)");
            }
        }
    }
}

// ============================================================================
// Shared types
// ============================================================================

use garden_common::infra::koi_client::DiscoveredStone;

// ============================================================================
// Linux lurk-listener — uses mdns-sd browse loop
// ============================================================================

/// Start mDNS lurk-listener for passive topology discovery
///
/// Returns a broadcast receiver for discovered stones. The listener runs
/// in the background and emits events when neighbor stones are discovered
/// via mDNS announcements.
///
/// This enables immediate topology awareness on startup - stones appear
/// in the hot-cache before any active UDP discovery requests.
#[cfg(not(target_os = "windows"))]
#[allow(clippy::unused_async)]
pub async fn start_mdns_lurk_listener(
    self_stone_name: String,
) -> anyhow::Result<tokio::sync::broadcast::Receiver<DiscoveredStone>> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    use tokio::sync::broadcast;

    let (tx, rx) = broadcast::channel::<DiscoveredStone>(32);

    let listener_tx = tx.clone();
    std::thread::spawn(move || {
        let mdns = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                tracing::warn!(error = ?e, "mDNS lurk-listener: Failed to create daemon");
                return;
            }
        };

        let receiver = match mdns.browse(garden_common::constants::MDNS_SERVICE_TYPE_LOCAL) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = ?e, "mDNS lurk-listener: Failed to browse");
                return;
            }
        };

        tracing::info!("mDNS lurk-listener started (passive topology discovery)");

        loop {
            match receiver.recv() {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    if let Some(discovered) =
                        garden_common::infra::koi_client::extract_stone_from_service_info(&info)
                    {
                        // Skip self-announcements
                        if discovered.stone_name == self_stone_name {
                            continue;
                        }

                        tracing::info!(
                            stone_name = %discovered.stone_name,
                            endpoint = %discovered.endpoint,
                            "mDNS lurk-listener: Discovered neighbor stone"
                        );

                        let _ = listener_tx.send(discovered);
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    tracing::debug!(service = %fullname, "mDNS lurk-listener: Service removed");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(error = ?e, "mDNS lurk-listener: recv error");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    Ok(rx)
}

// ============================================================================
// Windows lurk-listener — uses Koi SSE events stream via common
// ============================================================================

/// Start mDNS lurk-listener via Koi SSE events stream.
///
/// Delegates to `garden_common::infra::koi_client::run_koi_discovery_loop`
/// for SSE parsing, chunk buffering, and reconnection. Self-filtering is
/// the caller's responsibility (see `run.rs` consumer loop).
#[cfg(target_os = "windows")]
pub async fn start_mdns_lurk_listener(
    _self_stone_name: String,
) -> anyhow::Result<tokio::sync::broadcast::Receiver<DiscoveredStone>> {
    use garden_common::infra::koi_client::KoiClient;
    use tokio::sync::broadcast;

    let (tx, rx) = broadcast::channel::<DiscoveredStone>(32);

    if let Some(client) = KoiClient::try_connect().await {
        let client = std::sync::Arc::new(client);
        tokio::spawn(garden_common::infra::koi_client::run_koi_discovery_loop(
            client,
            garden_common::constants::MDNS_SERVICE_TYPE,
            tx,
        ));
    } else {
        tracing::debug!("Koi not available, mDNS lurk-listener disabled on Windows");
    }

    Ok(rx)
}
