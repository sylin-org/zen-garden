//! Network state persistence
//!
//! Persists static IP state to disk for:
//! - Crash recovery (know what was configured)
//! - Offering lifecycle tracking (who requested static IP)
//! - Debugging (see what's configured vs desired)
//!
//! State file location: `/etc/zen-garden/network-state.json`

use crate::domain::{NetworkError, StaticIpState};
use std::path::{Path, PathBuf};

/// Default state file path
#[cfg(target_os = "linux")]
const STATE_FILE_PATH: &str = "/etc/zen-garden/network-state.json";

#[cfg(target_os = "windows")]
const STATE_FILE_PATH: &str = ".zen-garden/network-state.json";

#[cfg(target_os = "macos")]
const STATE_FILE_PATH: &str = "/etc/zen-garden/network-state.json";

/// Get the state file path
pub fn state_file_path() -> PathBuf {
    PathBuf::from(STATE_FILE_PATH)
}

/// Load network state from disk
///
/// Returns default state if file doesn't exist or is invalid.
pub async fn load_network_state() -> StaticIpState {
    load_network_state_from(&state_file_path()).await
}

/// Load network state from a specific path
pub async fn load_network_state_from(path: &Path) -> StaticIpState {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => match serde_json::from_str::<StaticIpState>(&content) {
            Ok(state) => {
                tracing::debug!(
                    path = ?path,
                    mode = ?state.mode,
                    requesters = state.requester_count(),
                    "Loaded network state from disk"
                );
                state
            }
            Err(e) => {
                tracing::warn!(
                    path = ?path,
                    error = %e,
                    "Failed to parse network state, using defaults"
                );
                StaticIpState::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(path = ?path, "Network state file not found, using defaults");
            StaticIpState::default()
        }
        Err(e) => {
            tracing::warn!(
                path = ?path,
                error = %e,
                "Failed to read network state, using defaults"
            );
            StaticIpState::default()
        }
    }
}

/// Save network state to disk
///
/// Creates parent directories if needed.
/// Uses atomic write (write to temp, then rename).
pub async fn save_network_state(state: &StaticIpState) -> Result<(), NetworkError> {
    save_network_state_to(state, &state_file_path()).await
}

/// Save network state to a specific path
pub async fn save_network_state_to(state: &StaticIpState, path: &Path) -> Result<(), NetworkError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            NetworkError::PersistenceFailed(format!(
                "Failed to create directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    // Serialize to JSON (pretty for readability)
    let content = serde_json::to_string_pretty(state).map_err(|e| {
        NetworkError::PersistenceFailed(format!("Failed to serialize state: {}", e))
    })?;

    // Write to temp file first (atomic write)
    let temp_path = path.with_extension("json.tmp");

    tokio::fs::write(&temp_path, &content).await.map_err(|e| {
        NetworkError::PersistenceFailed(format!(
            "Failed to write temp file {}: {}",
            temp_path.display(),
            e
        ))
    })?;

    // Rename temp to final (atomic on most filesystems)
    tokio::fs::rename(&temp_path, path).await.map_err(|e| {
        // Try to clean up temp file
        let _ = std::fs::remove_file(&temp_path);
        NetworkError::PersistenceFailed(format!(
            "Failed to rename {} to {}: {}",
            temp_path.display(),
            path.display(),
            e
        ))
    })?;

    tracing::debug!(
        path = ?path,
        mode = ?state.mode,
        requesters = state.requester_count(),
        "Saved network state to disk"
    );

    Ok(())
}

/// Delete network state file
///
/// Used when reverting to DHCP with no state to preserve.
pub async fn delete_network_state() -> Result<(), NetworkError> {
    let path = state_file_path();

    match tokio::fs::remove_file(&path).await {
        Ok(()) => {
            tracing::debug!(path = ?path, "Deleted network state file");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File doesn't exist - that's fine
            Ok(())
        }
        Err(e) => Err(NetworkError::PersistenceFailed(format!(
            "Failed to delete {}: {}",
            path.display(),
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NetworkMode;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_save_and_load_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test-state.json");

        // Create state
        let mut state = StaticIpState::default();
        state.add_requester("pihole");
        state.mode = NetworkMode::static_ip("192.168.1.100".parse().unwrap());

        // Save
        save_network_state_to(&state, &path).await.unwrap();

        // Verify file exists
        assert!(path.exists());

        // Load
        let loaded = load_network_state_from(&path).await;

        assert_eq!(loaded.requester_count(), 1);
        assert!(loaded.requested_by.contains(&"pihole".to_string()));
        assert!(loaded.mode.is_static());
    }

    #[tokio::test]
    async fn test_load_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");

        let state = load_network_state_from(&path).await;

        // Should return default state
        assert!(state.mode.is_dhcp());
        assert!(!state.has_requesters());
    }

    #[tokio::test]
    async fn test_load_invalid_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("invalid.json");

        // Write invalid JSON
        tokio::fs::write(&path, "not valid json").await.unwrap();

        let state = load_network_state_from(&path).await;

        // Should return default state
        assert!(state.mode.is_dhcp());
    }
}
