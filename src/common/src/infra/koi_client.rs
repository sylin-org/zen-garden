//! HTTP client for Koi mDNS proxy
//!
//! Koi is a local mDNS proxy that exposes mDNS operations via HTTP/SSE.
//! This module provides a reusable client for any Zen Garden binary that
//! needs mDNS discovery on Windows (Moss, Lantern, etc.).
//!
//! All methods are fail-safe — errors are logged but never break operation.
//! If Koi is unavailable, callers degrade gracefully to UDP-only discovery.

use std::collections::HashMap;
use std::time::Duration;

const DEFAULT_KOI_HOST: &str = "localhost";
const DEFAULT_KOI_PORT: u16 = 5641;
const REGISTRATION_LEASE_SECS: u32 = 120;
const HEARTBEAT_INTERVAL_SECS: u64 = 60;
const MAX_RECONNECT_BACKOFF_SECS: u64 = 30;

/// HTTP client for Koi mDNS proxy
pub struct KoiClient {
    client: reqwest::Client,
    base_url: String,
}

/// Registration response from Koi
#[derive(Debug, serde::Deserialize)]
struct RegisterResponse {
    registered: RegisteredInfo,
}

#[derive(Debug, serde::Deserialize)]
struct RegisteredInfo {
    id: String,
}

/// SSE event data from Koi events stream.
/// Handles both nested format (`{ "event": "...", "service": {...} }`)
/// and flat format (`{ "name": "...", "ip": "...", ... }`).
#[derive(Debug, serde::Deserialize)]
pub struct KoiEventData {
    // Nested format (current Koi)
    #[allow(dead_code)]
    pub event: Option<String>,
    pub service: Option<KoiServiceInfo>,
    // Flat format (fallback)
    pub name: Option<String>,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub txt: Option<HashMap<String, String>>,
}

/// Service info within a Koi SSE event
#[derive(Debug, serde::Deserialize)]
pub struct KoiServiceInfo {
    pub name: String,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub txt: Option<HashMap<String, String>>,
}

impl KoiClient {
    /// Probe Koi health. Returns None if Koi is unreachable.
    pub async fn try_connect() -> Option<Self> {
        let host = std::env::var("KOI_HOST").unwrap_or_else(|_| DEFAULT_KOI_HOST.to_string());
        let port: u16 = std::env::var("KOI_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_KOI_PORT);

        let base_url = format!("http://{}:{}", host, port);

        // no_proxy: Koi is always local, bypass any HTTP_PROXY settings
        let client = reqwest::Client::builder().no_proxy().build().ok()?;

        let resp = client
            .get(format!("{}/healthz", base_url))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .ok()?;

        if resp.status().is_success() {
            tracing::info!(base_url = %base_url, "Connected to Koi mDNS proxy");
            Some(Self { client, base_url })
        } else {
            tracing::debug!(status = %resp.status(), "Koi health check returned non-OK");
            None
        }
    }

    /// Register a service. Returns registration ID.
    pub async fn register(
        &self,
        name: &str,
        service_type: &str,
        port: u16,
        ip: &str,
        txt: HashMap<String, String>,
        lease_secs: u32,
    ) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "name": name,
            "type": service_type,
            "port": port,
            "ip": ip,
            "txt": txt,
            "lease_secs": lease_secs,
        });

        let resp = self
            .client
            .post(format!("{}/v1/services", self.base_url))
            .timeout(Duration::from_secs(5))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Koi register failed ({}): {}", status, body_text);
        }

        let result: RegisterResponse = resp.json().await?;
        Ok(result.registered.id)
    }

    /// Unregister by ID (best-effort).
    pub async fn unregister(&self, id: &str) -> anyhow::Result<()> {
        let resp = self
            .client
            .delete(format!("{}/v1/services/{}", self.base_url, id))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        if !resp.status().is_success() {
            tracing::debug!(
                id = %id,
                status = %resp.status(),
                "Koi unregister non-OK (best-effort)"
            );
        }

        Ok(())
    }

    /// Send heartbeat. Returns true if renewed, false if registration expired (404).
    pub async fn heartbeat(&self, id: &str) -> anyhow::Result<bool> {
        let resp = self
            .client
            .put(format!("{}/v1/services/{}/heartbeat", self.base_url, id))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        if resp.status().as_u16() == 404 {
            return Ok(false);
        }

        if !resp.status().is_success() {
            anyhow::bail!("Koi heartbeat failed: {}", resp.status());
        }

        Ok(true)
    }

    /// Open SSE events stream. Returns the raw response for chunk-based reading.
    /// No timeout — the SSE connection stays open indefinitely.
    pub async fn open_events_stream(
        &self,
        service_type: &str,
    ) -> anyhow::Result<reqwest::Response> {
        let url = format!(
            "{}/v1/events?type={}&idle_for=0",
            self.base_url, service_type
        );

        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            anyhow::bail!("Koi events endpoint returned {}", resp.status());
        }

        Ok(resp)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn registration_lease_secs() -> u32 {
        REGISTRATION_LEASE_SECS
    }

    pub fn heartbeat_interval() -> Duration {
        Duration::from_secs(HEARTBEAT_INTERVAL_SECS)
    }

    pub fn max_reconnect_backoff() -> Duration {
        Duration::from_secs(MAX_RECONNECT_BACKOFF_SECS)
    }
}

/// Build TXT record properties for mDNS registration
pub fn build_txt_properties(
    stone_id: Option<&str>,
    stone_name: &str,
    mac: Option<&str>,
    version: &str,
    health: &str,
    api_port: u16,
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
    properties
}

/// Extract service info from SSE event data (handles both nested and flat formats)
///
/// Returns `(name, ip, port, txt_properties)` or None if required fields are missing.
pub fn extract_service_info(
    data: &KoiEventData,
) -> Option<(String, String, u16, HashMap<String, String>)> {
    if let Some(ref svc) = data.service {
        // Nested format: { "service": { "name", "ip", "port", "txt" } }
        let ip = svc.ip.clone()?;
        let port = svc.port?;
        Some((
            svc.name.clone(),
            ip,
            port,
            svc.txt.clone().unwrap_or_default(),
        ))
    } else {
        // Flat format: { "name", "ip", "port", "txt" }
        let name = data.name.clone()?;
        let ip = data.ip.clone()?;
        let port = data.port?;
        Some((name, ip, port, data.txt.clone().unwrap_or_default()))
    }
}

/// Check if an IP address is LAN-routable
///
/// Accepts private ranges (RFC 1918), rejects loopback, link-local, and Docker bridge.
pub fn is_lan_routable(ip: &str) -> bool {
    let addr: std::net::Ipv4Addr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return false, // IPv6 or invalid — skip
    };

    let octets = addr.octets();

    // 10.0.0.0/8
    if octets[0] == 10 {
        return true;
    }

    // 172.16.0.0/12 (excluding 172.17.0.0/16 — Docker default bridge)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) && octets[1] != 17 {
        return true;
    }

    // 192.168.0.0/16
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }

    false
}

// ============================================================================
// High-level discovery API
// ============================================================================

/// A stone discovered via mDNS (either Koi SSE or native mdns-sd).
///
/// This is the canonical discovery result type used by all consumers.
/// Self-filtering (skip own stone_name) is the caller's responsibility.
#[derive(Debug, Clone)]
pub struct DiscoveredStone {
    pub stone_id: Option<String>,
    pub stone_name: String,
    pub endpoint: String,
    pub mac: Option<String>,
    pub version: Option<String>,
    pub health: Option<String>,
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}

/// Parse a single SSE event block into a [`DiscoveredStone`].
///
/// Checks both the SSE `event:` header and the JSON body `event` field
/// for `"resolved"` — either match is sufficient. Returns `None` for
/// non-resolved events, unparseable data, or non-LAN-routable IPs.
///
/// Self-filtering is NOT performed here — the caller decides.
pub fn parse_sse_event(event_block: &str) -> Option<DiscoveredStone> {
    let mut event_type = String::new();
    let mut data_line = String::new();

    for line in event_block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_type = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_line = value.trim().to_string();
        }
    }

    if data_line.is_empty() {
        return None;
    }

    // Skip removed events — consumers that care (Lantern) handle these locally
    if event_type == "removed" {
        return None;
    }

    let event_data: KoiEventData = match serde_json::from_str(&data_line) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = ?e, data = %data_line, "Failed to parse Koi SSE event data");
            return None;
        }
    };

    // Check event type from BOTH the SSE header and the JSON body.
    // Koi's events endpoint only sets the event type in the JSON body
    // (Event::default().data()), not as an SSE event: header.
    let resolved = event_type == "resolved"
        || event_data
            .event
            .as_deref()
            .is_some_and(|e| e == "resolved");

    if !resolved {
        return None;
    }

    let (_name, ip, port, txt) = extract_service_info(&event_data)?;

    if !is_lan_routable(&ip) {
        tracing::debug!(ip = %ip, "Koi: filtered non-LAN IP from discovery");
        return None;
    }

    let stone_name = txt.get("stone_name").cloned()?;
    let endpoint = format!("http://{}:{}", ip, port);

    tracing::info!(
        stone_name = %stone_name,
        endpoint = %endpoint,
        "Koi: discovered stone via mDNS"
    );

    Some(DiscoveredStone {
        stone_id: txt.get("stone_id").cloned(),
        stone_name,
        endpoint,
        mac: txt.get("mac").cloned(),
        version: txt.get("version").cloned(),
        health: txt.get("health").cloned(),
        discovered_at: chrono::Utc::now(),
    })
}

/// Stream SSE events from Koi, parsing each into [`DiscoveredStone`].
///
/// Buffers incoming chunks by `\n\n` delimiters and parses each complete
/// event block. Discovered stones are sent to the broadcast channel.
/// Returns `Ok(())` on clean disconnect, `Err` on connection errors.
pub async fn stream_sse_events(
    koi: &KoiClient,
    service_type: &str,
    tx: &tokio::sync::broadcast::Sender<DiscoveredStone>,
) -> anyhow::Result<()> {
    let mut resp = koi.open_events_stream(service_type).await?;

    tracing::debug!(base_url = %koi.base_url(), "Connected to Koi SSE events stream");

    let mut buffer = String::new();

    while let Some(chunk) = resp.chunk().await? {
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            if let Some(discovered) = parse_sse_event(&event_block) {
                let _ = tx.send(discovered);
            }
        }
    }

    Ok(())
}

/// Connect to Koi SSE and stream discovery events with automatic reconnection.
///
/// On disconnect or error, backs off exponentially (1s → max_reconnect_backoff).
/// On clean disconnect, resets to 1s. Runs forever — caller spawns this.
pub async fn run_koi_discovery_loop(
    koi: std::sync::Arc<KoiClient>,
    service_type: &str,
    tx: tokio::sync::broadcast::Sender<DiscoveredStone>,
) {
    let mut backoff = Duration::from_secs(1);
    let max_backoff = KoiClient::max_reconnect_backoff();

    tracing::info!("Koi mDNS discovery loop started (passive topology discovery via SSE)");

    loop {
        match stream_sse_events(&koi, service_type, &tx).await {
            Ok(()) => {
                backoff = Duration::from_secs(1);
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

/// Extract a [`DiscoveredStone`] from an `mdns_sd::ServiceInfo`.
///
/// Shared helper for Linux native mDNS discovery. Returns `None` if
/// the service has no LAN-routable IP addresses.
pub fn extract_stone_from_service_info(
    info: &mdns_sd::ServiceInfo,
) -> Option<DiscoveredStone> {
    let ip = info.get_addresses().iter().next()?;
    let ip_str = ip.to_string();

    if !is_lan_routable(&ip_str) {
        return None;
    }

    let get_txt = |key: &str| -> Option<String> {
        info.get_properties()
            .iter()
            .find(|p| p.key() == key)
            .map(|p| p.val_str().to_string())
    };

    let stone_name = get_txt("stone_name").unwrap_or_else(|| {
        info.get_fullname()
            .split('.')
            .next()
            .unwrap_or("unknown")
            .to_string()
    });

    Some(DiscoveredStone {
        stone_id: get_txt("stone_id"),
        stone_name,
        endpoint: format!("http://{}:{}", ip, info.get_port()),
        mac: get_txt("mac"),
        version: get_txt("version"),
        health: get_txt("health"),
        discovered_at: chrono::Utc::now(),
    })
}
