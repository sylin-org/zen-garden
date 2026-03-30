//! Zen Garden AI Orchestrator — multi-offering AI service orchestration.
//!
//! Architecture: one `Provider` trait per AI service type. Each provider
//! covers lifecycle (probe, enumerate) and inference (infer, stream,
//! embed, speak, transcribe). Protocol-specific clients live in `offerings/`.

pub mod api;
pub mod app_state;
pub mod catalog;
pub mod domain;
pub mod infra;
pub mod offerings;
pub mod providers;
pub mod tasks;

pub use app_state::AppState;
