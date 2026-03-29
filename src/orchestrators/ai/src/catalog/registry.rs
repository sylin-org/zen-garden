//! OfferingRegistry — stores all registered offering adapters.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};

use crate::domain::types::OfferingKind;

use super::traits::Offering;

/// Central registry of all offering adapters.
///
/// Populated at startup (one `Arc<dyn Offering>` per offering type).
/// Immutable after initialization — offerings are not added or removed
/// at runtime.
pub struct OfferingRegistry {
    offerings: HashMap<OfferingKind, Arc<dyn Offering>>,
}

impl OfferingRegistry {
    pub fn new() -> Self {
        Self {
            offerings: HashMap::new(),
        }
    }

    /// Register an offering adapter. Validates OC-1 (unique type) and
    /// OC-2 (non-empty capabilities).
    pub fn register(&mut self, offering: Arc<dyn Offering>) -> Result<()> {
        let kind = offering.offering_type();

        if self.offerings.contains_key(&kind) {
            bail!("duplicate offering type: {kind}");
        }

        if offering.capabilities().is_empty() {
            bail!("offering {kind} declares no capabilities");
        }

        self.offerings.insert(kind, offering);
        Ok(())
    }

    /// Get the adapter for a specific offering type.
    pub fn get(&self, kind: OfferingKind) -> Option<&Arc<dyn Offering>> {
        self.offerings.get(&kind)
    }

    /// All registered offering types.
    pub fn kinds(&self) -> impl Iterator<Item = OfferingKind> + '_ {
        self.offerings.keys().copied()
    }

    /// All registered adapters.
    pub fn all(&self) -> impl Iterator<Item = &Arc<dyn Offering>> {
        self.offerings.values()
    }

    /// Number of registered offerings.
    pub fn len(&self) -> usize {
        self.offerings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offerings.is_empty()
    }
}
