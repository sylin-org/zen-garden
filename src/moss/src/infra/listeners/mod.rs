//! Event Listeners - Consumers of domain events
//!
//! This module contains listeners that process domain events:
//! - ChirpListener: UDP topology broadcasts (offering events)
//! - PulseDomainBridge: Bridges domain events into the unified pulse channel
//! - TimerListener: Nurturing schedule management
//! - TimerExecutor: Direct timer management for testing/admin

mod chirp;
mod pulse;
mod timer;

pub use chirp::ChirpListener;
pub use pulse::{DomainPulse, PulseDomainBridge, PulseEvent, TransportPulse, spawn_transport_tap};
pub use timer::{TimerAction, TimerExecutor, TimerListener};

/// Listener names for logging/debugging (used by EventListener::name())
pub mod names {
    pub const CHIRP: &str = "chirp";
    pub const PULSE: &str = "pulse";
    pub const TIMER: &str = "timer";
}
