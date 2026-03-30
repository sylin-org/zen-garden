//! Cloud provider types — configuration and persistence.
//!
//! The per-provider Offering/InferenceAdapter impls have moved to `providers/`.
//! This module exposes only the types needed by cloud_sync and other consumers.

pub mod types;

pub use types::{CloudProviderConfig, CloudProviderStore};
