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

        let service_type = "_moss._tcp.local.";
        let host_name = format!("{}.local.", self.stone_name);

        // Build TXT record properties
        let mut properties: HashMap<String, String> = HashMap::new();
        if let Some(id) = &self.stone_id {
            properties.insert("stone_id".to_string(), id.clone());
        }
        properties.insert("stone_name".to_string(), self.stone_name.clone());
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
) -> anyhow::Result<MdnsHandle> {
    use mdns_sd::ServiceDaemon;

    let mdns = ServiceDaemon::new()?;

    let handle = MdnsHandle {
        daemon: mdns,
        stone_id: stone_id.map(|s| s.to_string()),
        stone_name: stone_name.to_string(),
        port,
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
    koi: Option<std::sync::Arc<crate::infra::koi_client::KoiClient>>,
    /// Current registration ID (shared with heartbeat task)
    reg_id: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    /// Stone metadata for re-registration
    stone_id: Option<String>,
    stone_name: String,
    port: u16,
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
        let old_id = self.reg_id.read().unwrap().clone();
        if let Some(old_id) = old_id {
            let _ = koi.unregister(&old_id).await;
        }

        // Register with updated IP
        let txt = crate::infra::koi_client::build_txt_properties(
            self.stone_id.as_deref(),
            &self.stone_name,
            mac,
        );

        match koi
            .register(
                &self.stone_name,
                "_moss._tcp",
                self.port,
                ip,
                txt,
                crate::infra::koi_client::KoiClient::registration_lease_secs(),
            )
            .await
        {
            Ok(new_id) => {
                *self.reg_id.write().unwrap() = Some(new_id);
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
        self.koi.is_some() && self.reg_id.read().unwrap().is_some()
    }

    /// Human-readable backend description for console output
    pub fn status_label(&self) -> &str {
        if self.koi.is_some() {
            "Koi mDNS proxy"
        } else {
            "unavailable (Koi not running)"
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
            if let Some(id) = self.reg_id.read().unwrap().clone() {
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
) -> anyhow::Result<MdnsHandle> {
    use crate::infra::koi_client::{self, KoiClient};
    use std::sync::{Arc, RwLock};

    let koi = KoiClient::try_connect().await;

    match koi {
        None => {
            tracing::debug!("Koi mDNS proxy not available, mDNS features disabled on Windows");
            Ok(MdnsHandle {
                koi: None,
                reg_id: Arc::new(RwLock::new(None)),
                stone_id: stone_id.map(|s| s.to_string()),
                stone_name: stone_name.to_string(),
                port,
                shutdown_tx: None,
            })
        }
        Some(client) => {
            let client = Arc::new(client);
            let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

            // Register if IP is valid (defer if loopback)
            let reg_id = if current_ip != "127.0.0.1" && !current_ip.is_empty() {
                let txt = koi_client::build_txt_properties(stone_id, stone_name, mac);

                match client
                    .register(
                        stone_name,
                        "_moss._tcp",
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

            // Spawn heartbeat task for lease renewal
            tokio::spawn(koi_heartbeat_loop(
                client.clone(),
                reg_id_lock.clone(),
                shutdown_rx,
                stone_name.to_string(),
                stone_id.map(|s| s.to_string()),
                port,
                mac.map(|s| s.to_string()),
            ));

            Ok(MdnsHandle {
                koi: Some(client),
                reg_id: reg_id_lock,
                stone_id: stone_id.map(|s| s.to_string()),
                stone_name: stone_name.to_string(),
                port,
                shutdown_tx: Some(shutdown_tx),
            })
        }
    }
}

/// Heartbeat loop — renews Koi registration, re-registers on expiry
#[cfg(target_os = "windows")]
async fn koi_heartbeat_loop(
    koi: std::sync::Arc<crate::infra::koi_client::KoiClient>,
    reg_id: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    stone_name: String,
    stone_id: Option<String>,
    port: u16,
    mac: Option<String>,
) {
    use crate::infra::koi_client::{self, KoiClient};

    loop {
        tokio::select! {
            _ = tokio::time::sleep(KoiClient::heartbeat_interval()) => {},
            _ = shutdown_rx.changed() => {
                tracing::debug!("Koi heartbeat task shutting down");
                return;
            }
        }

        let current_id = reg_id.read().unwrap().clone();
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
                    *reg_id.write().unwrap() = None;
                    continue;
                }

                let txt = koi_client::build_txt_properties(
                    stone_id.as_deref(),
                    &stone_name,
                    mac_fresh.as_deref().or(mac.as_deref()),
                );

                match koi
                    .register(
                        &stone_name,
                        "_moss._tcp",
                        port,
                        &ip,
                        txt,
                        KoiClient::registration_lease_secs(),
                    )
                    .await
                {
                    Ok(new_id) => {
                        *reg_id.write().unwrap() = Some(new_id);
                        tracing::info!("Koi re-registration successful after heartbeat 404");
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "Koi re-registration failed");
                        *reg_id.write().unwrap() = None;
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

/// Discovered stone from mDNS
#[derive(Debug, Clone)]
pub struct MdnsDiscoveredStone {
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub endpoint: String,
    /// MAC address for Wake-on-LAN support
    pub mac: Option<String>,
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}

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
) -> anyhow::Result<tokio::sync::broadcast::Receiver<MdnsDiscoveredStone>> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    use tokio::sync::broadcast;

    let (tx, rx) = broadcast::channel::<MdnsDiscoveredStone>(32);

    // Spawn background listener
    let listener_tx = tx.clone();
    std::thread::spawn(move || {
        let mdns = match ServiceDaemon::new() {
            Ok(daemon) => daemon,
            Err(e) => {
                tracing::warn!(error = ?e, "mDNS lurk-listener: Failed to create daemon");
                return;
            }
        };

        let service_type = "_moss._tcp.local.";
        let receiver = match mdns.browse(service_type) {
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
                    // Extract stone_id from TXT records
                    let stone_id: Option<String> = info
                        .get_properties()
                        .iter()
                        .find(|p| p.key() == "stone_id")
                        .map(|p| p.val_str().to_string());

                    // Extract stone_name from TXT record, or fall back to instance name
                    let stone_name: String = info
                        .get_properties()
                        .iter()
                        .find(|p| p.key() == "stone_name")
                        .map(|p| p.val_str().to_string())
                        .unwrap_or_else(|| {
                            info.get_fullname()
                                .split('.')
                                .next()
                                .unwrap_or("unknown")
                                .to_string()
                        });

                    // Extract MAC address for WoL support
                    let mac: Option<String> = info
                        .get_properties()
                        .iter()
                        .find(|p| p.key() == "mac")
                        .map(|p| p.val_str().to_string());

                    // Skip self-announcements
                    if stone_name == self_stone_name {
                        continue;
                    }

                    if let Some(ip) = info.get_addresses().iter().next() {
                        let endpoint = format!("http://{}:{}", ip, info.get_port());

                        let discovered = MdnsDiscoveredStone {
                            stone_id: stone_id.clone(),
                            stone_name: stone_name.clone(),
                            endpoint: endpoint.clone(),
                            mac: mac.clone(),
                            discovered_at: chrono::Utc::now(),
                        };

                        tracing::info!(
                            stone_id = ?stone_id,
                            stone_name = %stone_name,
                            endpoint = %endpoint,
                            mac = ?mac,
                            "mDNS lurk-listener: Discovered neighbor stone"
                        );

                        // Send to subscribers (ignore if no subscribers)
                        let _ = listener_tx.send(discovered);
                    }
                }
                Ok(ServiceEvent::ServiceRemoved(_, fullname)) => {
                    tracing::debug!(service = %fullname, "mDNS lurk-listener: Service removed");
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!(error = ?e, "mDNS lurk-listener: recv error");
                    // Small delay before retrying
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    Ok(rx)
}

// ============================================================================
// Windows lurk-listener — uses Koi SSE events stream
// ============================================================================

/// Start mDNS lurk-listener via Koi SSE events stream
///
/// Connects to Koi's `/v1/events?type=_moss._tcp&idle_for=0` SSE endpoint.
/// Parses resolved events and feeds `MdnsDiscoveredStone` into the broadcast
/// channel — same interface as the Linux mdns-sd listener.
///
/// Includes automatic reconnection with exponential backoff (1s -> 30s).
/// If Koi is unavailable, returns a dummy receiver (same as previous stub).
#[cfg(target_os = "windows")]
pub async fn start_mdns_lurk_listener(
    self_stone_name: String,
) -> anyhow::Result<tokio::sync::broadcast::Receiver<MdnsDiscoveredStone>> {
    use crate::infra::koi_client::KoiClient;
    use tokio::sync::broadcast;

    let (tx, rx) = broadcast::channel::<MdnsDiscoveredStone>(32);

    if let Some(client) = KoiClient::try_connect().await {
        let client = std::sync::Arc::new(client);
        tokio::spawn(koi_lurk_loop(client, tx, self_stone_name));
    } else {
        tracing::debug!("Koi not available, mDNS lurk-listener disabled on Windows");
        // tx is dropped here — rx.recv() will return Closed, which run.rs handles
    }

    Ok(rx)
}

/// SSE lurk loop — connects to Koi events stream with automatic reconnection
#[cfg(target_os = "windows")]
async fn koi_lurk_loop(
    koi: std::sync::Arc<crate::infra::koi_client::KoiClient>,
    tx: tokio::sync::broadcast::Sender<MdnsDiscoveredStone>,
    self_stone_name: String,
) {
    use crate::infra::koi_client::KoiClient;

    let mut backoff = std::time::Duration::from_secs(1);
    let max_backoff = KoiClient::max_reconnect_backoff();

    tracing::info!("Koi mDNS lurk-listener started (passive topology discovery via SSE)");

    loop {
        match koi_stream_events(&koi, &tx, &self_stone_name).await {
            Ok(()) => {
                // Clean disconnect — reconnect quickly
                backoff = std::time::Duration::from_secs(1);
                tracing::debug!("Koi SSE stream ended, reconnecting");
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    backoff_secs = backoff.as_secs(),
                    "Koi SSE stream error, will reconnect"
                );
            }
        }

        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(max_backoff);
    }
}

/// Stream SSE events from Koi and feed discovered stones into broadcast channel
#[cfg(target_os = "windows")]
async fn koi_stream_events(
    koi: &crate::infra::koi_client::KoiClient,
    tx: &tokio::sync::broadcast::Sender<MdnsDiscoveredStone>,
    self_stone_name: &str,
) -> anyhow::Result<()> {
    use crate::infra::koi_client;

    let mut resp = koi.open_events_stream("_moss._tcp").await?;

    tracing::debug!(base_url = %koi.base_url(), "Connected to Koi SSE events stream");

    let mut buffer = String::new();

    while let Some(chunk) = resp.chunk().await? {
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete events (delimited by blank line)
        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            if let Some(discovered) =
                koi_client::parse_sse_event(&event_block, self_stone_name)
            {
                let _ = tx.send(discovered);
            }
        }
    }

    Ok(())
}
