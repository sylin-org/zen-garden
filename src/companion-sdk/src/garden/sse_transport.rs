//! SSE transport — consumes moss's presence stream.
//!
//! [`SseTransport`] connects to a moss stone's `/api/v1/stone/presence/stream`
//! endpoint, parses SSE frames, translates wire kinds to canonical `core.*`
//! kinds (via [`wire_to_core_kind`]), deserializes the payload JSON into
//! a typed struct, and publishes the resulting [`Event`] to [`Pulse`].
//!
//! Reconnection uses exponential backoff (1, 2, 4, 8, 16, 32 seconds, cap)
//! and is resilient to connection errors, stream drops, and unknown event
//! kinds. On shutdown-token cancellation the transport exits cleanly.
//!
//! # Anti-corruption layer
//!
//! Moss emits legacy two-level wire kinds (`stone.load.updated`,
//! `presence.snapshot`, etc.). The SDK operates exclusively in the
//! three-level `core.*` namespace. `SseTransport` is the single boundary
//! that translates — the rest of the SDK never sees a wire kind.
//!
//! [`wire_to_core_kind`]: crate::garden::wire_to_core_kind
//! [`Event`]: crate::garden::Event
//! [`Pulse`]: crate::garden::Pulse

use super::core_payloads::{
    KIND_PRESENCE_SNAPSHOT, KIND_SERVICE_STARTED, KIND_SERVICE_STOPPED,
    KIND_STONE_HEALTH_CHANGED, KIND_STONE_LOAD_UPDATED, KIND_STONE_TENDED,
    KIND_STORAGE_CONNECTED, KIND_STORAGE_DETECTED, KIND_STORAGE_REMOVED, SSE_EMITTED_KINDS,
    ServiceStartedPayload, ServiceStoppedPayload, StorageConnectedPayload,
    StorageDetectedPayload, StorageRemovedPayload, StoneTendedPayload, wire_to_core_kind,
};
use super::event::Event;
use super::pulse::Pulse;
use super::transport::{BoxFuture, Transport};
use garden_common::presence::{
    PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload,
    event_types::PRESENCE_STREAM_PATH,
};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Shared HTTP client for SSE connections. No overall timeout because SSE
/// streams are long-lived; per-attempt error recovery is handled in-loop.
static SSE_HTTP: LazyLock<reqwest::Client> =
    LazyLock::new(|| reqwest::Client::builder().build().expect("SSE HTTP client"));

/// Exponential backoff schedule (seconds) for reconnection.
const BACKOFF_SECS: [u64; 6] = [1, 2, 4, 8, 16, 32];

// ---------------------------------------------------------------------------
// Public type
// ---------------------------------------------------------------------------

/// Consumes moss's `/presence/stream` SSE endpoint.
///
/// # Construction
///
/// ```
/// use garden_companion_sdk::garden::SseTransport;
///
/// let transport = SseTransport::new("http://localhost:7185");
/// // or with a custom path:
/// let transport = SseTransport::new("http://localhost:7185")
///     .with_path("/api/v1/stone/presence/stream");
/// ```
pub struct SseTransport {
    endpoint: String,
    path: String,
}

impl SseTransport {
    /// Construct with default presence-stream path.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            path: PRESENCE_STREAM_PATH.to_string(),
        }
    }

    /// Override the SSE path (rarely needed — defaults to
    /// [`PRESENCE_STREAM_PATH`]).
    ///
    /// [`PRESENCE_STREAM_PATH`]: garden_common::presence::event_types::PRESENCE_STREAM_PATH
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    fn url(&self) -> String {
        format!("{}{}", self.endpoint, self.path)
    }
}

impl Transport for SseTransport {
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(run_sse(self.url(), pulse, shutdown))
    }

    fn emitted_kinds(&self) -> &'static [&'static str] {
        SSE_EMITTED_KINDS
    }
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

async fn run_sse(url: String, pulse: Arc<Pulse>, shutdown: CancellationToken) {
    let mut consecutive_failures: u32 = 0;

    loop {
        if shutdown.is_cancelled() {
            return;
        }

        if consecutive_failures == 0 {
            tracing::info!(endpoint = %url, "SSE transport connecting");
        }

        let attempt = connect_and_run(&url, &pulse, &shutdown).await;

        match attempt {
            Ok(()) => {
                tracing::info!("SSE stream ended normally");
                consecutive_failures = 0;
            }
            Err(error) => {
                consecutive_failures += 1;
                let error_str = error.to_string();
                if (error_str.contains("refused") || error_str.contains("10061"))
                    && consecutive_failures <= 3
                {
                    tracing::debug!(attempt = consecutive_failures, %error, "SSE refused (starting up?)");
                } else {
                    tracing::warn!(attempt = consecutive_failures, %error, "SSE attempt failed");
                }
            }
        }

        let delay = BACKOFF_SECS
            .get((consecutive_failures as usize).saturating_sub(1))
            .copied()
            .unwrap_or(*BACKOFF_SECS.last().unwrap());

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

// ---------------------------------------------------------------------------
// One connection attempt
// ---------------------------------------------------------------------------

async fn connect_and_run(
    url: &str,
    pulse: &Arc<Pulse>,
    shutdown: &CancellationToken,
) -> anyhow::Result<()> {
    let response = SSE_HTTP
        .get(url)
        .header("Accept", "text/event-stream")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("SSE endpoint returned {}", response.status());
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    use futures_util::StreamExt;

    loop {
        tokio::select! {
            next = stream.next() => match next {
                Some(Ok(chunk)) => {
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    drain_complete_frames(&mut buffer, pulse);
                }
                Some(Err(e)) => return Err(anyhow::anyhow!("SSE read error: {e}")),
                None => return Ok(()),
            },
            _ = shutdown.cancelled() => return Ok(()),
        }
    }
}

/// Split out every complete SSE frame from the buffer (double-newline
/// delimited), parse each, and publish to `pulse`. Leaves the incomplete
/// trailing portion in the buffer.
fn drain_complete_frames(buffer: &mut String, pulse: &Arc<Pulse>) {
    while let Some(pos) = buffer.find("\n\n") {
        let frame = buffer[..pos].to_string();
        *buffer = buffer[pos + 2..].to_string();

        if let Some((wire_kind, data)) = parse_frame(&frame) {
            if let Some(event) = build_event(&wire_kind, &data) {
                let _ = pulse.ingest(event);
            } else {
                tracing::trace!(wire_kind = %wire_kind, "SSE frame skipped (unknown kind or parse error)");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frame parsing (salvaged from SseClient with the recent multi-line fix)
// ---------------------------------------------------------------------------

/// Parse an SSE frame into `(event_type, data)`. Multiple `data:` lines
/// are joined with `\n` per SSE spec (recent fix in `SseClient`). Returns
/// `None` if the frame has neither `event:` nor `data:` content.
fn parse_frame(frame: &str) -> Option<(String, String)> {
    let mut event_type = String::new();
    let mut data_lines: Vec<&str> = Vec::new();

    for line in frame.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_type = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.trim());
        }
    }

    if event_type.is_empty() && data_lines.is_empty() {
        return None;
    }
    if event_type.is_empty() {
        event_type = "message".into();
    }
    Some((event_type, data_lines.join("\n")))
}

// ---------------------------------------------------------------------------
// Kind dispatch — translate wire kind, deserialize data, build Event
// ---------------------------------------------------------------------------

/// Build an [`Event`] from a raw SSE wire frame. Returns `None` when:
///
/// - The wire kind has no canonical mapping (unknown event type), or
/// - The payload JSON fails to deserialize into the typed payload struct.
///
/// Both cases log at warn level in the calling site; here we return None
/// to keep this function pure.
fn build_event(wire_kind: &str, data: &str) -> Option<Event> {
    let core_kind = wire_to_core_kind(wire_kind)?;

    match core_kind {
        KIND_PRESENCE_SNAPSHOT => serde_json::from_str::<PresenceSnapshot>(data)
            .ok()
            .map(Event::new),
        KIND_STONE_HEALTH_CHANGED => serde_json::from_str::<StoneHealthChangedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_STONE_LOAD_UPDATED => serde_json::from_str::<StoneLoadUpdatedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_STONE_TENDED => serde_json::from_str::<StoneTendedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_SERVICE_STARTED => serde_json::from_str::<ServiceStartedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_SERVICE_STOPPED => serde_json::from_str::<ServiceStoppedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_STORAGE_CONNECTED => serde_json::from_str::<StorageConnectedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_STORAGE_DETECTED => serde_json::from_str::<StorageDetectedPayload>(data)
            .ok()
            .map(Event::new),
        KIND_STORAGE_REMOVED => serde_json::from_str::<StorageRemovedPayload>(data)
            .ok()
            .map(Event::new),
        _ => None, // unreachable — WIRE_KIND_MAP only produces known cores
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garden::PulseConfig;

    fn core_pulse() -> Arc<Pulse> {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 16,
            broadcast_capacity: 64,
        }));
        pulse.register_namespace("core");
        pulse
    }

    // --- parse_frame ---

    #[test]
    fn parse_frame_extracts_event_and_data() {
        let frame = "event: stone.load.updated\ndata: {\"cpu_percent\":50}";
        let (kind, data) = parse_frame(frame).unwrap();
        assert_eq!(kind, "stone.load.updated");
        assert_eq!(data, "{\"cpu_percent\":50}");
    }

    #[test]
    fn parse_frame_joins_multiline_data() {
        let frame = "event: x.y.z\ndata: {\"a\":\ndata: 1}";
        let (_kind, data) = parse_frame(frame).unwrap();
        assert_eq!(data, "{\"a\":\n1}");
    }

    #[test]
    fn parse_frame_defaults_event_type_when_missing() {
        let frame = "data: hello";
        let (kind, data) = parse_frame(frame).unwrap();
        assert_eq!(kind, "message");
        assert_eq!(data, "hello");
    }

    #[test]
    fn parse_frame_returns_none_for_empty() {
        assert!(parse_frame("").is_none());
        assert!(parse_frame(": heartbeat").is_none());
    }

    // --- build_event ---

    #[test]
    fn build_event_translates_wire_kind_and_deserializes() {
        let data = r#"{"health":"thriving","cpu_percent":10,"memory_percent":20}"#;
        let evt = build_event("stone.health.changed", data).unwrap();
        assert_eq!(evt.kind, "core.stone.health.changed");
        let payload = evt.payload::<StoneHealthChangedPayload>().unwrap();
        assert_eq!(payload.health, "thriving");
    }

    #[test]
    fn build_event_handles_coalescing_load_updates() {
        let data = r#"{"cpu_percent":80,"memory_percent":60,"disk_percent":40}"#;
        let evt = build_event("stone.load.updated", data).unwrap();
        assert_eq!(evt.kind, "core.stone.load.updated");
        assert!(evt.payload.is_coalescing());
    }

    #[test]
    fn build_event_skips_unknown_wire_kind() {
        let evt = build_event("totally.unknown.event", "{}");
        assert!(evt.is_none());
    }

    #[test]
    fn build_event_skips_malformed_json() {
        let evt = build_event("stone.health.changed", "not-json");
        assert!(evt.is_none());
    }

    // --- drain_complete_frames + publishing ---

    #[test]
    fn drain_publishes_complete_frames_and_keeps_partial() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: stone.tended\ndata: {\"by\":\"rake\",\"from\":\"local\"}\n\nevent: partial\ndata: {\"half",
        );
        drain_complete_frames(&mut buffer, &pulse);

        // First frame delivered.
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.stone.tended");
        let payload = delivered.payload::<StoneTendedPayload>().unwrap();
        assert_eq!(payload.by, "rake");

        // Partial second frame preserved.
        assert!(buffer.starts_with("event: partial"));
    }

    #[test]
    fn drain_publishes_multiple_frames_in_order() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: service.started\ndata: {\"service\":\"mongodb\"}\n\n\
             event: service.stopped\ndata: {\"service\":\"redis\"}\n\n\
             event: stone.tended\ndata: {\"by\":\"rake\",\"from\":\"local\"}\n\n",
        );
        drain_complete_frames(&mut buffer, &pulse);

        assert_eq!(rx.try_recv().unwrap().kind, "core.service.started");
        assert_eq!(rx.try_recv().unwrap().kind, "core.service.stopped");
        assert_eq!(rx.try_recv().unwrap().kind, "core.stone.tended");
        assert_eq!(buffer, "");
    }

    #[test]
    fn drain_ignores_unknown_kinds_without_affecting_siblings() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: something.unknown\ndata: {}\n\n\
             event: service.started\ndata: {\"service\":\"mongodb\"}\n\n",
        );
        drain_complete_frames(&mut buffer, &pulse);

        // Only the known frame gets through; the unknown is silently skipped.
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.service.started");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn drain_survives_malformed_json_in_one_frame() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: stone.health.changed\ndata: not-json\n\n\
             event: stone.tended\ndata: {\"by\":\"rake\",\"from\":\"local\"}\n\n",
        );
        drain_complete_frames(&mut buffer, &pulse);

        // Malformed frame skipped; good frame still delivered.
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.stone.tended");
    }

    #[test]
    fn published_events_are_validated_by_pulse() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: stone.tended\ndata: {\"by\":\"rake\",\"from\":\"local\"}\n\n",
        );
        drain_complete_frames(&mut buffer, &pulse);

        // The event was accepted (not rejected by Pulse's validation).
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.stone.tended");

        let metrics = pulse.metrics();
        assert_eq!(metrics.accepted, 1);
        assert_eq!(metrics.rejected_invalid_kind, 0);
        assert_eq!(metrics.rejected_unregistered_namespace, 0);
        assert_eq!(metrics.rejected_kind_payload_mismatch, 0);
    }

    #[test]
    fn coalescing_load_event_goes_to_buffer_not_subscribers() {
        let pulse = core_pulse();
        let mut rx = pulse.subscribe();

        let mut buffer = String::from(
            "event: stone.load.updated\ndata: {\"cpu_percent\":50,\"memory_percent\":40}\n\n",
        );
        drain_complete_frames(&mut buffer, &pulse);

        // Coalescing → buffered, not yet delivered.
        assert!(rx.try_recv().is_err());

        // Manual flush → delivered.
        assert_eq!(pulse.flush_coalesced(), 1);
        let delivered = rx.try_recv().unwrap();
        assert_eq!(delivered.kind, "core.stone.load.updated");
    }

    // --- Transport trait plumbing ---

    #[test]
    fn sse_transport_advertises_emitted_kinds() {
        let transport = SseTransport::new("http://localhost:7185");
        let kinds = transport.emitted_kinds();
        assert!(kinds.contains(&"core.presence.snapshot"));
        assert!(kinds.contains(&"core.stone.load.updated"));
        assert!(kinds.contains(&"core.command.invocation") == false); // SSE does not emit command events
    }

    #[test]
    fn sse_transport_builds_url() {
        let t = SseTransport::new("http://localhost:7185");
        assert_eq!(t.url(), "http://localhost:7185/api/v1/stone/presence/stream");

        let custom = SseTransport::new("http://10.0.0.1:7185").with_path("/custom");
        assert_eq!(custom.url(), "http://10.0.0.1:7185/custom");
    }

    #[tokio::test]
    async fn sse_transport_exits_on_shutdown() {
        let pulse = core_pulse();
        let token = CancellationToken::new();

        // Use an unreachable URL so connect_and_run immediately errors;
        // the backoff loop will then observe the shutdown signal.
        let transport = SseTransport::new("http://127.0.0.1:1");
        let token_cancel = token.clone();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            token_cancel.cancel();
        });

        // This must return within a reasonable time after cancellation.
        tokio::time::timeout(
            Duration::from_secs(5),
            (Box::new(transport) as Box<dyn Transport>).run(pulse, token),
        )
        .await
        .expect("SSE transport did not exit on shutdown");
        handle.await.unwrap();
    }
}
