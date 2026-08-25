//! File system utilities
//!
//! Helpers for directory creation, path operations, and file I/O
//! with consistent error handling and logging.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Ensure directory exists (sync version)
pub fn ensure_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Ensure directory exists (async version)
pub async fn ensure_dir_async<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    tokio::fs::create_dir_all(path)
        .await
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Ensure parent directory exists
pub fn ensure_parent_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

/// Ensure parent directory exists (async)
pub async fn ensure_parent_dir_async<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        ensure_dir_async(parent).await?;
    }
    Ok(())
}

/// Safe path joining (normalizes separators)
pub fn join_path<P: AsRef<Path>>(base: P, parts: &[&str]) -> PathBuf {
    let mut path = base.as_ref().to_path_buf();
    for part in parts {
        path = path.join(part);
    }
    path
}

/// Convert path to string with lossy conversion
pub fn path_to_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().to_string_lossy().to_string()
}

/// Read file with context (sync)
pub fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Read file with context (async)
pub async fn read_file_async<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Write file with parent directory creation (sync)
pub fn write_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();
    ensure_parent_dir(path)?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Write file with parent directory creation (async)
pub async fn write_file_async<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();
    ensure_parent_dir_async(path).await?;
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_path() {
        let path = join_path("/var/lib", &["zen-garden", "data", "test.json"]);
        assert!(path.to_string_lossy().contains("zen-garden"));
        assert!(path.to_string_lossy().contains("test.json"));
    }

    #[test]
    fn test_path_to_string() {
        let path = PathBuf::from("/tmp/test");
        let s = path_to_string(path);
        assert!(s.contains("test"));
    }

    #[test]
    fn test_ensure_parent_dir() {
        let temp_dir = std::env::temp_dir().join("zen-test-ensure-parent");
        let test_file = temp_dir.join("subdir").join("test.txt");

        // Ensure parent should create the parent directory
        ensure_parent_dir(&test_file).expect("Should create parent dir");
        assert!(test_file.parent().unwrap().exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_ensure_dir_async() {
        let temp_dir = std::env::temp_dir().join("zen-test-async-dir");

        ensure_dir_async(&temp_dir)
            .await
            .expect("Should create dir");
        assert!(temp_dir.exists());

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_write_and_read_async() {
        let temp_dir = std::env::temp_dir().join("zen-test-write-read");
        let test_file = temp_dir.join("nested").join("test.txt");
        let content = "test content";

        write_file_async(&test_file, content)
            .await
            .expect("Should write file");
        assert!(test_file.exists());

        let read_content = read_file_async(&test_file).await.expect("Should read file");
        assert_eq!(read_content, content);

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
