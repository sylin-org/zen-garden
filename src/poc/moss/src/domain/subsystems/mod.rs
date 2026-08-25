//! Subsystems bounded context — Book VI of [ARCH-0017].
//!
//! Replaces the `SubSystems` struct of `Arc<AtomicBool>` fields with
//! a registration-based aggregate backed by `tokio::sync::watch`
//! channels. Subsystems are registered by name at bootstrap; monitor
//! tasks toggle readiness via typed commands; consumer tasks/handlers
//! poll via typed queries.
//!
//! ## Pattern deviations
//!
//! - **Ephemeral** — no persistence port, no store. Matches Metrics
//!   (Book I) and Jobs (Book IV).
//! - **Infallible mutations** — no `SubsystemsError`. Warn-level
//!   no-op on unknown names. Matches Metrics and Jobs.
//! - **No internal `RwLock`** — `watch::Sender` is inherently
//!   thread-safe; the `HashMap` is frozen after registration.
//!
//! [ARCH-0017]: ../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

pub mod aggregate;
pub mod event;

#[cfg(test)]
mod tests;

pub use aggregate::{SubsystemStatus, Subsystems};
pub use event::{ChangeKind as SubsystemsChangeKind, SubsystemsChanged};
