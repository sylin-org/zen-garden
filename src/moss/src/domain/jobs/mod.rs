//! Jobs bounded context — Book IV of [ARCH-0017].
//!
//! ## Chapter 3 state
//!
//! `Jobs` is a full DDD aggregate with typed commands, typed queries,
//! an internal `JobsChanged` event stream, `Arc<Metrics>` injection,
//! and a parallel wire-format `JobEvent` stream emitted via the
//! existing `EventBus`. State is an `Arc<RwLock<HashMap<String, Job>>>`
//! shared with the legacy `AppState::jobs` field during the strangler
//! phase — both views see the same map. Ch4 migrates executor sites
//! to typed commands; Ch5 migrates the remaining sites, deletes the
//! legacy field, and wires the `JobsReaperTask` onto the supervisor.
//!
//! ## Pattern deviations
//!
//! - **Ephemeral** — no `JobStore` port, no persistence. Matches
//!   Metrics (Book I) and Tool (Book II).
//! - **Dual event streams** — `changes()` internal `JobsChanged` +
//!   preserved wire `JobEvent` via `EventBus`. Matches Book II
//!   `ToolDelta` precedent.
//! - **Infallible mutations** — no `JobsError`, commands return `()`
//!   (or a value). No-op on unknown ids with a warn-level trace.
//!   Matches Book I `Metrics` (no `MetricsError`).
//!
//! All three are first-class in `docs/specs/domain-aggregates.md`.
//!
//! [ARCH-0017]: ../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

pub mod aggregate;
pub mod entry;
pub mod event;
pub mod maintenance;
pub mod state;

#[cfg(test)]
mod tests;

pub use aggregate::Jobs;
pub use entry::{Job, JobStatus};
pub use event::{ChangeKind as JobsChangeKind, EvictionReason, JobsChanged};
pub use maintenance::{DEFAULT_TERMINAL_TTL, ReapReport};
