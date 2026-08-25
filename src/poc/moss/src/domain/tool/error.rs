//! Tool domain error types.
//!
//! Most `Tool` commands are infallible against the in-memory registry
//! (upsert/remove always succeed on a `RwLock<GardenRegistryInner>`),
//! so the command methods return `Vec<ToolChanged>` or
//! `Option<ToolChanged>` directly rather than `Result`.
//!
//! The exception is the capability orchestrator
//! ([`super::capability`]), which looks up an offering by name before
//! mutating its sub-capability set. That lookup can fail with
//! `OfferingNotFound`.

use thiserror::Error;

/// Errors returned by fallible `Tool` commands.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Capability mutation requested for an unknown offering.
    #[error("offering '{name}' not found while updating capability set")]
    OfferingNotFound { name: String },
}
