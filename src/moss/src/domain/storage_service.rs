//! Storage routing — re-export from canonical location.
//!
//! The routing types moved to `domain/storage/routing.rs` in
//! Book VIII (ARCH-0025). This module re-exports for backward compatibility.

pub use crate::domain::storage::routing::{LocalStorage, ProxyTarget, StorageRoute};
