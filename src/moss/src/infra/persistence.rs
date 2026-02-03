//! Persistence layer - disk I/O for offerings, cache, etc.
//!
//! Composable functions for atomic file operations with proper error handling.
//! All persistence uses temp file + rename for atomic writes.

use anyhow::Result;
use std::path::PathBuf;

/// Get offerings cache file path
fn offerings_cache_path() -> PathBuf {
    PathBuf::from(garden_common::names::CONFIG_DIR).join("offerings_cache.json")
}

// ============================================================================
// Offerings Persistence
// ============================================================================

/// Load offerings from disk
///
/// Returns empty vec if file doesn't exist.
pub async fn load_offerings() -> Result<Vec<garden_common::Offering>> {
    let path = PathBuf::from(garden_common::names::CONFIG_DIR).join("moss-offerings.json");

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => {
            let offerings: Vec<garden_common::Offering> = serde_json::from_str(&content)?;
            tracing::info!(count = offerings.len(), "Loaded offerings from disk");
            Ok(offerings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("No offerings file found, starting fresh");
            Ok(Vec::new())
        }
        Err(e) => Err(e.into()),
    }
}

/// Save offerings to disk (atomic write)
pub async fn save_offerings(offerings: &[garden_common::Offering]) -> Result<()> {
    let dir = PathBuf::from(garden_common::names::CONFIG_DIR);
    let path = dir.join("moss-offerings.json");
    tokio::fs::create_dir_all(&dir).await?;

    atomic_write(path, &offerings).await?;
    tracing::debug!(count = offerings.len(), "Saved offerings to disk");
    Ok(())
}

// ============================================================================
// Offerings Cache
// ============================================================================

/// Load offerings cache from disk
///
/// Returns None if cache doesn't exist or is invalid.
pub async fn load_offerings_cache<T: serde::de::DeserializeOwned>() -> Result<Option<T>> {
    let path = offerings_cache_path();

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => {
            match serde_json::from_str(&content) {
                Ok(cache) => Ok(Some(cache)),
                Err(e) => {
                    tracing::warn!(error = ?e, "Invalid offerings cache, will rebuild");
                    Ok(None)
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Save offerings cache to disk (atomic write)
pub async fn save_offerings_cache<T: serde::Serialize>(cache: &T) -> Result<()> {
    let dir = PathBuf::from(garden_common::names::CONFIG_DIR);
    tokio::fs::create_dir_all(&dir).await?;

    let path = offerings_cache_path();
    atomic_write(&path, cache).await
}

// ============================================================================
// Stone Identity
// ============================================================================

/// Load or generate the stone ID (hardware-based)
///
/// The stone ID is a persistent, hardware-derived identifier for this stone.
/// It survives hostname changes, IP changes, OS reinstalls, and most hardware upgrades.
/// Uses GUIDv5 (SHA-1 namespace) derived from stable hardware characteristics.
///
/// ## Strategy:
/// 1. Try to load from cache (hardware-id file)
/// 2. Generate from hardware characteristics (motherboard UUID, machine GUID, etc.)
/// 3. Cache the result for faster subsequent boots
///
/// The hardware-based approach ensures the same physical machine always gets
/// the same stone ID, even after reinstalling the OS or deleting all Zen Garden data.
pub async fn load_or_generate_stone_id() -> String {
    // Check if we have a cached hardware ID
    if let Some(cached_id) = super::hardware_id::load_cached_hardware_id().await {
        tracing::debug!(stone_id = %cached_id, "Loaded cached hardware-based stone ID");
        return cached_id;
    }

    // Generate new hardware-based ID
    let hw_id = super::hardware_id::generate_hardware_id().await;
    tracing::info!(
        stone_id = %hw_id,
        "Generated hardware-based stone ID (will be stable for this physical machine)"
    );

    // Cache it for faster subsequent boots
    if let Err(e) = super::hardware_id::save_hardware_id_cache(&hw_id).await {
        tracing::warn!(
            error = ?e,
            "Failed to cache hardware ID (will regenerate on next boot, but same result)"
        );
    }

    hw_id
}

// ============================================================================
// Helpers
// ============================================================================

/// Atomic file write helper
///
/// Uses temp file + rename for atomic writes.
/// Handles Windows rename-over-existing-file issue.
async fn atomic_write<T: serde::Serialize, P: AsRef<std::path::Path>>(
    path: P,
    data: &T,
) -> Result<()> {
    let path = path.as_ref();
    let tmp_path = path.with_extension("tmp");

    let content = serde_json::to_string_pretty(data)?;
    tokio::fs::write(&tmp_path, content).await?;

    match tokio::fs::rename(&tmp_path, path).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Windows doesn't allow rename over existing file
            if cfg!(windows) {
                let _ = tokio::fs::remove_file(path).await;
                tokio::fs::rename(&tmp_path, path).await?;
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestData {
        value: String,
    }

    #[tokio::test]
    async fn test_atomic_write() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_atomic.json");

        let data = TestData { value: "test".into() };
        atomic_write(&test_file, &data).await.expect("write failed");

        let content = tokio::fs::read_to_string(&test_file).await.expect("read failed");
        let loaded: TestData = serde_json::from_str(&content).expect("parse failed");

        assert_eq!(loaded, data);

        // Cleanup
        let _ = tokio::fs::remove_file(&test_file).await;
    }
}
