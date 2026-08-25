//! `TopologyStore` port and file-system adapter.
//!
//! Topology is a persistent aggregate (second after Offerings). Ch3 of
//! ARCH-0020 defines the port trait; the `FileTopologyStore` adapter
//! delegates to the existing `persist_topology` free function in
//! `super::mod`, which writes to `{topology_dir}/garden-topology.json`
//! via `garden_common::persistence::atomic_write_file`. TOPO-0002's
//! invariants are preserved unchanged.

use anyhow::Result;
use garden_common::TopologyEntry;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Port for persisting and restoring the topology cache.
pub trait TopologyStore: Send + Sync {
    /// Load the peer cache from persistence. Returns an empty map on
    /// first boot (no file present) rather than an error.
    fn load(&self) -> BoxFut<'_, Result<HashMap<String, TopologyEntry>>>;

    /// Save the full topology set — `self_entry` is written first,
    /// followed by every peer in `entries` (excluding self). Atomic
    /// write via tmp + rename.
    fn save<'a>(
        &'a self,
        entries: &'a HashMap<String, TopologyEntry>,
        self_entry: &'a TopologyEntry,
    ) -> BoxFut<'a, Result<()>>;
}

/// File-system adapter. Reads from and writes to
/// `{topology_dir}/garden-topology.json` via the existing Ch2 helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileTopologyStore;

impl TopologyStore for FileTopologyStore {
    fn load(&self) -> BoxFut<'_, Result<HashMap<String, TopologyEntry>>> {
        Box::pin(async {
            let path = std::path::PathBuf::from(garden_common::constants::paths::topology_dir())
                .join(garden_common::constants::paths::TOPOLOGY_FILE);
            match tokio::fs::read_to_string(&path).await {
                Ok(json) => {
                    let stones: Vec<TopologyEntry> = serde_json::from_str(&json)
                        .map_err(|e| anyhow::anyhow!("parse {}: {}", path.display(), e))?;
                    let mut map = HashMap::new();
                    for stone in stones {
                        map.insert(stone.stone_id.clone(), stone);
                    }
                    Ok(map)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
                Err(e) => Err(anyhow::anyhow!("read {}: {}", path.display(), e)),
            }
        })
    }

    fn save<'a>(
        &'a self,
        entries: &'a HashMap<String, TopologyEntry>,
        self_entry: &'a TopologyEntry,
    ) -> BoxFut<'a, Result<()>> {
        Box::pin(async move {
            // Delegate to the existing module-level helper, which owns
            // the "self-first, then peers, atomic write" invariants
            // inherited from TOPO-0002. Ch5 will absorb the logic
            // directly into this adapter when `persist_topology` is
            // deleted from mod.rs.
            let temp_cache: super::TopologyCache =
                std::sync::Arc::new(tokio::sync::RwLock::new(entries.clone()));
            super::persist_topology(&temp_cache, self_entry).await
        })
    }
}
