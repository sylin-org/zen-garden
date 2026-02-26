//! Stone discovery via Koi mDNS HTTP API.
//!
//! Orchestrators never assume a local Moss is running. Instead, they query
//! Koi's mDNS capability to browse for `_moss._tcp` services on the network.
//!
//! # Discovery strategies (in priority order)
//!
//! 1. **Explicit stone** (`--stone` / `GARDEN_STONE`): skip discovery entirely.
//! 2. **Koi mDNS subscribe**: SSE stream of `_moss._tcp` lifecycle events.
//! 3. **Koi mDNS discover**: One-shot browse for immediate results.

use crate::http::check_response;
use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

/// A stone discovered on the network.
#[derive(Debug, Clone)]
pub struct DiscoveredStone {
    /// Human name from mDNS TXT `stone_name`.
    pub stone_name: String,
    /// Unique GUIDv7 from mDNS TXT `stone_id`.
    pub stone_id: Option<String>,
    /// Resolved IP address.
    pub ip: String,
    /// mDNS hostname, e.g. `stone-quartz-fen.local`.
    pub hostname: String,
    /// Moss API port (from mDNS TXT `api_port`, default 7185).
    pub api_port: u16,
    /// HTTPS port (from mDNS TXT `https_port`), when pond is active.
    pub https_port: Option<u16>,
    /// Moss version from TXT `version`.
    pub version: Option<String>,
    /// Health status from TXT `health`.
    pub health: Option<String>,
    /// Whether a pond is active from TXT `pond`.
    pub pond_active: bool,
}

impl DiscoveredStone {
    /// Moss HTTP endpoint using the `.local` hostname.
    pub fn endpoint(&self) -> String {
        format!("http://{}:{}", self.hostname, self.api_port)
    }
}

// ── Koi mDNS SSE event structures ──────────────────────────────

#[derive(Debug, Deserialize)]
struct MdnsFoundPayload {
    found: Option<MdnsService>,
}

#[derive(Debug, Deserialize)]
struct MdnsSubscribePayload {
    event: Option<String>,
    service: Option<MdnsService>,
}

#[derive(Debug, Deserialize)]
struct MdnsService {
    name: String,
    #[serde(rename = "type")]
    _type: Option<String>,
    host: Option<String>,
    ip: Option<String>,
    port: Option<u16>,
    txt: Option<std::collections::HashMap<String, String>>,
}

impl MdnsService {
    fn to_discovered_stone(&self) -> Option<DiscoveredStone> {
        let ip = self.ip.as_ref()?;
        let txt = self.txt.as_ref();

        let stone_name = txt
            .and_then(|t| t.get("stone_name"))
            .cloned()
            .unwrap_or_else(|| self.name.clone());

        let stone_id = txt.and_then(|t| t.get("stone_id")).cloned();

        // Derive hostname from mDNS host field, falling back to stone_name.local
        let hostname = self
            .host
            .as_deref()
            .map(|h| h.trim_end_matches('.').to_string())
            .unwrap_or_else(|| {
                if stone_name.contains('.') {
                    stone_name.clone()
                } else {
                    format!("{}.local", &stone_name)
                }
            });

        let api_port = txt
            .and_then(|t| t.get("api_port"))
            .and_then(|p| p.parse().ok())
            .or(self.port)
            .unwrap_or(garden_common::constants::MOSS_HTTP);

        let https_port = txt
            .and_then(|t| t.get("https_port"))
            .and_then(|p| p.parse().ok());

        let version = txt.and_then(|t| t.get("version")).cloned();
        let health = txt.and_then(|t| t.get("health")).cloned();
        let pond_active = txt
            .and_then(|t| t.get("pond"))
            .map(|v| v == garden_common::constants::POND_ACTIVE)
            .unwrap_or(false);

        Some(DiscoveredStone {
            stone_name,
            stone_id,
            ip: ip.clone(),
            hostname,
            api_port,
            https_port,
            version,
            health,
            pond_active,
        })
    }
}

// ── Public API ──────────────────────────────────────────────────

/// Discover stones using Koi mDNS browse (one-shot).
///
/// Connects to `GET {koi_endpoint}/v1/mdns/discover?type=_moss._tcp&idle_for=5`
/// and collects all found services before the idle timeout.
pub async fn discover_stones(koi_endpoint: &str) -> Result<Vec<DiscoveredStone>> {
    let url = format!(
        "{}/v1/mdns/discover?type={}&idle_for=5",
        koi_endpoint.trim_end_matches('/'),
        garden_common::constants::MDNS_SERVICE_TYPE,
    );

    tracing::info!(url = %url, "discovering stones via Koi mDNS browse");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?;

    let response = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .context("connect to Koi mDNS discover")?;
    let response = check_response(response, "Koi mDNS discover").await?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut stones = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read mDNS SSE chunk")?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
            buffer.drain(..=newline_pos);

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if let Ok(payload) = serde_json::from_str::<MdnsFoundPayload>(data) {
                    if let Some(svc) = payload.found {
                        if let Some(stone) = svc.to_discovered_stone() {
                            let key = format!("{}:{}", stone.ip, stone.api_port);
                            if seen_ips.insert(key) {
                                tracing::info!(
                                    stone = %stone.stone_name,
                                    ip = %stone.ip,
                                    port = stone.api_port,
                                    "discovered stone via mDNS"
                                );
                                stones.push(stone);
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::info!(count = stones.len(), "mDNS discovery complete");
    Ok(stones)
}

/// Subscribe to stone lifecycle events via Koi mDNS (long-lived SSE stream).
///
/// Connects to `GET {koi_endpoint}/v1/mdns/subscribe?type=_moss._tcp&idle_for=0`
/// (infinite stream) and calls the handler on each lifecycle event.
///
/// Returns when the stream ends or an error occurs.
pub async fn subscribe_stones(
    koi_endpoint: &str,
    mut on_found: impl FnMut(DiscoveredStone),
    mut on_removed: impl FnMut(String),
) -> Result<()> {
    let url = format!(
        "{}/v1/mdns/subscribe?type={}&idle_for=0",
        koi_endpoint.trim_end_matches('/'),
        garden_common::constants::MDNS_SERVICE_TYPE,
    );

    tracing::info!(url = %url, "subscribing to stone lifecycle events via Koi mDNS");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(0))
        .build()?;

    let response = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .context("connect to Koi mDNS subscribe")?;
    let response = check_response(response, "Koi mDNS subscribe").await?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read mDNS subscribe SSE chunk")?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
            buffer.drain(..=newline_pos);

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if let Ok(payload) = serde_json::from_str::<MdnsSubscribePayload>(data) {
                    match payload.event.as_deref() {
                        Some("found") | Some("resolved") => {
                            if let Some(svc) = payload.service {
                                if let Some(stone) = svc.to_discovered_stone() {
                                    on_found(stone);
                                }
                            }
                        }
                        Some("removed") => {
                            if let Some(svc) = payload.service {
                                let name = svc
                                    .txt
                                    .as_ref()
                                    .and_then(|t| t.get("stone_name"))
                                    .cloned()
                                    .unwrap_or(svc.name);
                                on_removed(name);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that a stone endpoint is reachable (health check).
pub async fn check_stone_health(endpoint: &str) -> bool {
    let url = format!("{}/health", endpoint.trim_end_matches('/'));
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    client.get(&url).send().await.is_ok()
}

/// Validate that Koi is reachable.
pub async fn check_koi_health(koi_endpoint: &str) -> bool {
    let url = format!("{}/healthz", koi_endpoint.trim_end_matches('/'));
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    client.get(&url).send().await.is_ok()
}

/// Fetch VRAM total (bytes) and GPU name from a stone's Moss API.
///
/// `stone_host` can be a hostname (e.g. `stone-quartz-fen.local`) or an IP.
///
/// Returns `(vram_total_bytes, gpu_name)`. Returns `(0, None)` on any error.
pub async fn fetch_stone_hw(stone_host: &str) -> (u64, Option<String>) {
    use garden_common::types::HardwareCapabilities;

    #[derive(Deserialize)]
    struct StoneResponse {
        data: HardwareCapabilities,
    }

    let url = format!("http://{}:{}/api/v1/stone", stone_host, garden_common::constants::MOSS_HTTP);

    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (0, None),
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(url = %url, error = %e, "could not reach stone for HW info");
            return (0, None);
        }
    };

    let parsed: StoneResponse = match resp.json().await {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(url = %url, error = %e, "could not parse stone HW response");
            return (0, None);
        }
    };

    let caps = &parsed.data;
    let vram_mb = caps
        .hardware
        .ai_capabilities
        .as_ref()
        .map(|ai| ai.total_vram_mb)
        .unwrap_or(0);
    let gpu_name = caps.hardware.gpus.first().map(|g| g.model.clone());

    (vram_mb * 1_048_576, gpu_name)
}

/// Fetch environment variables for a service from a stone's Moss API.
///
/// Returns an empty map on any error.
pub async fn fetch_service_env(
    moss_endpoint: &str,
    service_name: &str,
) -> std::collections::HashMap<String, String> {
    #[derive(Deserialize)]
    struct EnvResponse {
        data: std::collections::HashMap<String, String>,
    }

    let url = format!(
        "{}/api/v1/stone/services/{}/env",
        moss_endpoint.trim_end_matches('/'),
        service_name
    );

    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return std::collections::HashMap::new(),
    };

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(url = %url, error = %e, "could not fetch service env");
            return std::collections::HashMap::new();
        }
    };

    match resp.json::<EnvResponse>().await {
        Ok(parsed) => parsed.data,
        Err(e) => {
            tracing::debug!(url = %url, error = %e, "could not parse service env response");
            std::collections::HashMap::new()
        }
    }
}
