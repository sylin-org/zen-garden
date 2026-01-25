//! Centralized archive operations
//!
//! Provides a single, reusable module for all archive concerns:
//! - Creating compressed archives from directories
//! - Extracting archives to directories
//! - Checksum calculation and verification
//! - Archive metadata tracking
//!
//! Used by:
//! - Harvest operations (volume backups before nourishment)
//! - Stored offerings (portable backup packages)
//! - Any future archive needs
//!
//! # Example
//! ```ignore
//! use crate::infra::archive::{Archiver, ArchiveInfo};
//!
//! let archiver = Archiver::new();
//! let info = archiver.create("/source/dir", "/dest/archive.tar.gz").await?;
//! println!("Created archive: {} bytes, checksum: {}", info.size_bytes, info.checksum);
//!
//! // Later, verify and extract
//! archiver.verify(&info).await?;
//! archiver.extract("/dest/archive.tar.gz", "/restore/dir").await?;
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Metadata about a created archive
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveInfo {
    /// Path to the archive file
    pub path: PathBuf,
    /// Archive size in bytes
    pub size_bytes: u64,
    /// Blake3 checksum (format: "blake3:{hex}")
    pub checksum: String,
    /// When the archive was created
    pub created_at: DateTime<Utc>,
}

impl ArchiveInfo {
    /// Format size for human display
    pub fn format_size(&self) -> String {
        format_bytes(self.size_bytes)
    }
}

/// Archive service - centralized archive operations
///
/// Stateless service that provides archive create/extract/verify operations.
/// Use a single instance or create as needed - no shared state.
#[derive(Debug, Clone, Default)]
pub struct Archiver;

impl Archiver {
    /// Create a new archiver instance
    pub fn new() -> Self {
        Self
    }

    /// Create a compressed archive from a source directory
    ///
    /// Returns metadata about the created archive including size and checksum.
    pub async fn create(&self, source: impl AsRef<Path>, dest: impl AsRef<Path>) -> Result<ArchiveInfo> {
        let source = source.as_ref();
        let dest = dest.as_ref();

        create_archive(source, dest).await
    }

    /// Extract an archive to a target directory
    pub async fn extract(&self, archive: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<()> {
        let archive = archive.as_ref();
        let target = target.as_ref();

        extract_archive(archive, target).await
    }

    /// Verify an archive's integrity against its recorded checksum
    pub async fn verify(&self, info: &ArchiveInfo) -> Result<bool> {
        verify_checksum(&info.path, &info.checksum).await
    }

    /// Calculate checksum for a file
    pub async fn checksum(&self, path: impl AsRef<Path>) -> Result<String> {
        calculate_checksum(path.as_ref()).await
    }

    /// Get the size of a directory recursively
    pub async fn directory_size(&self, path: impl AsRef<Path>) -> Result<u64> {
        directory_size(path.as_ref()).await
    }
}

// ============================================================================
// Core Functions (can be used directly for simple cases)
// ============================================================================

/// Create a compressed archive from a source directory
///
/// Creates a .tar.gz archive preserving directory structure.
pub async fn create_archive(source: &Path, dest: &Path) -> Result<ArchiveInfo> {
    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("Failed to create archive directory")?;
    }

    let source_str = source.to_str().context("Invalid source path encoding")?;
    let dest_str = dest.to_str().context("Invalid destination path encoding")?;

    // Get parent directory and directory name for tar
    let parent_dir = source
        .parent()
        .unwrap_or(Path::new("."))
        .to_str()
        .context("Invalid parent path encoding")?;
    let dir_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");

    tracing::debug!(source = %source_str, dest = %dest_str, "Creating archive");

    // Use tar with gzip compression
    let status = Command::new("tar")
        .args(["-czf", dest_str, "-C", parent_dir, dir_name])
        .status()
        .await
        .context("Failed to run tar command")?;

    if !status.success() {
        anyhow::bail!("tar command failed with exit code: {:?}", status.code());
    }

    // Get archive metadata
    let metadata = tokio::fs::metadata(dest)
        .await
        .context("Failed to get archive metadata")?;

    let checksum = calculate_checksum(dest).await?;

    let info = ArchiveInfo {
        path: dest.to_path_buf(),
        size_bytes: metadata.len(),
        checksum,
        created_at: Utc::now(),
    };

    tracing::debug!(
        path = %dest_str,
        size_bytes = info.size_bytes,
        "Archive created successfully"
    );

    Ok(info)
}

/// Extract an archive to a target directory
pub async fn extract_archive(archive: &Path, target: &Path) -> Result<()> {
    // Ensure target directory exists
    tokio::fs::create_dir_all(target)
        .await
        .context("Failed to create target directory")?;

    let archive_str = archive.to_str().context("Invalid archive path encoding")?;
    let target_str = target.to_str().context("Invalid target path encoding")?;

    tracing::debug!(archive = %archive_str, target = %target_str, "Extracting archive");

    let status = Command::new("tar")
        .args(["-xzf", archive_str, "-C", target_str])
        .status()
        .await
        .context("Failed to run tar extract command")?;

    if !status.success() {
        anyhow::bail!("tar extract failed with exit code: {:?}", status.code());
    }

    tracing::debug!(target = %target_str, "Archive extracted successfully");

    Ok(())
}

/// Calculate blake3 checksum of a file
pub async fn calculate_checksum(path: &Path) -> Result<String> {
    let data = tokio::fs::read(path)
        .await
        .context("Failed to read file for checksum")?;

    let hash = blake3::hash(&data);
    Ok(format!("blake3:{}", hash.to_hex()))
}

/// Verify checksum matches expected value
pub async fn verify_checksum(path: &Path, expected: &str) -> Result<bool> {
    let actual = calculate_checksum(path).await?;
    Ok(actual == expected)
}

/// Get the size of a directory recursively
pub async fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;

    if path.is_file() {
        let metadata = tokio::fs::metadata(path).await?;
        return Ok(metadata.len());
    }

    let mut entries = tokio::fs::read_dir(path).await?;
    while let Some(entry) = entries.next_entry().await? {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            total += Box::pin(directory_size(&entry_path)).await?;
        } else {
            let metadata = tokio::fs::metadata(&entry_path).await?;
            total += metadata.len();
        }
    }

    Ok(total)
}

/// Format bytes for human display
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checksum_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        tokio::fs::write(&file_path, b"hello world").await.unwrap();

        let checksum1 = calculate_checksum(&file_path).await.unwrap();
        let checksum2 = calculate_checksum(&file_path).await.unwrap();

        assert_eq!(checksum1, checksum2);
        assert!(checksum1.starts_with("blake3:"));
    }

    #[tokio::test]
    async fn test_verify_checksum() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        tokio::fs::write(&file_path, b"test data").await.unwrap();

        let checksum = calculate_checksum(&file_path).await.unwrap();
        assert!(verify_checksum(&file_path, &checksum).await.unwrap());
        assert!(!verify_checksum(&file_path, "blake3:invalid").await.unwrap());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5_242_880), "5.0 MB");
        assert_eq!(format_bytes(2_147_483_648), "2.0 GB");
    }

    #[tokio::test]
    async fn test_archiver_service() {
        let archiver = Archiver::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        tokio::fs::write(&file_path, b"test").await.unwrap();

        let checksum = archiver.checksum(&file_path).await.unwrap();
        assert!(checksum.starts_with("blake3:"));
    }
}
