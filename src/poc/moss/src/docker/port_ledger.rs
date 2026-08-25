//! Persisted host-port allocations for managed offerings.
//!
//! Once an offering's container port is published on a host port — possibly
//! remapped from the manifest default to avoid a collision — that allocation is
//! recorded here and reused on every subsequent (re)deploy, so the bound port
//! stays stable across restarts and upgrades. Without this, port resolution is
//! recomputed from live state each deploy and a service can silently move ports.
//!
//! Best-effort: a load/save failure logs and falls back to recomputing from the
//! manifest — it never blocks a deploy.

use garden_common::constants::paths::data_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::warn;

const LEDGER_FILE: &str = "offering-ports.json";

/// Serializes the load → resolve → save cycle across concurrent deploys so two
/// containers can't race onto the same host port (a TOCTOU on bind-probing that
/// existed before persistence). Held for the duration of a single offering's
/// port resolution.
pub(crate) static ALLOCATION_LOCK: Mutex<()> = Mutex::const_new(());

/// Offering name → (container_port → allocated host_port).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct OfferingPortLedger {
    #[serde(default)]
    allocations: HashMap<String, HashMap<u16, u16>>,
}

fn ledger_path() -> PathBuf {
    Path::new(&data_dir()).join(LEDGER_FILE)
}

impl OfferingPortLedger {
    /// Load the ledger from `{data_dir}/offering-ports.json`. Any error (missing,
    /// unreadable, malformed) yields an empty ledger.
    pub(crate) async fn load() -> Self {
        Self::load_from(&ledger_path()).await
    }

    async fn load_from(path: &Path) -> Self {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                warn!(error = %e, "Failed to parse offering port ledger; starting fresh");
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                warn!(error = %e, "Failed to read offering port ledger; starting fresh");
                Self::default()
            }
        }
    }

    /// Persist atomically. Best-effort: logs on failure.
    pub(crate) async fn save(&self) {
        self.save_to(&ledger_path()).await;
    }

    async fn save_to(&self, path: &Path) {
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => {
                if let Err(e) = garden_common::persistence::atomic_write_file(path, &bytes).await {
                    warn!(error = %e, "Failed to persist offering port ledger");
                }
            }
            Err(e) => warn!(error = %e, "Failed to serialize offering port ledger"),
        }
    }

    /// Previously-allocated `container_port → host_port` map for an offering.
    pub(crate) fn prior(&self, offering: &str) -> Option<&HashMap<u16, u16>> {
        self.allocations.get(offering)
    }

    /// Record (replacing) the resolved allocation for an offering. Replacing the
    /// whole map drops entries for container ports the manifest no longer maps.
    pub(crate) fn set(&mut self, offering: &str, allocation: HashMap<u16, u16>) {
        self.allocations.insert(offering.to_string(), allocation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrips_allocations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("offering-ports.json");

        let mut ledger = OfferingPortLedger::default();
        ledger.set("postgresql", HashMap::from([(5432u16, 5433u16)]));
        ledger.set("flaresolverr", HashMap::from([(8191u16, 8192u16)]));
        ledger.save_to(&path).await;

        let loaded = OfferingPortLedger::load_from(&path).await;
        assert_eq!(loaded.prior("postgresql").unwrap().get(&5432), Some(&5433));
        assert_eq!(loaded.prior("flaresolverr").unwrap().get(&8191), Some(&8192));
        assert!(loaded.prior("unknown").is_none());
    }

    #[tokio::test]
    async fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = OfferingPortLedger::load_from(&dir.path().join("nope.json")).await;
        assert!(ledger.prior("anything").is_none());
    }

    #[tokio::test]
    async fn set_replaces_whole_offering_map() {
        let mut ledger = OfferingPortLedger::default();
        ledger.set("svc", HashMap::from([(80u16, 8080u16), (443u16, 8443u16)]));
        // Manifest later drops the 443 mapping.
        ledger.set("svc", HashMap::from([(80u16, 8080u16)]));
        let prior = ledger.prior("svc").unwrap();
        assert_eq!(prior.get(&80), Some(&8080));
        assert!(prior.get(&443).is_none());
    }
}
