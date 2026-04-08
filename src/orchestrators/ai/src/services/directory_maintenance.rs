//! Directory maintenance task.
//!
//! Waits on the Directory's dirty-counter `watch` channel. On every
//! change, debounces briefly to coalesce bursts (e.g., a provider
//! publishing three registrations back-to-back), then calls
//! `directory.rebuild_snapshot()`.
//!
//! There is no ticker, no periodic refresh. Every rebuild is caused
//! by an explicit provider or directory event.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::domain::directory::Directory;

/// How long to wait after the first dirty pulse before rebuilding,
/// so that a burst of publications coalesces into a single rebuild.
pub const DEBOUNCE_WINDOW: Duration = Duration::from_millis(50);

/// Run the maintenance loop until `shutdown` fires.
pub async fn run(directory: Arc<Directory>, shutdown: CancellationToken) {
    let mut dirty = directory.subscribe_dirty();

    // Unconditional initial rebuild.
    //
    // By the time this task starts, providers may already have been
    // registered (and may already be publishing state into their
    // watch channels via send_replace). The dirty pulses from those
    // pre-task events arrived before this receiver existed; without
    // an unconditional rebuild here we would only react to *future*
    // events, leaving the directory snapshot stale at version 0.
    dirty.mark_unchanged();
    directory.rebuild_snapshot().await;

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            res = dirty.changed() => {
                if res.is_err() {
                    break;
                }
                // Debounce: sleep briefly; any additional pulses that
                // arrive in this window will be coalesced into the
                // next `changed()` wait (the watch channel only
                // remembers "there was a change", not a queue).
                tokio::time::sleep(DEBOUNCE_WINDOW).await;
                dirty.mark_unchanged();
                directory.rebuild_snapshot().await;
            }
        }
    }
}
