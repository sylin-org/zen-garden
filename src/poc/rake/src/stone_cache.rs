use crate::connection::resolution::{CachedStoneInfo, CachedStoneOps};
use anyhow::Result;
use garden_common::client::StoneApi;
use garden_common::HardwareCapabilities;
use garden_discovery::cache::{Discovery, KnownStone};
use std::sync::{Arc, LazyLock};

pub use garden_discovery::cache::STONE;

/// Shared `Arc` wrapper around a `Discovery` instance for use with `Resilient`.
///
/// This is a separate instance from `STONE` (the process singleton), but
/// functionally equivalent for recovery caching in resilient connections.
pub static STONE_ARC: LazyLock<Arc<Discovery>> = LazyLock::new(|| Arc::new(Discovery::new()));

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
    let api = StoneApi::new(client.clone(), endpoint.to_string());
    let capabilities = api.stone().capabilities_core().await?;

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
