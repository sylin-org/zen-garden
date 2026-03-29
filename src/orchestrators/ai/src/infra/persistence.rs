//! Configuration and metrics persistence.
//!
//! - `config.toml` — user settings (TOML, human-editable).
//! - `metrics/` — per-stone and summary data (JSON, machine-generated).
//!   Delete the folder to reset all historical metrics.
//!
//! Layout:
//!   {data_dir}/config.toml
//!   {data_dir}/metrics/summary.json
//!   {data_dir}/metrics/stones/{stone_name}.json

use crate::domain::types::{MetricsSnapshot, OrchestratorConfig, StoneMetrics};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── Config ───────────────────────────────────────────────────────

/// Load configuration from disk, or return defaults.
pub async fn load_config(data_dir: &str) -> OrchestratorConfig {
    let path = config_path(data_dir);
    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match toml::from_str(&content) {
            Ok(config) => {
                tracing::info!(path = %path.display(), "loaded orchestrator config");
                config
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse orchestrator config, using defaults");
                OrchestratorConfig::default()
            }
        },
        Err(_) => {
            tracing::info!("no config file found, using defaults");
            OrchestratorConfig::default()
        }
    }
}

/// Save configuration to disk.
pub async fn save_config(data_dir: &str, config: &OrchestratorConfig) -> Result<()> {
    let path = config_path(data_dir);
    let content = toml::to_string_pretty(config).context("serialize config")?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    tokio::fs::write(&path, content)
        .await
        .context("write config.toml")?;
    tracing::debug!(path = %path.display(), "saved orchestrator config");
    Ok(())
}

fn config_path(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("config.toml")
}

// ── Metrics Folder ───────────────────────────────────────────────

fn metrics_dir(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("metrics")
}

fn stones_dir(data_dir: &str) -> PathBuf {
    metrics_dir(data_dir).join("stones")
}

/// Sanitize a stone name for use as a filename.
fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Save all metrics to `{data_dir}/metrics/`.
///
/// Writes `summary.json` (global counters + per-model) and one
/// `stones/{stone_name}.json` per stone.
pub async fn save_metrics(data_dir: &str, snapshot: &MetricsSnapshot) -> Result<()> {
    let dir = metrics_dir(data_dir);
    let stones = stones_dir(data_dir);
    tokio::fs::create_dir_all(&stones).await.ok();

    // ── summary.json ────────────────────────────────────────────
    let summary = serde_json::json!({
        "requests_total": snapshot.requests_total,
        "tokens_in_total": snapshot.tokens_in_total,
        "tokens_out_total": snapshot.tokens_out_total,
        "errors_total": snapshot.errors_total,
        "per_model": snapshot.per_model,
        "started_at": snapshot.started_at,
        "snapshot_at": snapshot.snapshot_at,
    });
    let summary_json = serde_json::to_string_pretty(&summary).context("serialize summary")?;
    tokio::fs::write(dir.join("summary.json"), summary_json)
        .await
        .context("write summary.json")?;

    // ── stones/{name}.json ──────────────────────────────────────
    for (name, sm) in &snapshot.per_stone {
        let stone_data = serde_json::json!({
            "stone_name": name,
            "requests": sm.requests,
            "tokens_in": sm.tokens_in,
            "tokens_out": sm.tokens_out,
            "errors": sm.errors,
            "total_duration_ns": sm.total_duration_ns,
            "eval_duration_ns": sm.eval_duration_ns,
            "gen_tokens_per_sec": if sm.eval_duration_ns > 0 {
                Some(sm.tokens_out as f64 / (sm.eval_duration_ns as f64 / 1_000_000_000.0))
            } else { None },
            "roundtrip_tokens_per_sec": if sm.total_duration_ns > 0 {
                Some(sm.tokens_out as f64 / (sm.total_duration_ns as f64 / 1_000_000_000.0))
            } else { None },
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });
        let filename = format!("{}.json", safe_filename(name));
        let content = serde_json::to_string_pretty(&stone_data).context("serialize stone")?;
        tokio::fs::write(stones.join(filename), content).await.ok();
    }

    Ok(())
}

/// Load metrics from the `{data_dir}/metrics/` folder.
///
/// Falls back to a legacy single-file `metrics.json` if the folder
/// doesn't exist yet (one-time migration path).
pub async fn load_metrics(data_dir: &str) -> MetricsSnapshot {
    let dir = metrics_dir(data_dir);
    let summary_path = dir.join("summary.json");

    // Try folder-based format
    let summary_val = match tokio::fs::read_to_string(&summary_path).await {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(v) => v,
            Err(_) => return MetricsSnapshot::default(),
        },
        Err(_) => {
            // Fall back to legacy single metrics.json
            let legacy = Path::new(data_dir).join("metrics.json");
            return match tokio::fs::read_to_string(&legacy).await {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => MetricsSnapshot::default(),
            };
        }
    };

    let mut snapshot = MetricsSnapshot {
        requests_total: summary_val["requests_total"].as_u64().unwrap_or_default(),
        tokens_in_total: summary_val["tokens_in_total"].as_u64().unwrap_or_default(),
        tokens_out_total: summary_val["tokens_out_total"]
            .as_u64()
            .unwrap_or_default(),
        errors_total: summary_val["errors_total"].as_u64().unwrap_or_default(),
        per_model: serde_json::from_value(summary_val["per_model"].clone()).unwrap_or_default(),
        started_at: summary_val["started_at"].as_str().map(String::from),
        snapshot_at: summary_val["snapshot_at"].as_str().map(String::from),
        per_stone: HashMap::new(),
    };

    // Load per-stone files
    let stones = stones_dir(data_dir);
    if let Ok(mut entries) = tokio::fs::read_dir(&stones).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = tokio::fs::read_to_string(&path).await {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        let name = v["stone_name"].as_str().unwrap_or("").to_string();
                        if !name.is_empty() {
                            let sm = StoneMetrics {
                                requests: v["requests"].as_u64().unwrap_or_default(),
                                tokens_in: v["tokens_in"].as_u64().unwrap_or_default(),
                                tokens_out: v["tokens_out"].as_u64().unwrap_or_default(),
                                errors: v["errors"].as_u64().unwrap_or_default(),
                                total_duration_ns: v["total_duration_ns"]
                                    .as_u64()
                                    .unwrap_or_default(),
                                eval_duration_ns: v["eval_duration_ns"]
                                    .as_u64()
                                    .unwrap_or_default(),
                            };
                            snapshot.per_stone.insert(name, sm);
                        }
                    }
                }
            }
        }
    }

    snapshot
}

/// Delete the entire `{data_dir}/metrics/` folder.
///
/// Also removes the legacy `metrics.json` if present.
pub async fn clear_metrics(data_dir: &str) -> Result<()> {
    let dir = metrics_dir(data_dir);
    tokio::fs::remove_dir_all(&dir).await.ok();

    // Legacy cleanup
    let legacy = Path::new(data_dir).join("metrics.json");
    tokio::fs::remove_file(&legacy).await.ok();

    tracing::info!(path = %dir.display(), "cleared metrics folder");
    Ok(())
}
