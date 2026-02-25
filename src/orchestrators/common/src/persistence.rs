//! Tending state persistence.
//!
//! All orchestrators bind to a tended stone (the Moss instance they query for
//! topology). This module provides load/save for that binding so orchestrators
//! can resume after restart without re-discovering.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Persisted tending state — which stone the orchestrator is bound to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendedStone {
    pub stone_name: String,
    pub stone_id: Option<String>,
    pub endpoint: String,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

/// Load tending state from `{data_dir}/.tending`.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub async fn load_tending(data_dir: &str) -> Option<TendedStone> {
    let path = Path::new(data_dir).join(".tending");
    let data = tokio::fs::read_to_string(&path).await.ok()?;
    let stone: TendedStone = serde_json::from_str(&data).ok()?;
    tracing::info!(
        stone = %stone.stone_name,
        endpoint = %stone.endpoint,
        "restored tending state from disk"
    );
    Some(stone)
}

/// Save tending state to `{data_dir}/.tending`.
pub async fn save_tending(data_dir: &str, stone: &TendedStone) -> Result<()> {
    let path = Path::new(data_dir).join(".tending");
    let json = serde_json::to_string_pretty(stone).context("serialize tending state")?;
    tokio::fs::write(&path, json)
        .await
        .context("write .tending file")?;
    Ok(())
}

/// Remove tending state from disk.
pub async fn clear_tending(data_dir: &str) {
    let path = Path::new(data_dir).join(".tending");
    let _ = tokio::fs::remove_file(&path).await;
}
