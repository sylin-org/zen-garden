//! Stone discovery via Koi mDNS HTTP API.
//!
//! The orchestrator never assumes a local Moss is running. Instead, it queries
//! Koi's mDNS capability to browse for `_moss._tcp` services on the network.
//! This mirrors how Rake discovers stones — through Koi's HTTP bridge over
//! multicast, which works from containers, Windows, and any environment where
//! Koi is available.
//!
//! # Discovery strategies (in priority order)
//!
//! 1. **Explicit stone** (`--stone` / `GARDEN_STONE`): skip discovery entirely.
//! 2. **Koi mDNS subscribe**: SSE stream of `_moss._tcp` lifecycle events.
//! 3. **Koi mDNS discover**: One-shot browse for immediate results.

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
    /// HTTP endpoint for the Moss API on this stone.
    pub fn http_endpoint(&self) -> String {
        format!("http://{}:{}", self.ip, self.api_port)
    }

    /// Preferred endpoint — always HTTP for now.
    ///
    /// The orchestrator doesn't have certmesh certificates, so HTTPS
    /// connections to pond-enrolled stones would fail. Use HTTP until
    /// the orchestrator can enroll in the mesh.
    pub fn endpoint(&self) -> String {
        self.http_endpoint()
    }
}

// ── Koi mDNS SSE event structures ──────────────────────────────

/// Wrapper for mDNS found/resolved SSE `data` payloads.
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
    #[allow(dead_code)]
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
        .timeout(Duration::from_secs(15)) // 5s idle_for + buffer
        .build()?;

    let response = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .context("connect to Koi mDNS discover")?
        .error_for_status()
        .context("Koi mDNS discover status")?;

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
                            // Deduplicate by IP:port
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
    mut on_removed: impl FnMut(String), // stone name
) -> Result<()> {
    let url = format!(
        "{}/v1/mdns/subscribe?type={}&idle_for=0",
        koi_endpoint.trim_end_matches('/'),
        garden_common::constants::MDNS_SERVICE_TYPE,
    );

    tracing::info!(url = %url, "subscribing to stone lifecycle events via Koi mDNS");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(0)) // no timeout for infinite SSE
        .build()?;

    let response = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .context("connect to Koi mDNS subscribe")?
        .error_for_status()
        .context("Koi mDNS subscribe status")?;

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
                // mDNS subscribe sends { "event": "found"|"resolved"|"removed", "service": {...} }
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

// ── Topology API ────────────────────────────────────────────────

/// An Ollama stone discovered via the topology REST endpoint.
///
/// The topology endpoint (`GET /api/v1/garden/topology`) is the authoritative
/// view of all stones and their offerings, populated via UDP chirp between
/// stones. Unlike the Tools API SSE stream (which is eventually consistent),
/// the topology returns every stone that has been seen on the network.
///
/// Hardware capabilities (VRAM, GPU name) are extracted directly from the
/// chirp payload — no separate portrait fetch required.
#[derive(Debug, Clone)]
pub struct TopologyOllamaStone {
    pub stone_id: String,
    pub stone_name: String,
    pub ip: String,
    pub moss_port: u16,
    /// Total VRAM in bytes, from `capabilities.hardware.ai_capabilities.total_vram_mb`.
    /// Zero if the stone hasn't completed GPU detection yet.
    pub vram_total_bytes: u64,
    /// Primary GPU name (first entry in `capabilities.hardware.gpus`).
    pub gpu_name: Option<String>,
}

impl TopologyOllamaStone {
    /// Default Ollama endpoint: `http://{ip}:11434`.
    pub fn ollama_endpoint(&self) -> String {
        format!("http://{}:11434", self.ip)
    }
}

/// Query the topology endpoint on a tended stone and return all stones that
/// have a running Ollama offering.
///
/// This mirrors how `garden-rake observe` discovers Ollama stones — a single
/// REST call that returns the full network view.  Hardware capabilities
/// (VRAM, GPU name) are extracted directly from the chirp payload.
pub async fn query_topology_ollama(stone_endpoint: &str) -> Result<Vec<TopologyOllamaStone>> {
    use garden_common::types::topology::TopologyEntry;

    #[derive(Deserialize)]
    struct TopologyResponse {
        data: Vec<TopologyEntry>,
    }

    let url = format!(
        "{}/api/v1/garden/topology",
        stone_endpoint.trim_end_matches('/')
    );

    tracing::info!(url = %url, "querying topology for Ollama stones");

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("connect to topology endpoint at {url}"))?
        .error_for_status()
        .with_context(|| format!("topology endpoint status from {url}"))?;

    let topo: TopologyResponse = response.json().await.context("parse topology response")?;

    let mut results = Vec::new();
    for entry in &topo.data {
        let has_ollama = entry
            .services
            .iter()
            .any(|s| s.offering == "ollama" && s.status == "running");
        if has_ollama {
            // Extract VRAM and GPU name from chirp capabilities
            let (vram_total_bytes, gpu_name) = match &entry.capabilities {
                Some(caps) => {
                    let vram_mb = caps
                        .hardware
                        .ai_capabilities
                        .as_ref()
                        .map(|ai| ai.total_vram_mb)
                        .unwrap_or(0);
                    let name = caps.hardware.gpus.first().map(|g| g.model.clone());
                    (vram_mb * 1_048_576, name)
                }
                None => (0, None),
            };

            results.push(TopologyOllamaStone {
                stone_id: entry.stone_id.clone(),
                stone_name: entry.stone_name.clone(),
                ip: entry.address.ip.to_string(),
                moss_port: entry.address.port,
                vram_total_bytes,
                gpu_name,
            });
        }
    }

    tracing::info!(
        count = results.len(),
        stones = ?results.iter().map(|s| &s.stone_name).collect::<Vec<_>>(),
        "topology query found Ollama stones"
    );

    Ok(results)
}

/// Fetch VRAM total (bytes) and GPU name from a stone's Moss API.
///
/// Queries `GET /api/v1/stone` on the stone's Moss port and extracts
/// hardware capabilities from the response. Used for SSE-discovered stones
/// that weren't in the initial topology snapshot.
///
/// Returns `(vram_total_bytes, gpu_name)`.  Returns `(0, None)` on any error.
pub async fn fetch_stone_hw(stone_ip: &str) -> (u64, Option<String>) {
    use garden_common::types::HardwareCapabilities;

    #[derive(Deserialize)]
    struct StoneResponse {
        data: HardwareCapabilities,
    }

    let url = format!("http://{}:7185/api/v1/stone", stone_ip);

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
