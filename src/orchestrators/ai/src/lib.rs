//! Zen Garden AI Orchestrator — multi-offering AI service orchestration.
//!
//! Architecture: monolith of bounded contexts. Each offering type is a
//! self-contained adapter implementing the `Offering` trait. Shared
//! infrastructure (routing, demand, fitness, placement) operates on
//! generic `ServiceInstance` and `Capability` types.

pub mod catalog;
pub mod domain;
