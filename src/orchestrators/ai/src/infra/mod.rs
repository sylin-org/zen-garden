//! Infrastructure layer — I/O adapters and persistence.
//!
//! Uses `orchestrator_common::` for shared concerns (Koi, topology, gateway,
//! persistence, events, dashboard). Only AI-orchestrator-specific wiring here.

pub mod events;
pub mod persistence;
