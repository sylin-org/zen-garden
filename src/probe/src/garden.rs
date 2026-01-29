//! Live Garden - connects to real stones for testing

use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;

/// A connected garden with discovered stones
#[derive(Clone)]
pub struct LiveGarden {
    pub stones: Vec<Stone>,
    pub tended: Option<Stone>,
}

impl LiveGarden {
    /// Discover garden by querying a known stone's topology
    pub async fn discover(initial_endpoint: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

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

        // Parse stones from topology response
        if let Some(data) = resp.get("data") {
            if let Some(stones_arr) = data.get("stones").and_then(|s| s.as_array()) {
                for stone_val in stones_arr {
                    if let (Some(name), Some(endpoint)) = (
                        stone_val.get("name").and_then(|n| n.as_str()),
                        stone_val.get("endpoint").and_then(|e| e.as_str()),
                    ) {
                        let stone = Stone::new(name.to_string(), endpoint.to_string());

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
            let caps_url = format!("{}/capabilities", initial_endpoint);
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

            let stone = Stone::new(stone_name, initial_endpoint.to_string());
            tended = Some(stone.clone());
            stones.push(stone);
        }

        Ok(Self { stones, tended })
    }

    /// Connect to specific stone endpoints
    pub fn connect(endpoints: &[(&str, &str)]) -> Self {
        let stones: Vec<Stone> = endpoints
            .iter()
            .map(|(name, endpoint)| Stone::new(name.to_string(), endpoint.to_string()))
            .collect();

        let tended = stones.first().cloned();

        Self { stones, tended }
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
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?
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
        let resp = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?
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
        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .with_context(|| format!("DELETE {} failed", url))?
            .json()
            .await
            .with_context(|| format!("Failed to parse response from {}", url))?;

        Ok(resp)
    }

    /// HTTP DELETE returning raw JSON
    pub async fn delete_json(&self, path: &str) -> Result<serde_json::Value> {
        self.delete(path).await
    }

    /// Check if stone is healthy
    pub async fn is_healthy(&self) -> bool {
        self.get_json("/health").await.is_ok()
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
                .get_json(&format!("/api/v1/offerings/{}", offering))
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
