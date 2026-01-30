//! Stone Presence Protocol types (PRESENCE-0001)
//!
//! Protocol contracts for SSE communication between Moss and Companions.
//! Contains ONLY data structures, no implementation logic.

pub mod types;
pub mod event_types;

pub use types::{PresenceSnapshot, StoneState, ServiceState, EventFilter, ClientNotification};
