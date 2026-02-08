//! Periodic announcement task
//!
//! Runs in background, announcing stone presence every 30 seconds.
//! Every chirp acts as a heartbeat — peers mark stones offline after 45s
//! of silence, so chirps MUST be unconditional.
//!
//! Design:
//! - Simple interval loop (KISS)
//! - No complex state management (YAGNI)
//! - Delegates to announcement module (SoC)
//! - Respects network readiness (no chirps until network is ready)

use std::sync::atomic::Ordering;
use tokio::time::{interval, Duration};
use crate::AppState;

/// Start periodic announcement task
///
/// Announces stone presence every 30 seconds via all channels.
/// Chirps unconditionally — they double as heartbeats for topology
/// liveness (peers mark offline after 45s silence).
/// Skips announcements only if network is not ready (no valid LAN IP).
///
/// Runs for process lifetime.
pub fn start_periodic_announcer(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Skip first tick (already announced at startup)
        ticker.tick().await;

        loop {
            ticker.tick().await;

            // Check network readiness - skip if not ready
            if !state.subsystems.network.ready.load(Ordering::Relaxed) {
                tracing::trace!("Periodic announcement skipped (network not ready)");
                continue;
            }

            // Read current self topology entry
            let entry = state.self_entry.read().await.clone();

            // Always chirp — peers rely on periodic chirps as heartbeats
            // to maintain online status in the topology cache.
            match crate::announcement::announce(&entry).await {
                Ok(()) => tracing::trace!("Periodic chirp sent"),
                Err(e) => tracing::warn!(error = ?e, "Periodic announcement failed"),
            }
        }
    });

    tracing::info!("Periodic announcer started (30s interval)");
}
