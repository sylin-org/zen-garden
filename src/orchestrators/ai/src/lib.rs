//! Zen Garden AI Orchestrator — multi-offering AI service orchestration.
//!
//! Architecture: monolith of bounded contexts. Each offering type is a
//! self-contained adapter implementing the `Offering` trait. Shared
//! infrastructure (routing, demand, fitness, placement) operates on
//! generic `ServiceInstance` and `Capability` types.

pub mod api;
pub mod app_state;
pub mod catalog;
pub mod domain;
pub mod infra;
pub mod offerings;
pub mod tasks;

pub use app_state::AppState;
