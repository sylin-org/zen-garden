//! Activity collector — subscribes to domain events and stores them in a ring buffer

use crate::infra::event_bus::SseEvent;
use crate::AppState;

/// Spawn the activity collector background task.
///
/// Listens to domain events and pushes SseEvent records into the activity buffer
/// for the GET /api/v1/garden/activity endpoint.
pub fn spawn_activity_collector(state: &AppState) -> tokio::task::JoinHandle<()> {
    let activity = state.activity.clone();
    let mut rx = state.event_bus.subscribe();
    let buffer_size = AppState::activity_buffer_size();

    tokio::spawn(async move {
        tracing::info!("Activity collector started (buffer: {})", buffer_size);

        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Skip heartbeat noise — only store meaningful events
                    if event.event_type() == "stone.heartbeat"
                        || event.event_type() == "topology.refreshed"
                    {
                        continue;
                    }

                    let sse_event = SseEvent::from(&event);
                    let mut buf = activity.write().await;
                    if buf.len() >= buffer_size {
                        buf.pop_front();
                    }
                    buf.push_back(sse_event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "Activity collector lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("Activity collector stopped (channel closed)");
                    break;
                }
            }
        }
    })
}
