//! Companion domain — companion registry lifecycle.

use std::sync::Arc;

/// Companion domain context (`state.companion`).
///
/// Owns the companion registry that manages external companions
/// (Cricket, Firefly, and other hardware companions).
#[derive(Clone)]
pub struct Companion {
    /// Registry of all discovered and running companions.
    pub registry: Arc<crate::infra::CompanionRegistry>,
}
