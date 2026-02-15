//! Unified mDNS service announcement and discovery via Koi embedded
//!
//! Replaces the former platform-split implementation (mdns-sd on Linux,
//! KoiClient HTTP on Windows) with a single code path using `koi_embedded`.
//! Koi handles service registration, lease management, and browse internally.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use garden_common::constants::MDNS_SERVICE_TYPE;
use garden_common::infra::koi_client::DiscoveredStone;
use koi_embedded::KoiHandle;

/// mDNS service handle for re-registration on IP/MAC changes
///
/// Wraps a Koi embedded handle and manages the Moss service registration.
/// All platforms use the same code path - no `#[cfg]` conditionals.
pub struct MdnsHandle {
    koi: Arc<KoiHandle>,
    /// Current registration ID (returned by Koi on register)
    registration_id: std::sync::RwLock<Option<String>>,
    /// Stone metadata for re-registration
    stone_id: Option<String>,
    stone_name: String,
    port: u16,
    /// Moss version string (static for process lifetime)
    version: String,
    /// Current health status (updated on transitions)
    health: std::sync::RwLock<String>,
    /// Pond active flag — shared with AppState for TXT property updates
    pond_active: Arc<AtomicBool>,
}

impl MdnsHandle {
    /// Register or re-register the mDNS service with an explicit IP address
    ///
    /// Called when:
    /// - Initial registration (if IP was valid at startup)
    /// - IP/MAC changes (to update resolution info)
    ///
    /// Koi handles dedup and lease management internally.
    pub async fn reregister(&self, ip: &str, mac: Option<&str>) -> anyhow::Result<()> {
        let mdns = self
            .koi
            .mdns()
            .map_err(|e| anyhow::anyhow!("mDNS not available: {}", e))?;

        // Unregister old registration if present
        let old_id = self
            .registration_id
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(ref old_id) = old_id {
            let _ = mdns.unregister(old_id);
        }

        let txt = build_txt_properties(
            self.stone_id.as_deref(),
            &self.stone_name,
            mac,
            &self.version,
            &self.health.read().unwrap_or_else(|e| e.into_inner()),
            self.port,
            self.pond_active.load(Ordering::Relaxed),
        );

        let result = mdns.register(koi_embedded::RegisterPayload {
            name: self.stone_name.clone(),
            service_type: MDNS_SERVICE_TYPE.to_string(),
            port: self.port,
            ip: Some(ip.to_string()),
            lease_secs: None, // Permanent registration
            txt,
        })?;

        let was_registered = self
            .registration_id
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        *self
            .registration_id
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(result.id);

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
        self.registration_id
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Human-readable backend description for console output
    pub fn status_label(&self) -> &'static str {
        "koi-embedded"
    }

    /// Update health status and re-register mDNS TXT record
    ///
    /// Called when stone health transitions (e.g. thriving -> withering).
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

impl Drop for MdnsHandle {
    fn drop(&mut self) {
        // Best-effort unregister on shutdown
        if let Ok(mdns) = self.koi.mdns() {
            if let Some(id) = self
                .registration_id
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
            {
                let _ = mdns.unregister(&id);
            }
        }
    }
}

/// Create mDNS handle, optionally registering immediately
///
/// If `current_ip` is a loopback address, the handle is created but
/// registration is deferred until a valid IP is available (via `reregister()`).
#[allow(clippy::too_many_arguments)]
pub async fn announce_moss(
    koi_handle: Arc<KoiHandle>,
    stone_id: Option<&str>,
    stone_name: &str,
    port: u16,
    mac: Option<&str>,
    current_ip: &str,
    version: &str,
    pond_active: Arc<AtomicBool>,
) -> anyhow::Result<MdnsHandle> {
    let handle = MdnsHandle {
        koi: koi_handle,
        registration_id: std::sync::RwLock::new(None),
        stone_id: stone_id.map(|s| s.to_string()),
        stone_name: stone_name.to_string(),
        port,
        version: version.to_string(),
        health: std::sync::RwLock::new("healthy".to_string()),
        pond_active,
    };

    // Gate: Don't advertise if we have a loopback IP
    if current_ip == "127.0.0.1" || current_ip.is_empty() {
        tracing::warn!(
            stone_name = %stone_name,
            current_ip = %current_ip,
            "mDNS registration deferred - detected loopback/invalid IP"
        );
        return Ok(handle);
    }

    // Valid IP - register immediately
    handle.reregister(current_ip, mac).await?;

    Ok(handle)
}

// ============================================================================
// Discovery (lurk-listener)
// ============================================================================

/// Start mDNS lurk-listener for passive topology discovery
///
/// Uses Koi's browse API to listen for `_moss._tcp` service announcements.
/// Returns a broadcast receiver for discovered stones. Self-filtering is
/// the caller's responsibility.
pub async fn start_mdns_lurk_listener(
    koi_handle: Arc<KoiHandle>,
    self_stone_name: String,
) -> anyhow::Result<tokio::sync::broadcast::Receiver<DiscoveredStone>> {
    let (tx, rx) = tokio::sync::broadcast::channel::<DiscoveredStone>(32);

    let mdns = koi_handle
        .mdns()
        .map_err(|e| anyhow::anyhow!("mDNS not available for lurk-listener: {}", e))?;

    let browse = mdns
        .browse(MDNS_SERVICE_TYPE)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start mDNS browse for lurk-listener: {}", e))?;

    tracing::info!("mDNS lurk-listener started (passive topology discovery via koi-embedded)");

    tokio::spawn(async move {
        while let Some(event) = browse.recv().await {
            match event {
                koi_embedded::MdnsEvent::Resolved(record) => {
                    if let Some(discovered) = extract_stone_from_record(&record) {
                        // Skip self-announcements
                        if discovered.stone_name == self_stone_name {
                            continue;
                        }

                        tracing::info!(
                            stone_name = %discovered.stone_name,
                            endpoint = %discovered.endpoint,
                            "mDNS lurk-listener: Discovered neighbor stone"
                        );

                        let _ = tx.send(discovered);
                    }
                }
                koi_embedded::MdnsEvent::Removed { ref name, .. } => {
                    tracing::debug!(service = %name, "mDNS lurk-listener: Service removed");
                }
                _ => {}
            }
        }
        tracing::warn!("mDNS lurk-listener browse stream ended");
    });

    Ok(rx)
}

// ============================================================================
// Helpers
// ============================================================================

/// Build TXT record properties for mDNS registration
pub fn build_txt_properties(
    stone_id: Option<&str>,
    stone_name: &str,
    mac: Option<&str>,
    version: &str,
    health: &str,
    api_port: u16,
    pond_active: bool,
) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    if let Some(id) = stone_id {
        properties.insert("stone_id".to_string(), id.to_string());
    }
    properties.insert("stone_name".to_string(), stone_name.to_string());
    properties.insert("version".to_string(), version.to_string());
    properties.insert("health".to_string(), health.to_string());
    properties.insert("api_port".to_string(), api_port.to_string());
    if let Some(mac_addr) = mac {
        properties.insert("mac".to_string(), mac_addr.to_string());
    }
    // Pond TXT properties — advertise pond membership and HTTPS port
    if pond_active {
        properties.insert(
            garden_common::constants::TXT_POND.to_string(),
            garden_common::constants::POND_ACTIVE.to_string(),
        );
        properties.insert(
            garden_common::constants::TXT_HTTPS_PORT.to_string(),
            garden_common::constants::MOSS_HTTPS.to_string(),
        );
    }
    properties
}

/// Extract a [DiscoveredStone] from a Koi [ServiceRecord](koi_embedded::ServiceRecord).
///
/// Returns `None` if the record has no LAN-routable IP address.
pub fn extract_stone_from_record(record: &koi_embedded::ServiceRecord) -> Option<DiscoveredStone> {
    let ip = record.ip.as_deref()?;

    if !garden_common::infra::koi_client::is_lan_routable(ip) {
        return None;
    }

    let port = record.port.unwrap_or(garden_common::constants::MOSS_HTTP);
    let txt = &record.txt;

    let stone_name = txt
        .get("stone_name")
        .cloned()
        .unwrap_or_else(|| record.name.clone());

    Some(DiscoveredStone {
        stone_id: txt.get("stone_id").cloned(),
        stone_name,
        endpoint: format!("http://{}:{}", ip, port),
        mac: txt.get("mac").cloned(),
        version: txt.get("version").cloned(),
        health: txt.get("health").cloned(),
        discovered_at: chrono::Utc::now(),
    })
}
