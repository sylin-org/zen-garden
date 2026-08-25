//! Ceremony domain module
//!
//! Orchestrates multi-phase, long-running operations in the garden:
//! - Nourishment (updates) for offerings and stones
//! - Vacate (migration) of offerings between stones
//! - Store (portable backup creation)
//!
//! Ceremonies provide:
//! - Crash recovery via persistent journal
//! - Progress tracking with phase-level granularity
//! - Automatic rollback on failure
//! - Event emission for CLI/UI feedback

mod nourish;
mod phases;
mod registry;
mod types;

pub use nourish::execute_nourish_offering;
pub use registry::CeremonyRegistry;
pub use types::{
    Ceremony, CeremonyId, CeremonyInitiator, CeremonyOptions, CeremonyState, CeremonyType, Phase,
    PhaseState,
};
