//! Metrics processor — consumes MetricEvent from the proxy channel,
//! updates MetricsEngine and DemandLedger.

use std::time::Instant;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::types::MetricEvent;

/// Background task: drain the metrics channel and update state.
pub async fn run(
    state: AppState,
    mut rx: mpsc::UnboundedReceiver<MetricEvent>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            event = rx.recv() => {
                let Some(event) = event else { break };

                // Update demand ledger.
                if let MetricEvent::Request {
                    ref model,
                    ref stone,
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

                // Update metrics engine.
                let mut metrics = state.metrics.write().await;
                metrics.process_event(event);
            }
        }
    }

    tracing::info!("metrics processor shutting down");
}
