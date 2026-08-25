//! Event Listeners - Consumers of domain events
//!
//! This module contains listeners that process domain events:
//! - ChirpListener: UDP topology broadcasts (offering events)
//! - PulseDomainBridge: Bridges domain events into the unified pulse channel

mod chirp;
mod pulse;

pub use chirp::ChirpListener;
pub use pulse::{DomainPulse, PulseDomainBridge, PulseEvent, TransportPulse, spawn_transport_tap};

/// Listener names for logging/debugging (used by EventListener::name())
pub mod names {
    pub const CHIRP: &str = "chirp";
    pub const PULSE: &str = "pulse";
}
