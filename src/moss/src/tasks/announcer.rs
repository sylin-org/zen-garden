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
use tokio::time::{Duration, interval};
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
    tokio::spawn(periodic_announcer_task(state, token));
    tracing::info!("Periodic announcer started (30s interval)");
}

/// Inner future for the periodic announcer, usable by both
/// `start_periodic_announcer` and the `TaskSupervisor`.
pub(crate) async fn periodic_announcer_task(state: AppState, token: CancellationToken) {
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

        // Build the self topology entry on demand from source domains.
        let entry = state.build_self_entry().await;

        // Always chirp — peers rely on periodic chirps as heartbeats
        // to maintain online status in the topology cache.
        match state.topology.chirp(&entry).await {
            Ok(()) => tracing::trace!("Periodic chirp sent"),
            Err(e) => tracing::warn!(error = ?e, "Periodic announcement failed"),
        }

        // Every other tick (~60s): broadcast a snapshot tools beacon so
        // remote registries can reconcile stale announced entries.
        if tick_count.is_multiple_of(2) {
            let snapshot_deltas = state
                .tool
                .local_snapshot_for_beacon(&state.current.stone.id)
                .await;
            let endpoint = state.current.address.read().await.http_base();
            if !endpoint.trim().is_empty()
                && let Err(e) = state
                    .tool
                    .publish_snapshot(
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
