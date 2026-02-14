use crate::client::{CachedStoneInfo, CachedStoneOps};
use anyhow::Result;
use garden_common::client::stone_cache::{CachedStone as CommonCachedStone, StoneCache};
use garden_common::{GardenApiResponse, HardwareCapabilities};
use std::time::Duration;

// Re-export GLOBAL_CACHE from common for backward compatibility
pub use garden_common::client::stone_cache::GLOBAL_CACHE;

/// Rake-specific cached stone wrapper
///
/// Wraps common::CachedStone with rake-specific fields.
#[derive(Clone)]
pub struct CachedStone {
    pub endpoint: String,
    pub capabilities: HardwareCapabilities,
    pub last_seen: std::time::Instant,
}

impl CachedStone {
    /// Get the cache key for this stone
    pub fn cache_key(&self) -> String {
        self.capabilities
            .stone_id
            .clone()
            .unwrap_or_else(|| self.capabilities.stone_name.clone())
    }

    /// Convert from common CachedStone (requires fetching capabilities)
    #[allow(dead_code)]
    fn from_common(common: &CommonCachedStone, capabilities: HardwareCapabilities) -> Self {
        Self {
            endpoint: common.endpoint.clone(),
            capabilities,
            last_seen: common.last_seen,
        }
    }
}

// Implement CachedStoneOps trait for StoneCache (common version)
impl CachedStoneOps for StoneCache {
    fn get(&self, stone_name: &str) -> Option<CachedStoneInfo> {
        StoneCache::get(self, stone_name).map(|cached| CachedStoneInfo {
            endpoint: cached.endpoint,
        })
    }

    fn insert(&self, endpoint: String, capabilities: HardwareCapabilities) {
        let stone_id = capabilities.stone_id.clone();
        let stone_name = capabilities.stone_name.clone();
        StoneCache::insert(self, endpoint, stone_id, stone_name);
    }
}

/// Fetch stone capabilities from endpoint and cache them
pub async fn fetch_and_cache_stone(
    client: &reqwest::Client,
    endpoint: &str,
    cache: &StoneCache,
) -> Result<CachedStone> {
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
    cache.insert(endpoint.to_string(), stone_id, stone_name);

    Ok(CachedStone {
        endpoint: endpoint.to_string(),
        capabilities,
        last_seen: std::time::Instant::now(),
    })
}
