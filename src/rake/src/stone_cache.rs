use crate::client::{CachedStoneInfo, CachedStoneOps};
use anyhow::Result;
use garden_common::client::discovery::{Discovery, KnownStone};
use garden_common::{GardenApiResponse, HardwareCapabilities};
use std::time::Duration;

pub use garden_common::client::discovery::STONE;

/// Rake-specific discovered stone wrapper
///
/// Wraps common::KnownStone with rake-specific fields.
#[derive(Clone)]
pub struct DiscoveredStone {
    pub endpoint: String,
    pub capabilities: HardwareCapabilities,
    pub last_seen: std::time::Instant,
}

impl DiscoveredStone {
    /// Get the cache key for this stone
    pub fn cache_key(&self) -> String {
        self.capabilities
            .stone_id
            .clone()
            .unwrap_or_else(|| self.capabilities.stone_name.clone())
    }

    /// Convert from common KnownStone (requires fetching capabilities)
    #[expect(dead_code)]
    fn from_known(known: &KnownStone, capabilities: HardwareCapabilities) -> Self {
        Self {
            endpoint: known.endpoint.clone(),
            capabilities,
            last_seen: known.last_seen,
        }
    }
}

// Implement CachedStoneOps trait for Discovery
impl CachedStoneOps for Discovery {
    fn get(&self, stone_name: &str) -> Option<CachedStoneInfo> {
        Discovery::get(self, stone_name).map(|known| CachedStoneInfo {
            endpoint: known.endpoint,
        })
    }

    fn insert(&self, endpoint: String, capabilities: HardwareCapabilities) {
        let stone_id = capabilities.stone_id.clone();
        let stone_name = capabilities.stone_name.clone();
        Discovery::insert(self, endpoint, stone_id, stone_name);
    }
}

/// Fetch stone capabilities from endpoint and cache them
pub async fn fetch_and_cache_stone(
    client: &reqwest::Client,
    endpoint: &str,
    discovery: &Discovery,
) -> Result<DiscoveredStone> {
    let caps_url = format!(
        "{}/api/v1/stone/capabilities",
        endpoint.trim_end_matches('/')
    );
    let response: GardenApiResponse<HardwareCapabilities> = client
        .get(&caps_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .json()
        .await?;
    let capabilities = response.data;

    // Cache using the simplified interface (endpoint, stone_id, stone_name)
    let stone_id = capabilities.stone_id.clone();
    let stone_name = capabilities.stone_name.clone();
    discovery.insert(endpoint.to_string(), stone_id, stone_name);

    Ok(DiscoveredStone {
        endpoint: endpoint.to_string(),
        capabilities,
        last_seen: std::time::Instant::now(),
    })
}
