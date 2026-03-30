//! Infinity protocol layer — HTTP client and types.
//!
//! The Offering trait impl has moved to `providers/infinity.rs`.
//! This module exposes only the client and types needed by other modules.

pub mod client;
pub mod types;

pub use client::InfinityClient;
