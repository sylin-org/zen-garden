//! Persistence abstraction for jobs and configuration
//!
//! Uses JSON file storage with atomic writes to prevent corruption.
//! All writes go through: write to .tmp → sync → rename (atomic on POSIX, best-effort on Windows)

use serde::{Deserialize, Serialize};

/// Persistence errors
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("Failed to read from {path}: {source}")]
    ReadFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to write to {path}: {source}")]
    WriteFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to serialize data: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("File not found: {0}")]
    NotFound(String),

    #[error("Corrupted data in {path}: {reason}")]
    CorruptedData { path: String, reason: String },
}

/// Generic persistence provider for JSON-serializable data
pub trait PersistenceProvider<T>: Send + Sync
where
    T: Serialize + for<'de> Deserialize<'de> + Send,
{
    /// Load data from storage
    ///
    /// Returns None if file doesn't exist, Error if corrupted
    fn load(&self)
    -> impl std::future::Future<Output = Result<Option<T>, PersistenceError>> + Send;

    /// Save data to storage atomically
    ///
    /// Uses atomic file write pattern to prevent corruption
    fn save(
        &self,
        data: &T,
    ) -> impl std::future::Future<Output = Result<(), PersistenceError>> + Send;

    /// Delete storage file
    fn delete(&self) -> impl std::future::Future<Output = Result<(), PersistenceError>> + Send;

    /// Check if storage file exists
    fn exists(&self) -> impl std::future::Future<Output = bool> + Send;
}
