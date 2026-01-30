//! Event Listeners - Consumers of offering lifecycle events
//!
//! This module contains listeners that process OfferingEvents:
//! - ChirpListener: UDP topology broadcasts
//! - SseListener: Real-time client events
//! - TimerListener: Nurturing schedule management

mod chirp;
mod sse;
mod timer;

pub use chirp::ChirpListener;
pub use sse::{SseEvent, SseListener};
pub use timer::TimerListener;
