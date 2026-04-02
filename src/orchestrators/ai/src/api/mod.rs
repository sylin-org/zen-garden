//! HTTP API layer for the AI Orchestrator.
//!
//! - `proxy`: Ollama-compatible proxy on port 21434.
//! - `health`: Health check endpoint.

pub mod dashboard;
pub mod generic_proxy;
pub mod health;
pub mod provider_test;
pub mod proxy;
pub mod service_actions;
pub mod skill_manage;
pub mod static_files;
pub mod unified;
pub mod workflows;
