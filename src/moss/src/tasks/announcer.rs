//! Periodic announcement task
//!
//! Runs in background, announcing stone presence every 30 seconds.
//! Every chirp acts as a heartbeat — peers mark stones offline after 90s
//! of silence, so chirps MUST be unconditional.
//!
//! Every other cycle (every ~60s) a full tools snapshot beacon is broadcast
//! so remote registries can reconcile stale announced entries.
//!
//! Design:
//! - Simple interval loop (KISS)
//! - No complex state management (YAGNI)
//! - Delegates to announcement module (SoC)
//! - Respects network readiness (no chirps until network is ready)

use crate::AppState;
use std::sync::atomic::Ordering;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

/// Start periodic announcement task
///
/// Announces stone presence every 30 seconds via all channels.
/// Chirps unconditionally — they double as heartbeats for topology
/// liveness (peers mark offline after 90s silence).
/// Skips announcements only if network is not ready (no valid LAN IP).
///
/// Every other tick (~60s) also broadcasts a snapshot tools beacon so
/// remote registries can reconcile stale entries.
///
/// Exits cooperatively when the shutdown token is cancelled (MOSS-0004).
pub fn start_periodic_announcer(state: AppState, token: CancellationToken) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Skip first tick (already announced at startup)
        ticker.tick().await;

        let mut tick_count: u64 = 0;

        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = token.cancelled() => {
                    tracing::debug!("Periodic announcer shutting down (MOSS-0004)");
                    break;
                }
            }

            tick_count += 1;

            // Check network readiness - skip if not ready
            if !state.subsystems.network.ready.load(Ordering::Relaxed) {
                tracing::trace!("Periodic announcement skipped (network not ready)");
                continue;
            }

            // Refresh self_entry before chirping — this evicts expired gateways
            // (TTL=60s) and ensures we never broadcast stale registrations.
            state.sync_self_services(false).await;

            // Read current self topology entry
            let entry = state.current.topology.self_entry.read().await.clone();

            // Always chirp — peers rely on periodic chirps as heartbeats
            // to maintain online status in the topology cache.
            match crate::announcement::announce(&entry).await {
                Ok(()) => tracing::trace!("Periodic chirp sent"),
                Err(e) => tracing::warn!(error = ?e, "Periodic announcement failed"),
            }

            // Every other tick (~60s): broadcast a snapshot tools beacon so
            // remote registries can reconcile stale announced entries.
            if tick_count % 2 == 0 {
                let snapshot_deltas = {
                    let reg = state.tool.registry.read().await;
                    reg.local_snapshot_for_beacon(&state.current.stone.id)
                };
                let endpoint = state.current.topology.self_entry.read().await.address.http_base();
                if !endpoint.trim().is_empty() {
                    if let Err(e) = crate::infra::broadcast_tools_snapshot_beacon(
                        &state.current.stone.id,
                        &state.current.stone.name,
                        &endpoint,
                        snapshot_deltas,
                    )
                    .await
                    {
                        tracing::warn!(error = %e, "Failed to broadcast periodic tools snapshot beacon");
                    }
                }
            }
        }
    });

    tracing::info!("Periodic announcer started (30s interval)");
}
