//! Orchestration domain — coordination plane (ARCH-0004, ARCH-0029)
//!
//! The `Orchestration` wrapper struct was dissolved in ARCH-0029 (Book XI).
//! Storage coordination primitives moved to `domain::storage::Coordination`.
//! Nurturing and Nourishment remain as thin infrastructure structs promoted
//! to direct `Moss` fields.
//!
//! Sub-namespaces:
//! - [`nurturing`]   — A/B backup scheduling and harvest archives
//! - [`nourishment`] — update job SSE channels

pub mod nourishment;
pub mod nurturing;

pub use nourishment::NourishmentOrchestration;
pub use nurturing::NurturingOrchestration;
