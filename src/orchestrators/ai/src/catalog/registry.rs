//! [`OfferingRegistry`] — runtime catalog of registered offering adapters.

use std::sync::Arc;

use crate::domain::types::OfferingKind;

use super::traits::Offering;

/// Registry of all offering adapters available to the orchestrator.
///
/// Stored in [`AppState`](crate::app_state::AppState) and shared across tasks
/// and API handlers. Immutable after startup — offerings are registered during
/// initialization and never added or removed at runtime.
pub struct OfferingRegistry {
    offerings: Vec<Arc<dyn Offering>>,
}

impl OfferingRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            offerings: Vec::new(),
        }
    }

    /// Register an offering adapter.
    ///
    /// # Panics
    ///
    /// Panics if an offering with the same [`OfferingKind`] is already
    /// registered (OC-1 invariant).
    pub fn register(&mut self, offering: Arc<dyn Offering>) {
        let kind = offering.offering_type();
        assert!(
            !self.offerings.iter().any(|o| o.offering_type() == kind),
            "Duplicate offering type: {kind:?}"
        );
        self.offerings.push(offering);
    }

    /// Look up an offering by kind.
    pub fn get(&self, kind: OfferingKind) -> Option<&Arc<dyn Offering>> {
        self.offerings.iter().find(|o| o.offering_type() == kind)
    }

    /// Iterate all registered offerings.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Offering>> {
        self.offerings.iter()
    }

    /// Number of registered offerings.
    pub fn len(&self) -> usize {
        self.offerings.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.offerings.is_empty()
    }
}

impl Default for OfferingRegistry {
    fn default() -> Self {
        Self::new()
    }
}
