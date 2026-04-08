//! Zen Garden AI Orchestrator — ORCH-0028 core.
//!
//! The orchestrator is a single pipeline fed by provider implementations
//! registered at startup. See [`docs/decisions/ORCH-0028-orchestrator-core.md`]
//! for the decision record.
//!
//! Module layout:
//!
//! - [`domain`] — aggregates, value objects, vocabulary, canonical keys.
//! - [`services`] — stateless orchestration services (contextualizer,
//!   media resolver, dispatcher, recommendation).
//! - [`providers`] — concrete provider implementations, one per vendor.
//! - [`http`] — axum handlers, routers, and request/response envelopes.
//! - [`app_state`] — the top-level state container shared across handlers
//!   and background tasks.

pub mod app_state;
pub mod domain;
pub mod http;
pub mod providers;
pub mod services;

pub use app_state::AppState;
