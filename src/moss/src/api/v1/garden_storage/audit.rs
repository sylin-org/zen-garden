//! Audit logging (append-only JSON lines)

use serde::Serialize;
use tokio::io::AsyncWriteExt;

/// Audit log entry for access events
#[derive(Debug, Serialize)]
pub struct AuditAccessEntry {
    pub timestamp: String,
    pub category: String,
    pub action: String,
    pub status: u16,
    pub stone_id: String,
    pub stone_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offering_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harvest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requesting_stone_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requesting_stone_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forwarded_for: Option<String>,
}

/// Append an audit entry to the local audit log (best effort).
pub async fn log_access(entry: &AuditAccessEntry) {
    let path = garden_common::constants::paths::audit_log_path();
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            tracing::warn!(path = %path, error = ?e, "Failed to create audit log directory");
            return;
        }
    }

    let line = match serde_json::to_string(entry) {
        Ok(json) => format!("{}\n", json),
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to serialize audit entry");
            return;
        }
    };

    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(path = %path, error = ?e, "Failed to open audit log");
            return;
        }
    };

    if let Err(e) = file.write_all(line.as_bytes()).await {
        tracing::warn!(path = %path, error = ?e, "Failed to write audit log entry");
    }
}
