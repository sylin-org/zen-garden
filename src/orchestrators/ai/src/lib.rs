//! Zen Garden AI Orchestrator — unified capability routing for all AI services.
//!
//! Manages discovery, routing, fitness profiling, demand tracking, and proxy
//! dispatch across heterogeneous AI service types (Ollama, ComfyUI, Speaches,
//! cloud providers, etc.) through an offering adapter pattern.

pub mod api;
pub mod app_state;
pub mod catalog;
pub mod domain;
pub mod infra;
pub mod offerings;
pub mod tasks;
