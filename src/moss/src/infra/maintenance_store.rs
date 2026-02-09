//! Maintenance sweep persistence
//!
//! One JSON file per sweep run: `{data_dir}/maintenance/sweep-{timestamp}.json`
//! - Write-once (no read-modify-write)
//! - Natural sort by filename (ISO 8601 timestamps)
//! - Pruned to last N files by the orchestrator
//! - No in-memory cache (cold data, read on API request)

use crate::domain::maintenance::SweepRun;
use anyhow::{Context, Result};
use std::path::PathBuf;

const MAX_SWEEP_HISTORY: usize = 20;
const SWEEP_DIR: &str = "maintenance";

/// Get the maintenance sweep directory
fn sweep_dir() -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir()).join(SWEEP_DIR)
}

/// Save a sweep run to disk and prune old files
pub async fn save_sweep_run(run: &SweepRun) -> Result<()> {
    let dir = sweep_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .context("Failed to create maintenance directory")?;

    let filename = format!(
        "sweep-{}.json",
        run.timestamp.format("%Y%m%dT%H%M%SZ")
    );
    let path = dir.join(&filename);

    let content =
        serde_json::to_string_pretty(run).context("Failed to serialize sweep run")?;
    tokio::fs::write(&path, content)
        .await
        .context("Failed to write sweep file")?;

    // Best-effort prune — don't fail the save if pruning fails
    if let Err(e) = prune_old_sweeps(&dir).await {
        tracing::warn!(error = ?e, "Failed to prune old sweep files");
    }

    Ok(())
}

/// Load sweep history (newest first)
pub async fn load_sweep_history() -> Result<Vec<SweepRun>> {
    let dir = sweep_dir();

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .context("Failed to read maintenance directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let name = entry
            .file_name()
            .to_str()
            .unwrap_or_default()
            .to_string();
        if name.starts_with("sweep-") && name.ends_with(".json") {
            files.push(entry.path());
        }
    }

    // Sort by filename descending (newest first — ISO timestamps sort naturally)
    files.sort();
    files.reverse();

    let mut runs = Vec::with_capacity(files.len());
    for path in files {
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match serde_json::from_str::<SweepRun>(&content) {
                Ok(run) => runs.push(run),
                Err(e) => {
                    tracing::warn!(file = ?path, error = ?e, "Skipping malformed sweep file");
                }
            },
            Err(e) => {
                tracing::warn!(file = ?path, error = ?e, "Failed to read sweep file");
            }
        }
    }

    Ok(runs)
}

/// Delete oldest sweep files beyond retention limit
async fn prune_old_sweeps(dir: &std::path::Path) -> Result<()> {
    let mut files = Vec::new();
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .context("Failed to read maintenance directory for pruning")?;

    while let Some(entry) = entries.next_entry().await? {
        let name = entry
            .file_name()
            .to_str()
            .unwrap_or_default()
            .to_string();
        if name.starts_with("sweep-") && name.ends_with(".json") {
            files.push((name, entry.path()));
        }
    }

    if files.len() <= MAX_SWEEP_HISTORY {
        return Ok(());
    }

    // Sort ascending by name (oldest first)
    files.sort_by(|a, b| a.0.cmp(&b.0));

    // Remove oldest files beyond retention
    let to_remove = files.len() - MAX_SWEEP_HISTORY;
    for (_, path) in files.iter().take(to_remove) {
        if let Err(e) = tokio::fs::remove_file(path).await {
            tracing::warn!(file = ?path, error = ?e, "Failed to prune old sweep file");
        }
    }

    Ok(())
}
