//! Storage platform abstraction — re-export from canonical location.
//!
//! The `StoragePlatform` trait moved to `domain/storage/ports.rs` in
//! Book VIII (ARCH-0025). This module re-exports for backward compatibility.

pub use crate::domain::storage::ports::StoragePlatform;
