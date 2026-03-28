//! Metrics persistence — periodic flush to disk.

use std::path::Path;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;

const FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Background task: periodically persist metrics snapshot to disk.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(FLUSH_INTERVAL) => {}
        }

        let snapshot = {
            let metrics = state.metrics.read().await;
            metrics.snapshot()
        };

        let metrics_dir = Path::new(&state.data_dir).join("metrics");
        if let Err(e) = tokio::fs::create_dir_all(&metrics_dir).await {
            tracing::warn!(error = %e, "failed to create metrics dir");
            continue;
        }

        let path = metrics_dir.join("summary.json");
        match serde_json::to_string_pretty(&snapshot) {
            Ok(json) => {
                if let Err(e) = tokio::fs::write(&path, json).await {
                    tracing::warn!(error = %e, "failed to write metrics");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize metrics");
            }
        }
    }

    tracing::info!("metrics flush task shutting down");
}
