//! Content store operations trait.
//!
//! Abstracts path-based read/write/delete operations on a mounted
//! storage bank. The concrete `ContentStore` in infra handles
//! encryption, notification, and mount-path resolution transparently.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Path-based content store operations.
///
/// Used by domain and API code to interact with storage banks
/// without depending on the concrete `ContentStore` in infra.
/// The implementor handles encryption and replication notifications.
#[async_trait]
pub trait ContentStoreOps: Send + Sync {
    /// Read binary data from a relative path.
    async fn read(&self, rel: &Path) -> Result<Vec<u8>>;

    /// Write binary data to a relative path.
    async fn write(&self, rel: &Path, data: &[u8]) -> Result<()>;

    /// Read UTF-8 text from a relative path.
    async fn read_string(&self, rel: &Path) -> Result<String>;

    /// Write UTF-8 text to a relative path.
    async fn write_string(&self, rel: &Path, data: &str) -> Result<()>;

    /// Delete a file at a relative path. Returns true if file existed.
    async fn delete(&self, rel: &Path) -> Result<bool>;

    /// Check if a file exists at a relative path.
    async fn exists(&self, rel: &Path) -> bool;
}
