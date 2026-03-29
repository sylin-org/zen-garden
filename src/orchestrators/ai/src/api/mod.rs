//! HTTP API layer for the AI Orchestrator.
//!
//! - `proxy`: Ollama-compatible proxy on port 21434.
//! - `health`: Health check endpoint.

pub mod health;
pub mod proxy;
