//! HTTP client for Koi mDNS proxy (Windows only)
//!
//! Koi is a local mDNS proxy that exposes mDNS operations via HTTP/SSE.
//! This module provides a client that Moss uses on Windows to achieve
//! mDNS feature parity with Linux.
//!
//! All methods are fail-safe — errors are logged but never break Moss operation.
//! If Koi is unavailable, Moss degrades gracefully to UDP-only discovery.

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
pub(crate) struct KoiEventData {
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
pub(crate) struct KoiServiceInfo {
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
pub(crate) fn build_txt_properties(
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
pub(crate) fn extract_service_info(
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
pub(crate) fn is_lan_routable(ip: &str) -> bool {
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

/// Parse a single SSE event block into an MdnsDiscoveredStone
///
/// SSE format:
/// ```text
/// event: resolved
/// data: {"name":"stone-name","type":"_moss._tcp.local.","ip":"192.168.1.x","port":7185,"txt":{...}}
/// ```
pub(crate) fn parse_sse_event(
    event_block: &str,
    self_stone_name: &str,
) -> Option<crate::mdns::MdnsDiscoveredStone> {
    let mut event_type = String::new();
    let mut data_line = String::new();

    for line in event_block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_type = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_line = value.trim().to_string();
        }
    }

    // Only process "resolved" events
    if event_type != "resolved" || data_line.is_empty() {
        if event_type == "removed" {
            tracing::debug!(
                data = %data_line,
                "Koi: service removed event (topology handles TTL)"
            );
        }
        return None;
    }

    let event_data: KoiEventData = match serde_json::from_str(&data_line) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(
                error = ?e,
                data = %data_line,
                "Failed to parse Koi SSE event data"
            );
            return None;
        }
    };

    let (name, ip, port, txt) = extract_service_info(&event_data)?;

    // Prefer stone_name from TXT, fall back to service name
    let stone_name = txt
        .get("stone_name")
        .cloned()
        .unwrap_or_else(|| name.clone());

    // Skip self-announcements
    if stone_name == self_stone_name {
        return None;
    }

    // Filter non-LAN IPs (defense-in-depth for non-pinned registrations)
    if !is_lan_routable(&ip) {
        tracing::debug!(
            ip = %ip,
            stone_name = %stone_name,
            "Koi: filtered non-LAN IP from discovery"
        );
        return None;
    }

    let stone_id = txt.get("stone_id").cloned();
    let mac = txt.get("mac").cloned();
    let version = txt.get("version").cloned();
    let health = txt.get("health").cloned();
    let endpoint = format!("http://{}:{}", ip, port);

    tracing::info!(
        stone_id = ?stone_id,
        stone_name = %stone_name,
        endpoint = %endpoint,
        mac = ?mac,
        version = ?version,
        health = ?health,
        "Koi: Discovered neighbor stone via mDNS"
    );

    Some(crate::mdns::MdnsDiscoveredStone {
        stone_id,
        stone_name,
        endpoint,
        mac,
        version,
        health,
        discovered_at: chrono::Utc::now(),
    })
}
