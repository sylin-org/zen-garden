//! Metrics flush task: periodically write metrics to disk.

use crate::app_state::AppState;
use crate::infra::persistence;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Flush interval.
const FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Run the metrics flush loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(FLUSH_INTERVAL) => {}
            _ = shutdown.cancelled() => {
                // Final flush on shutdown
                flush(&state).await;
                return;
            }
        }

        flush(&state).await;
    }
}

async fn flush(state: &AppState) {
    if !state.observability.metrics_enabled().await {
        return;
    }
    let snapshot = state.observability.metrics_snapshot().await;

    if let Err(e) = persistence::save_metrics(&state.data_dir, &snapshot).await {
        tracing::warn!(error = %e, "failed to flush metrics");
    }
}
