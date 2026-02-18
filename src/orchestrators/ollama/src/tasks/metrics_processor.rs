//! Metrics processor: receives metric events from the proxy channel
//! and writes them to the MetricsEngine.
//!
//! Decouples the proxy request path from write locks on metrics,
//! eliminating contention between inference handling and metric recording.

use crate::app_state::AppState;
use crate::domain::types::MetricEvent;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Run the metrics processor loop.
pub async fn run(
    state: AppState,
    mut rx: mpsc::UnboundedReceiver<MetricEvent>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                let mut metrics = state.metrics.write().await;
                metrics.process_event(event);
            }
            _ = shutdown.cancelled() => return,
        }
    }
}
