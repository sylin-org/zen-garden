//! Content store operations trait.
//!
//! Abstracts path-based read/write/delete operations on a mounted
//! storage bank. The concrete `ContentStore` in infra handles
//! encryption, notification, and mount-path resolution transparently.

use anyhow::Result;
use std::future::Future;
use std::path::Path;

/// Path-based content store operations.
///
/// Used by domain and API code to interact with storage banks
/// without depending on the concrete `ContentStore` in infra.
/// The implementor handles encryption and replication notifications.
pub trait ContentStoreOps: Send + Sync {
    /// Read binary data from a relative path.
    fn read(&self, rel: &Path) -> impl Future<Output = Result<Vec<u8>>> + Send;

    /// Write binary data to a relative path.
    fn write(&self, rel: &Path, data: &[u8]) -> impl Future<Output = Result<()>> + Send;

    /// Read UTF-8 text from a relative path.
    fn read_string(&self, rel: &Path) -> impl Future<Output = Result<String>> + Send;

    /// Write UTF-8 text to a relative path.
    fn write_string(&self, rel: &Path, data: &str) -> impl Future<Output = Result<()>> + Send;

    /// Delete a file at a relative path. Returns true if file existed.
    fn delete(&self, rel: &Path) -> impl Future<Output = Result<bool>> + Send;

    /// Check if a file exists at a relative path.
    fn exists(&self, rel: &Path) -> impl Future<Output = bool> + Send;
}
