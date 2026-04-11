//! Pond domain — enrollment state and certmesh CA lifecycle.

use super::ceremony::Ceremony;
use crate::domain::PondState;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Pond domain context (`state.security.pond`).
#[derive(Clone)]
pub struct Pond {
    /// Enrollment state and cornerstone identity.
    /// Mutations trigger `PondEvent::EnrollmentChanged` on the EventBus.
    pub state: PondState,

    /// True when the certmesh CA is initialized and unlocked.
    /// Cached for fast checks (chirp signing, HTTPS routing).
    pub active: Arc<AtomicBool>,

    /// Ceremony coordination infrastructure.
    pub ceremony: Ceremony,
}
