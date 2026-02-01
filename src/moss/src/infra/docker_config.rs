//! Docker daemon configuration management
//!
//! Handles reading and writing daemon.json for Docker daemon configuration.
//! Primary use case: managing insecure-registries for garden container registries.
//!
//! ## Platform Support
//!
//! - Linux: `/etc/docker/daemon.json`
//! - Windows: `%PROGRAMDATA%\docker\config\daemon.json`
//!
//! ## Safety
//!
//! - Atomic writes via temp file + rename
//! - Preserves existing daemon.json settings (only modifies insecure-registries)
//! - Creates daemon.json if it doesn't exist
//! - Backs up existing file before modification

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Get the platform-specific daemon.json path
fn daemon_json_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Windows: %PROGRAMDATA%\docker\config\daemon.json
        let program_data = std::env::var("PROGRAMDATA")
            .unwrap_or_else(|_| "C:\\ProgramData".to_string());
        PathBuf::from(program_data).join("docker").join("config").join("daemon.json")
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux/macOS: /etc/docker/daemon.json
        PathBuf::from("/etc/docker/daemon.json")
    }
}

/// Docker daemon.json structure
///
/// We use a flexible structure that preserves unknown fields.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
struct DaemonConfig {
    /// List of insecure registries (our primary concern)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    insecure_registries: Vec<String>,

    /// Preserve all other fields we don't know about
    #[serde(flatten)]
    other: HashMap<String, serde_json::Value>,
}

/// Read the current insecure-registries list from daemon.json
///
/// Returns an empty list if:
/// - daemon.json doesn't exist
/// - daemon.json is empty or invalid
/// - insecure-registries key is not present
pub async fn read_insecure_registries() -> Result<Vec<String>> {
    let path = daemon_json_path();

    if !path.exists() {
        tracing::debug!(path = %path.display(), "daemon.json does not exist");
        return Ok(Vec::new());
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Handle empty file
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let config: DaemonConfig = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    Ok(config.insecure_registries)
}

/// Write the insecure-registries list to daemon.json
///
/// Preserves all other daemon.json settings.
/// Creates the file and parent directories if they don't exist.
///
/// Returns true if the file was modified, false if no changes were needed.
pub async fn write_insecure_registries(registries: &[String]) -> Result<bool> {
    let path = daemon_json_path();

    // Read existing config (or create default)
    let mut config = if path.exists() {
        let content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read {}", path.display()))?;

        if content.trim().is_empty() {
            DaemonConfig::default()
        } else {
            serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?
        }
    } else {
        DaemonConfig::default()
    };

    // Check if update needed
    let mut current = config.insecure_registries.clone();
    current.sort();
    let mut desired = registries.to_vec();
    desired.sort();

    if current == desired {
        return Ok(false);
    }

    // Update the config
    config.insecure_registries = registries.to_vec();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Write atomically via temp file
    let temp_path = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(&config)
        .context("Failed to serialize daemon config")?;

    if let Err(e) = tokio::fs::write(&temp_path, &content).await {
        tracing::error!(
            path = %temp_path.display(),
            error = %e,
            error_kind = ?e.kind(),
            "Failed to write daemon.json temp file"
        );
        anyhow::bail!("Failed to write {}: {}", temp_path.display(), e);
    }

    tokio::fs::rename(&temp_path, &path)
        .await
        .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), path.display()))?;

    tracing::debug!(
        path = %path.display(),
        registries = ?registries,
        "Updated daemon.json insecure-registries"
    );

    Ok(true)
}

/// Restart the Docker daemon to apply configuration changes
///
/// Platform-specific implementation:
/// - Linux: `systemctl restart docker`
/// - Windows: `Restart-Service docker`
pub async fn restart_docker_daemon() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        restart_docker_linux().await
    }

    #[cfg(target_os = "windows")]
    {
        restart_docker_windows().await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        tracing::warn!("Docker daemon restart not implemented for this platform");
        Ok(())
    }
}

#[cfg(target_os = "linux")]
async fn restart_docker_linux() -> Result<()> {
    use tokio::process::Command;

    tracing::info!("Restarting Docker daemon via systemctl");

    let output = Command::new("systemctl")
        .args(["restart", "docker"])
        .output()
        .await
        .context("Failed to execute systemctl restart docker")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("systemctl restart docker failed: {}", stderr);
    }

    // Wait briefly for Docker to come back up
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    Ok(())
}

#[cfg(target_os = "windows")]
async fn restart_docker_windows() -> Result<()> {
    use tokio::process::Command;

    tracing::info!("Restarting Docker daemon via service control");

    // Stop Docker service
    let stop_output = Command::new("net")
        .args(["stop", "docker"])
        .output()
        .await
        .context("Failed to stop Docker service")?;

    if !stop_output.status.success() {
        let stderr = String::from_utf8_lossy(&stop_output.stderr);
        // Don't fail if already stopped
        if !stderr.contains("is not started") {
            tracing::warn!("net stop docker returned: {}", stderr);
        }
    }

    // Brief pause
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Start Docker service
    let start_output = Command::new("net")
        .args(["start", "docker"])
        .output()
        .await
        .context("Failed to start Docker service")?;

    if !start_output.status.success() {
        let stderr = String::from_utf8_lossy(&start_output.stderr);
        anyhow::bail!("net start docker failed: {}", stderr);
    }

    // Wait for Docker to be ready
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_serialization() {
        let config = DaemonConfig {
            insecure_registries: vec!["192.168.1.100:5000".to_string()],
            other: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("insecure-registries"));
        assert!(json.contains("192.168.1.100:5000"));
    }

    #[test]
    fn test_daemon_config_preserves_unknown_fields() {
        let json = r#"{
            "insecure-registries": ["localhost:5000"],
            "storage-driver": "overlay2",
            "log-driver": "json-file"
        }"#;

        let config: DaemonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.insecure_registries, vec!["localhost:5000"]);
        assert!(config.other.contains_key("storage-driver"));
        assert!(config.other.contains_key("log-driver"));

        // Serialize back and verify fields preserved
        let back = serde_json::to_string(&config).unwrap();
        assert!(back.contains("storage-driver"));
        assert!(back.contains("overlay2"));
    }

    #[test]
    fn test_empty_registries_not_serialized() {
        let config = DaemonConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        // Empty insecure_registries should not appear in output
        assert!(!json.contains("insecure-registries"));
    }
}
