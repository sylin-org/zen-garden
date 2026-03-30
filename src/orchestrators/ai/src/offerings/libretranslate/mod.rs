//! LibreTranslate protocol layer — HTTP client and types.
//!
//! The Offering trait impl has moved to `providers/libretranslate.rs`.
//! This module exposes only the client and types needed by other modules.

pub mod client;
pub mod types;

pub use client::LibreTranslateClient;
