//! Shared infrastructure for Zen Garden orchestrators.
//!
//! Provides stone discovery, gateway registration, tools stream subscription,
//! tending state persistence, and dashboard SSE helpers — common to all
//! orchestrators (Ollama, MongoDB, future).

pub mod dashboard;
pub mod discovery;
pub mod events;
pub mod gateway;
pub mod http;
pub mod persistence;
pub mod resilient_stream;
pub mod stone_catalog;
pub mod tasks;
pub mod tools_stream;
pub mod topology;
