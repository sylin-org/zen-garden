//! The room's wire plane: presence, ingestion, dispatch, announcement
//! (the `room` context's transport; ADR-0015).
//!
//! Shape (CODE-RULES R2.8/R2.9): listeners feed ONE ingestion point; the
//! ingress parses, dedups, and hands every datagram to the dispatcher;
//! handlers register for the types they claim and pull from their own
//! bounded queues. Garden-wide state lives in ONE hot topology cache;
//! domain state moves by events, never by polling (L18, L22).

pub mod announce;
pub mod config;
pub mod dispatch;
pub mod ingress;
pub mod pipeline;
pub mod probe;
pub mod responder;
pub mod topology;
