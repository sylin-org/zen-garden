//! Stone Presence Protocol types (PRESENCE-0001)
//!
//! Protocol contracts for SSE communication between Moss and Companions.
//! Contains ONLY data structures, no implementation logic.

pub mod event_types;
pub mod types;

pub use types::{
    ClientNotification, EventFilter, OfferingState, PresenceSnapshot, SeedBankSummary,
    StoneHealthChangedPayload, StoneLoadUpdatedPayload, StoneState,
};
