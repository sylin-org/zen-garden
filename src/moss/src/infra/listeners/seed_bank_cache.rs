//! Seed Bank Cache Listener
//!
//! Updates the seed bank cache in AppState when storage events occur.
//! This ensures the portrait endpoint always has fresh data without doing I/O.

use crate::domain::events::{DomainEvent, StorageEvent};
use crate::infra::event_bus::EventListener;
use crate::infra::storage::SeedBankRegistry;
use garden_common::storage::SeedBankInfo;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Listener that updates seed bank cache on storage events
pub struct SeedBankCacheListener {
    cache: Arc<RwLock<Vec<SeedBankInfo>>>,
}

impl SeedBankCacheListener {
    pub fn new(cache: Arc<RwLock<Vec<SeedBankInfo>>>) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl EventListener for SeedBankCacheListener {
    fn name(&self) -> &'static str {
        super::names::SEED_BANK_CACHE
    }

    async fn on_event(&self, event: &DomainEvent) {
        match event {
            DomainEvent::Storage(storage_event) => {
                match storage_event {
                    StorageEvent::SeedBankDetected { name, device, mount_path, capacity_gb, .. } => {
                        // Re-scan registry to get full SeedBankInfo
                        // This is acceptable here because it's event-driven, not on every portrait request
                        match SeedBankRegistry::scan().await {
                            Ok(registry) => {
                                let banks: Vec<SeedBankInfo> = registry.list().into_iter().cloned().collect();
                                let count = banks.len();
                                let mut cache = self.cache.write().await;
                                *cache = banks;
                                debug!(
                                    name = %name,
                                    device = %device,
                                    mount_path = %mount_path,
                                    capacity_gb = %capacity_gb,
                                    total_banks = count,
                                    "Seed bank cache updated after detection"
                                );
                            }
                            Err(e) => {
                                warn!(error = ?e, "Failed to refresh seed bank cache after detection");
                            }
                        }
                    }
                    StorageEvent::SeedBankRemoved { name, device, .. } => {
                        // Remove the seed bank from cache by name
                        let mut cache = self.cache.write().await;
                        let before_count = cache.len();
                        cache.retain(|b| b.name != *name && b.device != *device);
                        let after_count = cache.len();
                        debug!(
                            name = %name,
                            device = %device,
                            removed = before_count - after_count,
                            remaining = after_count,
                            "Seed bank cache updated after removal"
                        );
                    }
                    // Sync events don't affect the cache structure
                    StorageEvent::SyncStarted { .. } | StorageEvent::SyncCompleted { .. } => {}
                }
            }
            // Ignore non-storage events
            _ => {}
        }
    }
}
