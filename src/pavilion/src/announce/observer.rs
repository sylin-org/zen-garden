//! SSE observers — translate protocol streams into [`GardenEvent`]s.
//!
//! One task per observed stone. Each task survives transient
//! disconnects with a brief backoff. The supervisor in
//! [`crate::app`] reconciles the active observer set against
//! awareness so a fresh task spawns when a new stone joins and a
//! cancellation fires when one is evicted.
//!
//! ## Wire format
//!
//! Moss serves storage ticks via `axum::response::sse::Sse` — line
//! framing is `event: <name>\n` followed by `data: <json>\n` and a
//! blank line. We only care about the `data:` line; the parser is
//! intentionally minimal.

use std::time::Duration;

use futures_util::StreamExt;
use garden_common::storage::StorageTick;
use tokio_util::sync::CancellationToken;

use super::policy::Announcer;

/// Cooldown between reconnect attempts after an error or EOF.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);

/// Identity of a stone the observer is watching. Carries the
/// minimum the SSE loop needs (endpoint to dial, name for log
/// lines + event payloads).
#[derive(Debug, Clone)]
pub struct ObserverTarget {
    pub stone_name: String,
    pub endpoint: String,
}

/// Spawn a storage-stream observer against `target`. Returns a
/// cancellation token; cancel it to stop the observer.
pub fn spawn_storage_observer(
    announcer: Announcer,
    target: ObserverTarget,
) -> CancellationToken {
    let token = CancellationToken::new();
    let token_inner = token.clone();
    tauri::async_runtime::spawn(async move {
        run_storage_loop(announcer, target, token_inner).await;
    });
    token
}

async fn run_storage_loop(
    announcer: Announcer,
    target: ObserverTarget,
    token: CancellationToken,
) {
    tracing::info!(
        stone = %target.stone_name,
        endpoint = %target.endpoint,
        "storage observer: starting"
    );

    loop {
        if token.is_cancelled() {
            break;
        }

        match connect_and_read(&announcer, &target, &token).await {
            Ok(()) => {
                tracing::debug!(
                    stone = %target.stone_name,
                    "storage observer: stream closed cleanly"
                );
            }
            Err(e) => {
                tracing::warn!(
                    stone = %target.stone_name,
                    error = %e,
                    "storage observer: stream error"
                );
            }
        }

        if token.is_cancelled() {
            break;
        }

        tokio::select! {
            _ = tokio::time::sleep(RECONNECT_BACKOFF) => {}
            _ = token.cancelled() => break,
        }
    }

    tracing::info!(
        stone = %target.stone_name,
        "storage observer: stopped"
    );
}

async fn connect_and_read(
    announcer: &Announcer,
    target: &ObserverTarget,
    token: &CancellationToken,
) -> anyhow::Result<()> {
    // Streaming-tuned client (no overall timeout, longer per-read).
    // The default `api_for` 8 s timeout would abort the SSE stream
    // every 8 s regardless of whether keep-alive comments arrived.
    let api = crate::connection::streaming_api_for_endpoint(&target.endpoint);
    let response = api.storage().stream().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => return Ok(()),
            chunk = byte_stream.next() => {
                let Some(bytes) = chunk else { return Ok(()); };
                let bytes = bytes?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // SSE event boundary is a blank line. Drain everything we
                // have so far one event at a time.
                while let Some(idx) = buffer.find("\n\n") {
                    let frame = buffer[..idx].to_string();
                    buffer.drain(..idx + 2);
                    handle_frame(&frame, announcer, target).await;
                }
            }
        }
    }
}

async fn handle_frame(frame: &str, announcer: &Announcer, target: &ObserverTarget) {
    // Concatenate every `data:` line in the frame — SSE allows split
    // payloads, though Moss's storage stream emits one line per event.
    let payload: String = frame
        .lines()
        .filter_map(|l| l.strip_prefix("data:").or_else(|| l.strip_prefix("data: ")))
        .collect::<Vec<_>>()
        .join("\n");
    if payload.is_empty() {
        return; // keep-alive comment or non-data line
    }

    let tick: StorageTick = match serde_json::from_str(&payload) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                stone = %target.stone_name,
                error = %e,
                payload = %payload.trim(),
                "storage observer: malformed tick"
            );
            return;
        }
    };

    if tick.creates == 0 && tick.modifies == 0 && tick.deletes == 0 {
        // Cursor-only tick (no churn) — Activity wants edges, not heartbeats.
        return;
    }

    announcer
        .observe(super::event::GardenEvent::StorageActivity {
            stone_name: target.stone_name.clone(),
            bank_name: tick.storage,
            creates: tick.creates,
            modifies: tick.modifies,
            deletes: tick.deletes,
        })
        .await;
}
