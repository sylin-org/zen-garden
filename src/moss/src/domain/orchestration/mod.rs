//! Orchestration domain — coordination plane (ARCH-0004)
//!
//! Owns the coordination primitives that drive Moss's internal loops.
//! Does not own data; data lives in the domain contexts it coordinates.
//!
//! Sub-namespaces:
//! - [`storage`]     — volume lifecycle signals (tick, nudge, rescan)
//! - [`nurturing`]   — A/B backup scheduling and harvest archives
//! - [`nourishment`] — update job SSE channels

pub mod nourishment;
pub mod nurturing;
pub mod storage;

pub use nourishment::NourishmentOrchestration;
pub use nurturing::NurturingOrchestration;
pub use storage::StorageOrchestration;

/// Cross-domain coordination plane.
///
/// Held as `AppState.orchestration: Arc<Orchestration>`.
/// Field path: `state.orchestration.{storage|nurturing|nourishment}.*`
#[derive(Clone)]
pub struct Orchestration {
    pub storage: StorageOrchestration,
    pub nurturing: NurturingOrchestration,
    pub nourishment: NourishmentOrchestration,
}
