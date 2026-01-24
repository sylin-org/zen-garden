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
    /// Register or re-register the mDNS service
    ///
    /// Called when:
    /// - Initial registration (if IP was valid at startup)
    /// - IP/MAC changes (to update resolution info)
    ///
    /// Safe to call multiple times - mdns_sd handles dedup internally.
    pub fn reregister(&self, mac: Option<&str>) -> anyhow::Result<()> {
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
            "0.0.0.0", // Let mDNS daemon auto-resolve
            self.port,
            properties,
        )?;

        self.daemon.register(service)?;

        let was_registered = self.registered.swap(true, std::sync::atomic::Ordering::SeqCst);
        if was_registered {
            tracing::info!(
                stone_name = %self.stone_name,
                mac = ?mac,
                "mDNS service re-registered after resolution change"
            );
        } else {
            tracing::info!(
                stone_name = %self.stone_name,
                mac = ?mac,
                "mDNS service registered (deferred from startup)"
            );
        }

        Ok(())
    }

    /// Check if service is currently registered
    pub fn is_registered(&self) -> bool {
        self.registered.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Create mDNS handle, optionally registering immediately
///
/// If `current_ip` is a loopback address, the service daemon is created but
/// registration is deferred until a valid IP is available (via `reregister()`).
/// This prevents advertising bad resolution info to the network.
#[cfg(not(target_os = "windows"))]
pub fn announce_moss(
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

    // Valid IP - register immediately
    handle.reregister(mac)?;

    Ok(handle)
}

/// Dummy mDNS handle for Windows (no-op)
#[cfg(target_os = "windows")]
pub struct MdnsHandle;

#[cfg(target_os = "windows")]
impl MdnsHandle {
    pub fn reregister(&self, _mac: Option<&str>) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn is_registered(&self) -> bool {
        false
    }
}

#[cfg(target_os = "windows")]
pub fn announce_moss(
    _stone_id: Option<&str>,
    _stone_name: &str,
    _port: u16,
    _mac: Option<&str>,
    _current_ip: &str,
) -> anyhow::Result<MdnsHandle> {
    tracing::debug!("mDNS not available on Windows, skipping");
    Ok(MdnsHandle)
}

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

/// Start mDNS lurk-listener for passive topology discovery
///
/// Returns a broadcast receiver for discovered stones. The listener runs
/// in the background and emits events when neighbor stones are discovered
/// via mDNS announcements.
///
/// This enables immediate topology awareness on startup - stones appear
/// in the hot-cache before any active UDP discovery requests.
#[cfg(not(target_os = "windows"))]
pub fn start_mdns_lurk_listener(
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
                    let stone_id: Option<String> = info.get_properties()
                        .iter()
                        .find(|p| p.key() == "stone_id")
                        .map(|p| p.val_str().to_string());

                    // Extract stone_name from TXT record, or fall back to instance name
                    let stone_name: String = info.get_properties()
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
                    let mac: Option<String> = info.get_properties()
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

#[cfg(target_os = "windows")]
pub fn start_mdns_lurk_listener(
    _self_stone_name: String,
) -> anyhow::Result<tokio::sync::broadcast::Receiver<MdnsDiscoveredStone>> {
    use tokio::sync::broadcast;

    tracing::debug!("mDNS lurk-listener not available on Windows");

    // Return a dummy receiver that will never receive anything
    let (_tx, rx) = broadcast::channel::<MdnsDiscoveredStone>(1);
    Ok(rx)
}
