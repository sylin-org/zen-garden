//! HTTP handlers.
//!
//! The proxy handler dispatches to offerings via capability routing.
//! Extension, management, and dashboard handlers operate on domain state.

pub mod compat;
pub mod extension;
pub mod health;
pub mod proxy;

// Future:
// pub mod benchmark_api;
// pub mod dashboard;
// pub mod management;
// pub mod providers_api;
