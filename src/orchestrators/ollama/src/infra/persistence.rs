//! Configuration and metrics persistence.
//!
//! - `router-config.toml` — user settings (TOML, human-editable).
//! - `metrics.json` — counter snapshots (JSON, machine-generated).
//!
//! Both live on the container volume mount.

use crate::domain::types::{MetricsSnapshot, RouterConfig};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Load configuration from disk, or return defaults.
pub async fn load_config(data_dir: &str) -> RouterConfig {
    let path = config_path(data_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => {
                tracing::info!(path = %path.display(), "loaded router config");
                config
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse router config, using defaults");
                RouterConfig::default()
            }
        },
        Err(_) => {
            tracing::info!("no config file found, using defaults");
            RouterConfig::default()
        }
    }
}

/// Save configuration to disk.
pub async fn save_config(data_dir: &str, config: &RouterConfig) -> Result<()> {
    let path = config_path(data_dir);
    let content = toml::to_string_pretty(config).context("serialize config")?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    tokio::fs::write(&path, content)
        .await
        .context("write router-config.toml")?;
    tracing::debug!(path = %path.display(), "saved router config");
    Ok(())
}

/// Load metrics from disk (for display on startup).
pub async fn load_metrics(data_dir: &str) -> MetricsSnapshot {
    let path = metrics_path(data_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => MetricsSnapshot::default(),
    }
}

/// Flush metrics snapshot to disk.
pub async fn save_metrics(data_dir: &str, snapshot: &MetricsSnapshot) -> Result<()> {
    let path = metrics_path(data_dir);

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    let content = serde_json::to_string_pretty(snapshot).context("serialize metrics")?;
    tokio::fs::write(&path, content)
        .await
        .context("write metrics.json")?;
    Ok(())
}

fn config_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("router-config.toml")
}

fn metrics_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("metrics.json")
}
