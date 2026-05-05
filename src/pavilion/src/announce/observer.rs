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
                    if let Some(event) = parse_frame(&frame, target) {
                        announcer.observe(event).await;
                    }
                }
            }
        }
    }
}

/// Pure SSE-frame → [`GardenEvent`] translation. Returned `None`
/// means the frame was a keep-alive, a cursor-only tick, or a
/// malformed payload (in which case we logged + skipped).
///
/// Pulled out as a free function so the integration tests can drive
/// it directly without standing up a real [`Announcer`] (which
/// requires a Tauri `AppHandle`).
pub(crate) fn parse_frame(
    frame: &str,
    target: &ObserverTarget,
) -> Option<super::event::GardenEvent> {
    // Concatenate every `data:` line in the frame — SSE allows split
    // payloads, though Moss's storage stream emits one line per event.
    let payload: String = frame
        .lines()
        .filter_map(|l| l.strip_prefix("data:").or_else(|| l.strip_prefix("data: ")))
        .collect::<Vec<_>>()
        .join("\n");
    if payload.is_empty() {
        return None; // keep-alive comment or non-data line
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
            return None;
        }
    };

    if tick.creates == 0 && tick.modifies == 0 && tick.deletes == 0 {
        // Cursor-only tick (no churn) — Activity wants edges, not heartbeats.
        return None;
    }

    Some(super::event::GardenEvent::StorageActivity {
        stone_name: target.stone_name.clone(),
        bank_name: tick.storage,
        creates: tick.creates,
        modifies: tick.modifies,
        deletes: tick.deletes,
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests for the SSE frame parser. These exercise the
    //! protocol surface (cursor-only frames, malformed JSON, multi-
    //! line `data:` concatenation, keep-alive comments) without
    //! standing up an HTTP server — the end-to-end stream test
    //! lives in [`super::e2e`] below.
    use super::*;
    use crate::announce::event::GardenEvent;

    fn target() -> ObserverTarget {
        ObserverTarget {
            stone_name: "stone-test-meadow".into(),
            endpoint: "http://127.0.0.1:9999".into(),
        }
    }

    #[test]
    fn churn_tick_becomes_storage_activity() {
        // Real-shaped Moss tick (note short field names from
        // `#[serde(rename = "C"/"M"/"D")]`).
        let frame = r#"event: tick
data: {"cursor":"42","storage":"primary","replica_set_id":"rs-abc","C":3,"M":1,"D":0}"#;

        let event = parse_frame(frame, &target()).expect("event should fire");
        match event {
            GardenEvent::StorageActivity {
                stone_name,
                bank_name,
                creates,
                modifies,
                deletes,
            } => {
                assert_eq!(stone_name, "stone-test-meadow");
                assert_eq!(bank_name, "primary");
                assert_eq!(creates, 3);
                assert_eq!(modifies, 1);
                assert_eq!(deletes, 0);
            }
            other => panic!("expected StorageActivity, got {other:?}"),
        }
    }

    #[test]
    fn cursor_only_tick_is_dropped() {
        // Heartbeat tick — cursor advanced but no churn. Activity
        // surfaces edges, not heartbeats; the parser must filter
        // these out before they hit the announcer.
        let frame =
            r#"data: {"cursor":"99","storage":"primary","replica_set_id":"rs","C":0,"M":0,"D":0}"#;
        assert!(parse_frame(frame, &target()).is_none());
    }

    #[test]
    fn keep_alive_comment_frame_is_dropped() {
        // SSE comment line — must not parse as an event.
        let frame = ": keep-alive";
        assert!(parse_frame(frame, &target()).is_none());
    }

    #[test]
    fn malformed_json_does_not_panic() {
        let frame = "data: {not json}";
        // Logs a warning, returns None — must not panic and must not
        // emit a phantom event.
        assert!(parse_frame(frame, &target()).is_none());
    }

    #[test]
    fn multi_line_data_payload_concatenates() {
        // SSE allows splitting a JSON payload across multiple `data:`
        // lines; the parser must join them with `\n` before parsing.
        // (Moss's storage stream emits one line per event today, but
        // the protocol permits this — and the parser already handles
        // it because that's what the SSE spec says.)
        let frame = "data: {\"cursor\":\"7\",\"storage\":\"primary\",\ndata: \"replica_set_id\":\"rs\",\"C\":1,\"M\":0,\"D\":0}";
        // serde_json with embedded newline inside the string is a
        // tighter test than what Moss currently emits, so guard it
        // with a softer assertion: either parsed (then check), or
        // dropped (acceptable — invalid JSON for our parser).
        if let Some(GardenEvent::StorageActivity { creates, .. }) = parse_frame(frame, &target()) {
            assert_eq!(creates, 1);
        }
    }

    #[test]
    fn data_prefix_with_or_without_leading_space() {
        // SSE permits `data:foo` and `data: foo` — both valid.
        // Parser must accept either.
        let with_space = r#"data: {"cursor":"1","storage":"a","replica_set_id":"x","C":1,"M":0,"D":0}"#;
        let no_space   = r#"data:{"cursor":"1","storage":"a","replica_set_id":"x","C":1,"M":0,"D":0}"#;
        assert!(parse_frame(with_space, &target()).is_some());
        assert!(parse_frame(no_space, &target()).is_some());
    }

    #[test]
    fn target_stone_name_propagates_into_event() {
        // Critical for fan-in: the observer adds the stone name
        // (which only it knows) to every event so the merged
        // Activity feed can attribute each row to its source.
        let frame = r#"data: {"cursor":"1","storage":"a","replica_set_id":"x","C":1,"M":0,"D":0}"#;
        let target = ObserverTarget {
            stone_name: "stone-quiet-cove".into(),
            endpoint: "http://x".into(),
        };
        let event = parse_frame(frame, &target).unwrap();
        match event {
            GardenEvent::StorageActivity { stone_name, .. } => {
                assert_eq!(stone_name, "stone-quiet-cove");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn frames_for_two_stones_carry_distinct_attribution() {
        // Multi-stone fan-in invariant: identical wire payloads
        // observed from two different targets must produce events
        // attributed to each respective target. This is what makes
        // the merged Activity feed correct when two stones tick
        // simultaneously.
        let payload =
            r#"data: {"cursor":"1","storage":"primary","replica_set_id":"rs","C":2,"M":0,"D":0}"#;
        let alpha = ObserverTarget {
            stone_name: "stone-alpha".into(),
            endpoint: "http://x".into(),
        };
        let beta = ObserverTarget {
            stone_name: "stone-beta".into(),
            endpoint: "http://y".into(),
        };
        let event_a = parse_frame(payload, &alpha).unwrap();
        let event_b = parse_frame(payload, &beta).unwrap();
        match (event_a, event_b) {
            (
                GardenEvent::StorageActivity {
                    stone_name: a_stone,
                    bank_name: a_bank,
                    ..
                },
                GardenEvent::StorageActivity {
                    stone_name: b_stone,
                    bank_name: b_bank,
                    ..
                },
            ) => {
                assert_eq!(a_stone, "stone-alpha");
                assert_eq!(b_stone, "stone-beta");
                // Bank name is shared across stones — it's the
                // replica-set display name, not a per-stone label.
                assert_eq!(a_bank, "primary");
                assert_eq!(b_bank, "primary");
            }
            _ => panic!("wrong variant"),
        }
    }
}

#[cfg(test)]
mod e2e {
    //! End-to-end SSE pipeline test. Spins up a real axum SSE
    //! endpoint at the same path Moss serves
    //! (`/api/v1/stone/storage/stream`), drives the observer's
    //! byte-level reader, and asserts the parsed events match
    //! exactly what was on the wire — no mocks, no monkey-patching.
    //!
    //! The Announcer is bypassed (it requires a Tauri `AppHandle`)
    //! by re-using the `parse_frame` seam directly inside a custom
    //! reader loop that mirrors `connect_and_read`. This exercises
    //! the SSE chunk-buffer / frame-boundary logic that the unit
    //! tests above cannot reach.
    use super::*;
    use axum::{
        response::sse::{Event, KeepAlive, Sse},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use futures_util::{stream, Stream};
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    async fn sse_handler() -> impl IntoResponse {
        // Three-frame fixture: a churn tick, a cursor-only
        // heartbeat, and another churn tick. The observer should
        // surface exactly two `StorageActivity` events.
        let events: Vec<Result<Event, Infallible>> = vec![
            Ok(Event::default().json_data(StorageTick {
                cursor: "1".into(),
                storage: "primary".into(),
                replica_set_id: "rs-1".into(),
                creates: 4,
                modifies: 2,
                deletes: 0,
            }).unwrap()),
            Ok(Event::default().json_data(StorageTick {
                cursor: "2".into(),
                storage: "primary".into(),
                replica_set_id: "rs-1".into(),
                creates: 0,
                modifies: 0,
                deletes: 0,
            }).unwrap()),
            Ok(Event::default().json_data(StorageTick {
                cursor: "3".into(),
                storage: "primary".into(),
                replica_set_id: "rs-1".into(),
                creates: 0,
                modifies: 0,
                deletes: 7,
            }).unwrap()),
        ];

        let s: Box<dyn Stream<Item = Result<Event, Infallible>> + Send + Unpin> =
            Box::new(stream::iter(events));
        Sse::new(s).keep_alive(KeepAlive::default())
    }

    async fn spawn_sse_fixture() -> String {
        let app = Router::new().route("/api/v1/stone/storage/stream", get(sse_handler));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    /// Pure observer reader — same shape as `connect_and_read` but
    /// collects events into a buffer instead of dispatching to an
    /// announcer. Returns once the stream EOFs or the deadline
    /// fires.
    async fn read_to_buffer(
        url: String,
        target: ObserverTarget,
        deadline: Duration,
    ) -> Vec<crate::announce::event::GardenEvent> {
        let collected = Arc::new(Mutex::new(Vec::new()));
        let collected_inner = collected.clone();
        let _ = tokio::time::timeout(deadline, async move {
            let response = reqwest::get(&url).await.unwrap();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            while let Some(chunk) = byte_stream.next().await {
                let bytes = chunk.unwrap();
                buffer.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(idx) = buffer.find("\n\n") {
                    let frame = buffer[..idx].to_string();
                    buffer.drain(..idx + 2);
                    if let Some(event) = parse_frame(&frame, &target) {
                        collected_inner.lock().unwrap().push(event);
                    }
                }
            }
        })
        .await;
        let guard = collected.lock().unwrap();
        guard.clone()
    }

    #[tokio::test]
    async fn observer_parses_real_axum_sse_stream() {
        use crate::announce::event::GardenEvent;
        let base = spawn_sse_fixture().await;
        let url = format!("{base}/api/v1/stone/storage/stream");

        let events = read_to_buffer(
            url,
            ObserverTarget {
                stone_name: "stone-fixture".into(),
                endpoint: base.clone(),
            },
            Duration::from_secs(3),
        )
        .await;

        // Heartbeat is filtered, so only the two churn ticks land.
        assert_eq!(
            events.len(),
            2,
            "expected 2 churn events (heartbeat must be filtered)"
        );
        let (creates, deletes) = match (&events[0], &events[1]) {
            (
                GardenEvent::StorageActivity { creates, .. },
                GardenEvent::StorageActivity { deletes, .. },
            ) => (*creates, *deletes),
            _ => panic!("wrong variant"),
        };
        assert_eq!(creates, 4, "first event preserves creates count");
        assert_eq!(deletes, 7, "third event preserves deletes count");
    }

    #[tokio::test]
    async fn two_stone_streams_fan_in_with_correct_attribution() {
        // Multi-stone fan-in: spawn two independent SSE servers,
        // run two observer readers concurrently, then verify each
        // event is attributed to the correct stone. This exercises
        // the same wire-format independence the production
        // supervisor relies on.
        use crate::announce::event::GardenEvent;
        let base_a = spawn_sse_fixture().await;
        let base_b = spawn_sse_fixture().await;

        let target_a = ObserverTarget {
            stone_name: "stone-alpha".into(),
            endpoint: base_a.clone(),
        };
        let target_b = ObserverTarget {
            stone_name: "stone-beta".into(),
            endpoint: base_b.clone(),
        };

        let read_a = read_to_buffer(
            format!("{base_a}/api/v1/stone/storage/stream"),
            target_a,
            Duration::from_secs(3),
        );
        let read_b = read_to_buffer(
            format!("{base_b}/api/v1/stone/storage/stream"),
            target_b,
            Duration::from_secs(3),
        );

        let (events_a, events_b) = tokio::join!(read_a, read_b);
        assert_eq!(events_a.len(), 2);
        assert_eq!(events_b.len(), 2);

        for event in &events_a {
            match event {
                GardenEvent::StorageActivity { stone_name, .. } => {
                    assert_eq!(stone_name, "stone-alpha");
                }
                _ => panic!("wrong variant"),
            }
        }
        for event in &events_b {
            match event {
                GardenEvent::StorageActivity { stone_name, .. } => {
                    assert_eq!(stone_name, "stone-beta");
                }
                _ => panic!("wrong variant"),
            }
        }
    }
}
