//! HTTP handlers.
//!
//! The proxy handler dispatches to offerings via capability routing.
//! Extension, management, and dashboard handlers operate on domain state.

pub mod extension;
pub mod health;
pub mod proxy;

// Phase 2+:
// pub mod benchmark_api;
// pub mod dashboard;
// pub mod management;
// pub mod providers_api;
