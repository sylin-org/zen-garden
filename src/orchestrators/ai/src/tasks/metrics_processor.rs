//! Metrics processor: receives metric events from the proxy channel
//! and writes them to the MetricsEngine and DemandLedger.
//!
//! Decouples the proxy request path from write locks on metrics,
//! eliminating contention between inference handling and metric recording.

use crate::app_state::AppState;
use crate::domain::types::MetricEvent;
use std::time::Instant;
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
                // Feed the demand ledger (ORCH-0009)
                if let MetricEvent::Request {
                    ref stone,
                    ref model,
                    capability,
                    tokens_out,
                    eval_duration_ns,
                    ..
                } = event
                {
                    let mut ledger = state.demand_ledger.write().await;
                    ledger.record_request(
                        Instant::now(),
                        capability,
                        model,
                        stone,
                        tokens_out,
                        eval_duration_ns,
                    );
                }

                // Feed the existing metrics engine
                let mut metrics = state.metrics.write().await;
                metrics.process_event(event);
            }
            _ = shutdown.cancelled() => return,
        }
    }
}
