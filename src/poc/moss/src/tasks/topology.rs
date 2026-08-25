//! Topology maintenance background task
//!
//! Periodically marks stale stones as offline, evicts old offline stones,
//! and flushes dirty topology cache to disk.

use tokio_util::sync::CancellationToken;

/// Start topology maintenance task (TOPO-0002: with persistence)
///
/// Periodically marks stale stones as offline, evicts old offline stones,
/// and flushes dirty topology cache to disk.
/// Runs every 30 seconds (aligns with stone chirp interval).
pub fn start_topology_maintenance(state: crate::Moss, token: CancellationToken) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        interval.tick().await; // Skip first immediate tick

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Topology maintenance shutting down (MOSS-0004)");
                    break;
                }
            }
            let self_entry = crate::domain::topology::composition::build_self_entry(&state).await;
            let (marked, evicted) = state.topology.maintain(&self_entry).await;
            if marked > 0 || evicted > 0 {
                tracing::debug!(
                    marked_offline = marked,
                    evicted = evicted,
                    "Topology maintenance complete"
                );
            }
        }
    });
}
