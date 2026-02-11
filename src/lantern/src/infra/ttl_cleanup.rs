//! TTL cleanup — background task for stone heartbeat expiry
//!
//! Periodically checks all registered stones and marks them offline
//! if their heartbeat TTL has expired.

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::domain::registration::check_ttl;
use crate::domain::topology::GardenTopology;
use crate::infra::event_bus::EventBus;

/// Interval between TTL checks (seconds)
const TTL_CHECK_INTERVAL_SECS: u64 = 10;

/// Run the TTL cleanup loop.
///
/// Checks stone heartbeats every 10 seconds and emits offline events.
pub async fn run_ttl_cleanup(
    topology: Arc<RwLock<GardenTopology>>,
    event_bus: EventBus,
) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(TTL_CHECK_INTERVAL_SECS)).await;

        let events = {
            let mut topo = topology.write().await;
            check_ttl(&mut topo)
        };

        for event in events {
            event_bus.emit(event);
        }
    }
}
