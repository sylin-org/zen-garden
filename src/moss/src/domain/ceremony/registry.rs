//! Ceremony registry - in-memory tracking of active ceremonies
//!
//! Simple thread-safe map for tracking ceremony state during execution.
//! Ceremonies are persisted separately via CeremonyJournal for crash recovery.

use super::types::{Ceremony, CeremonyId};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Thread-safe registry for active ceremonies
pub struct CeremonyRegistry {
    ceremonies: RwLock<HashMap<CeremonyId, Ceremony>>,
}

impl CeremonyRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            ceremonies: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a ceremony and return its ID
    pub async fn insert(&self, ceremony: Ceremony) -> CeremonyId {
        let id = ceremony.id.clone();
        self.ceremonies.write().await.insert(id.clone(), ceremony);
        id
    }

    /// Get a ceremony by ID
    pub async fn get(&self, id: &CeremonyId) -> Option<Ceremony> {
        self.ceremonies.read().await.get(id).cloned()
    }

    /// Update an existing ceremony
    pub async fn update(&self, ceremony: Ceremony) {
        self.ceremonies
            .write()
            .await
            .insert(ceremony.id.clone(), ceremony);
    }

    /// Remove a ceremony
    pub async fn remove(&self, id: &CeremonyId) -> Option<Ceremony> {
        self.ceremonies.write().await.remove(id)
    }

    /// List all active (non-terminal) ceremonies
    pub async fn list_active(&self) -> Vec<Ceremony> {
        self.ceremonies
            .read()
            .await
            .values()
            .filter(|c| c.state.is_active())
            .cloned()
            .collect()
    }

    /// List all ceremonies
    pub async fn list_all(&self) -> Vec<Ceremony> {
        self.ceremonies.read().await.values().cloned().collect()
    }

    /// Check if any ceremony is active for a given offering
    pub async fn has_active_for_offering(&self, offering: &str) -> bool {
        self.ceremonies.read().await.values().any(|c| {
            c.state.is_active()
                && c.ceremony_type
                    .target()
                    .map(|t| t == offering)
                    .unwrap_or(false)
        })
    }

    /// Get count of active ceremonies
    pub async fn active_count(&self) -> usize {
        self.ceremonies
            .read()
            .await
            .values()
            .filter(|c| c.state.is_active())
            .count()
    }
}

impl Default for CeremonyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ceremony::{CeremonyInitiator, CeremonyOptions, CeremonyType};

    fn test_ceremony(offering: &str) -> Ceremony {
        Ceremony::new(
            CeremonyType::NourishOffering {
                offering: offering.to_string(),
            },
            "stone-01".to_string(),
            CeremonyInitiator {
                source: "test".to_string(),
                stone_id: None,
                command: None,
            },
            CeremonyOptions::default(),
        )
    }

    #[tokio::test]
    async fn test_registry_insert_get() {
        let registry = CeremonyRegistry::new();
        let ceremony = test_ceremony("mongodb");
        let id = ceremony.id.clone();

        registry.insert(ceremony).await;

        let retrieved = registry.get(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_registry_list_active() {
        let registry = CeremonyRegistry::new();

        let mut c1 = test_ceremony("mongodb");
        c1.start();
        registry.insert(c1).await;

        let mut c2 = test_ceremony("redis");
        c2.complete();
        registry.insert(c2).await;

        let active = registry.list_active().await;
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn test_has_active_for_offering() {
        let registry = CeremonyRegistry::new();

        let mut c1 = test_ceremony("mongodb");
        c1.start();
        registry.insert(c1).await;

        assert!(registry.has_active_for_offering("mongodb").await);
        assert!(!registry.has_active_for_offering("redis").await);
    }
}
