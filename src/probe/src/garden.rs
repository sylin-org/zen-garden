//! Live Garden - connects to real stones for testing
//!
//! Supports two discovery modes:
//! 1. **UDP Discovery** (like Rake) - broadcasts to find all stones on the network
//! 2. **HTTP Topology** - queries a known stone's topology endpoint
//!
//! UDP discovery caches all responding stones, enabling:
//! - Fast failover when tended stone goes offline
//! - Inter-stone communication tests (deploy to A, verify B sees chirp)

use anyhow::{Context, Result};
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::{DiscoveryRequest, DiscoveryResponse};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// A connected garden with discovered stones
#[derive(Clone)]
pub struct LiveGarden {
    /// All discovered stones
    pub stones: Vec<Stone>,
    /// The currently tended stone (commands target this by default)
    pub tended: Option<Stone>,
    /// Discovery metadata
    pub discovery: DiscoveryInfo,
}

/// Information about how the garden was discovered
#[derive(Clone, Debug, Default)]
pub struct DiscoveryInfo {
    /// Discovery method used
    pub method: DiscoveryMethod,
    /// Time taken to discover all stones
    pub duration_ms: u64,
    /// Number of discovery responses received
    pub responses: usize,
    /// Per-stone discovery timing (name, response_time_ms)
    pub timings: Vec<(String, u64)>,
}

/// How the garden was discovered
#[derive(Clone, Debug, Default)]
pub enum DiscoveryMethod {
    /// UDP broadcast/multicast discovery (like Rake)
    Udp,
    /// HTTP topology query from known stone
    #[default]
    HttpTopology,
    /// Manual endpoint specification
    Manual,
}

impl LiveGarden {
    /// Discover garden via UDP broadcast (like Rake does)
    ///
    /// This broadcasts a discovery request and collects all responding stones.
    /// The first responder becomes the tended stone.
    ///
    /// Benefits over HTTP topology:
    /// - Finds all stones even if some don't know about others
    /// - Caches stones for fast failover
    /// - Shows real network physics (response times)
    pub async fn discover_udp(timeout: Duration) -> Result<Self> {
        let start = Instant::now();
        let mut stones = Vec::new();
        let mut timings = Vec::new();
        let mut seen_endpoints = HashSet::new();

        // Subscribe to discovery responses before sending request
        let mut response_rx =
            p2p::subscribe_to_announcement(announcement_types::DISCOVERY_RESPONSE)
                .await
                .context("Failed to subscribe to discovery responses")?;

        // Send discovery request
        let request_id = uuid::Uuid::now_v7().to_string();
        let request = DiscoveryRequest {
            discover: "moss".into(),
            request_id: request_id.clone(),
            requester: "garden-probe".into(),
        };

        p2p::send_announcement(announcement_types::DISCOVERY_REQUEST, &request)
            .await
            .context("Failed to send discovery request")?;

        tracing::info!(request_id = %request_id, "Sent UDP discovery broadcast");

        // Collect responses until timeout
        let collect_future = async {
            while let Some((payload, addr)) = response_rx.recv().await {
                if let Ok(response) = serde_json::from_value::<DiscoveryResponse>(payload) {
                    // Deduplicate by endpoint
                    if !seen_endpoints.contains(&response.stone_endpoint) {
                        seen_endpoints.insert(response.stone_endpoint.clone());

                        let response_time = start.elapsed().as_millis() as u64;
                        timings.push((response.stone_name.clone(), response_time));

                        tracing::info!(
                            stone = %response.stone_name,
                            endpoint = %response.stone_endpoint,
                            from = ?addr,
                            response_time_ms = response_time,
                            "Discovered stone"
                        );

                        stones.push(Stone::new(
                            response.stone_name.clone(),
                            response.stone_endpoint.clone(),
                        ));
                    }
                }
            }
        };

        // Wait for timeout
        let _ = tokio::time::timeout(timeout, collect_future).await;

        let duration = start.elapsed();
        tracing::info!(
            count = stones.len(),
            duration_ms = duration.as_millis(),
            "UDP discovery complete"
        );

        // First responder becomes tended
        let tended = stones.first().cloned();

        Ok(Self {
            stones,
            tended,
            discovery: DiscoveryInfo {
                method: DiscoveryMethod::Udp,
                duration_ms: duration.as_millis() as u64,
                responses: timings.len(),
                timings,
            },
        })
    }

    /// Discover garden by querying a known stone's topology
    ///
    /// Fallback when UDP discovery isn't available (e.g., firewall issues)
    pub async fn discover(initial_endpoint: &str) -> Result<Self> {
        let start = Instant::now();
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;

        // Query topology from initial stone
        let url = format!("{}/api/v1/garden", initial_endpoint);
        let resp: serde_json::Value = client
            .get(&url)
            .send()
            .await
            .context("Failed to connect to stone")?
            .json()
            .await
            .context("Failed to parse topology")?;

        let mut stones = Vec::new();
        let mut tended: Option<Stone> = None;
        let mut timings = Vec::new();

        // Parse stones from topology response
        if let Some(data) = resp.get("data") {
            if let Some(stones_arr) = data.get("stones").and_then(|s| s.as_array()) {
                for stone_val in stones_arr {
                    if let (Some(name), Some(endpoint)) = (
                        stone_val.get("name").and_then(|n| n.as_str()),
                        stone_val.get("endpoint").and_then(|e| e.as_str()),
                    ) {
                        let stone = Stone::new(name.to_string(), endpoint.to_string());
                        timings.push((name.to_string(), start.elapsed().as_millis() as u64));

                        // Mark the stone we queried as tended
                        if endpoint == initial_endpoint {
                            tended = Some(stone.clone());
                        }
                        stones.push(stone);
                    }
                }
            }
        }

        // If no tended stone found yet, mark the first one as tended
        if tended.is_none() && !stones.is_empty() {
            tended = Some(stones[0].clone());
        }

        // If no stones found from topology, at least add the initial one
        if stones.is_empty() {
            // Try to get stone name from the initial endpoint
            let caps_url = format!("{}/api/v1/stone/capabilities", initial_endpoint);
            let stone_name = if let Ok(resp) = client.get(&caps_url).send().await {
                if let Ok(caps) = resp.json::<serde_json::Value>().await {
                    caps.get("stone_name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            };

            let stone = Stone::new(stone_name.clone(), initial_endpoint.to_string());
            timings.push((stone_name, start.elapsed().as_millis() as u64));
            tended = Some(stone.clone());
            stones.push(stone);
        }

        let duration = start.elapsed();

        Ok(Self {
            stones,
            tended,
            discovery: DiscoveryInfo {
                method: DiscoveryMethod::HttpTopology,
                duration_ms: duration.as_millis() as u64,
                responses: timings.len(),
                timings,
            },
        })
    }

    /// Auto-discover using best available method
    ///
    /// Tries UDP discovery first, falls back to HTTP topology if provided.
    pub async fn auto_discover(timeout: Duration, fallback_endpoint: Option<&str>) -> Result<Self> {
        // Try UDP discovery first
        match Self::discover_udp(timeout).await {
            Ok(garden) if !garden.stones.is_empty() => {
                tracing::info!(
                    method = "UDP",
                    stones = garden.stones.len(),
                    "Auto-discovery succeeded"
                );
                return Ok(garden);
            }
            Ok(_) => {
                tracing::warn!("UDP discovery found no stones");
            }
            Err(e) => {
                tracing::warn!(error = %e, "UDP discovery failed");
            }
        }

        // Fall back to HTTP topology if endpoint provided
        if let Some(endpoint) = fallback_endpoint {
            tracing::info!(endpoint = %endpoint, "Falling back to HTTP topology discovery");
            return Self::discover(endpoint).await;
        }

        anyhow::bail!("No stones discovered via UDP and no fallback endpoint provided")
    }

    /// Connect to specific stone endpoints (manual mode)
    pub fn connect(endpoints: &[(&str, &str)]) -> Self {
        let stones: Vec<Stone> = endpoints
            .iter()
            .map(|(name, endpoint)| Stone::new(name.to_string(), endpoint.to_string()))
            .collect();

        let tended = stones.first().cloned();
        let stone_count = stones.len();

        Self {
            stones,
            tended,
            discovery: DiscoveryInfo {
                method: DiscoveryMethod::Manual,
                duration_ms: 0,
                responses: stone_count,
                timings: Vec::new(),
            },
        }
    }

    /// Get a stone by name
    pub fn stone(&self, name: &str) -> Option<&Stone> {
        self.stones.iter().find(|s| s.name == name)
    }

    /// Get the tended stone (or first if none marked)
    pub fn tended(&self) -> Option<&Stone> {
        self.tended.as_ref().or(self.stones.first())
    }

    /// Get all stone names
    pub fn stone_names(&self) -> Vec<&str> {
        self.stones.iter().map(|s| s.name.as_str()).collect()
    }

    /// Get all stones except the tended one (for inter-stone tests)
    pub fn other_stones(&self) -> Vec<&Stone> {
        self.stones
            .iter()
            .filter(|s| {
                self.tended
                    .as_ref()
                    .map(|t| t.endpoint != s.endpoint)
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Switch tended stone to a different one by name
    pub fn switch_tended(&mut self, name: &str) -> Result<()> {
        let new_tended = self
            .stones
            .iter()
            .find(|s| s.name == name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Stone '{}' not found", name))?;

        self.tended = Some(new_tended);
        Ok(())
    }

    /// Number of stones
    pub fn len(&self) -> usize {
        self.stones.len()
    }

    /// Is garden empty
    pub fn is_empty(&self) -> bool {
        self.stones.is_empty()
    }
}

/// A single stone client
#[derive(Clone)]
pub struct Stone {
    pub name: String,
    pub endpoint: String,
    client: Client,
}

impl Stone {
    pub fn new(name: String, endpoint: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            name,
            endpoint,
            client,
        }
    }

    /// HTTP GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("GET {} returned {}", url, status.as_u16());
        }

        let resp = response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))?;

        Ok(resp)
    }

    /// HTTP GET returning raw JSON
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        self.get(path).await
    }

    /// HTTP POST request
    pub async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("POST {} returned {}", url, status.as_u16());
        }

        let resp = response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))?;

        Ok(resp)
    }

    /// HTTP POST returning raw JSON
    pub async fn post_json<B: Serialize>(&self, path: &str, body: &B) -> Result<serde_json::Value> {
        self.post(path, body).await
    }

    /// HTTP DELETE request
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .with_context(|| format!("DELETE {} failed", url))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("DELETE {} returned {}", url, status.as_u16());
        }

        let resp = response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))?;

        Ok(resp)
    }

    /// HTTP DELETE returning raw JSON
    pub async fn delete_json(&self, path: &str) -> Result<serde_json::Value> {
        self.delete(path).await
    }

    /// HTTP DELETE returning just status code
    pub async fn delete_status_code(&self, path: &str) -> Result<u16> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .delete(&url)
            .send()
            .await
            .with_context(|| format!("DELETE {} failed", url))?;

        Ok(response.status().as_u16())
    }

    /// HTTP PUT with raw bytes
    pub async fn put_bytes(
        &self,
        path: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .put(&url)
            .header("Content-Type", content_type)
            .body(body)
            .send()
            .await
            .with_context(|| format!("PUT {} failed", url))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("PUT {} returned {}", url, status.as_u16());
        }

        let resp = response
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))?;

        Ok(resp)
    }

    /// HTTP GET returning raw bytes
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        let url = format!("{}{}", self.endpoint, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;

        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("GET {} returned {}", url, status.as_u16());
        }

        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("Failed to read bytes from {}", url))?;

        Ok(bytes.to_vec())
    }

    /// Check if stone is healthy
    pub async fn is_healthy(&self) -> bool {
        self.get_json("/health").await.is_ok()
    }

    /// Get stone platform info (os, architecture)
    /// Returns (os, architecture) - e.g., ("linux", "x86_64") or ("windows", "x86_64")
    pub async fn get_platform(&self) -> Result<(String, String)> {
        let health: serde_json::Value = self.get_json("/health").await?;

        let os = health
            .get("os")
            .and_then(|v| v.as_str())
            .unwrap_or("linux") // Default to Linux for older versions
            .to_string();

        let arch = health
            .get("architecture")
            .and_then(|v| v.as_str())
            .unwrap_or("x86_64")
            .to_string();

        Ok((os, arch))
    }

    /// Check if this stone is running Linux
    pub async fn is_linux(&self) -> bool {
        match self.get_platform().await {
            Ok((os, _)) => os.to_lowercase().contains("linux"),
            Err(_) => true, // Default to Linux if health check fails
        }
    }

    /// Wait until a condition is true, with timeout
    pub async fn wait_until<F, Fut>(
        &self,
        description: &str,
        timeout: Duration,
        poll_interval: Duration,
        condition: F,
    ) -> Result<u32>
    where
        F: Fn(&Stone) -> Fut,
        Fut: std::future::Future<Output = Result<bool>>,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut attempts = 0u32;

        loop {
            attempts += 1;
            match condition(self).await {
                Ok(true) => return Ok(attempts),
                Ok(false) => {}
                Err(e) => {
                    tracing::debug!(
                        stone = %self.name,
                        attempt = attempts,
                        error = %e,
                        "Wait condition check failed"
                    );
                }
            }

            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "{} on {}: timed out after {:?} ({} attempts)",
                    description,
                    self.name,
                    timeout,
                    attempts
                );
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Wait until offering reaches a specific state
    pub async fn wait_offering_state(
        &self,
        offering: &str,
        target_state: &str,
        timeout: Duration,
    ) -> Result<u32> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut attempts = 0u32;
        let poll_interval = Duration::from_secs(2);

        loop {
            attempts += 1;

            let resp: serde_json::Value = self
                .get_json(&format!("/api/v1/stone/offerings/{}", offering))
                .await?;

            let state = resp
                .get("data")
                .and_then(|d| d.get("vitality"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");

            if state == target_state {
                return Ok(attempts);
            }

            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!(
                    "Offering {} on {}: expected state '{}' but got '{}' after {:?} ({} attempts)",
                    offering,
                    self.name,
                    target_state,
                    state,
                    timeout,
                    attempts
                );
            }

            tokio::time::sleep(poll_interval).await;
        }
    }
}

impl std::fmt::Debug for Stone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stone")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .finish()
    }
}

impl std::fmt::Debug for LiveGarden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveGarden")
            .field("stones", &self.stone_names())
            .field("tended", &self.tended.as_ref().map(|s| &s.name))
            .finish()
    }
}
