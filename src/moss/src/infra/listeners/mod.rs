//! Event Listeners - Consumers of domain events
//!
//! This module contains listeners that process domain events:
//! - ChirpListener: UDP topology broadcasts (offering events)
//! - SseListener: Real-time client events (all domain events)
//! - TimerListener: Nurturing schedule management
//! - TimerExecutor: Direct timer management for testing/admin
//! - SeedBankCacheListener: Updates seed bank cache on storage events

mod chirp;
mod seed_bank_cache;
mod sse;
mod timer;

pub use chirp::ChirpListener;
pub use seed_bank_cache::SeedBankCacheListener;
pub use sse::{SseEvent, SseListener};
pub use timer::{TimerAction, TimerExecutor, TimerListener};

/// Listener names for logging/debugging (used by EventListener::name())
pub mod names {
    pub const CHIRP: &str = "chirp";
    pub const SSE: &str = "sse";
    pub const TIMER: &str = "timer";
    pub const SEED_BANK_CACHE: &str = "seed_bank_cache";
}
