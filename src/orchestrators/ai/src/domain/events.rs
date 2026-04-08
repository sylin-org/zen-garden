//! Event bus — the orchestrator's unified nervous system (ORCH-0030 §1).
//!
//! One `EventBus` per `AppState`. Every domain publishes state
//! transitions here; every subscriber (HTTP `/v1/events`, dashboards,
//! internal background tasks) filters by glob-based focus patterns.
//!
//! Design points:
//!
//! - **Topic grammar mirrors URL paths.** A client that knows how to
//!   ask for a resource already knows how to subscribe to its events.
//!   See [`Topic`] for the supported glob syntax.
//!
//! - **Monotonic sequence numbers** power `Last-Event-ID` resume.
//!   History is bounded; clients that fall behind the ring receive a
//!   synthetic `resume.gap` event instructing them to re-fetch state.
//!
//! - **One broadcast channel, per-subscriber filter.** The bus does
//!   not maintain per-topic channels. It publishes every event to one
//!   `broadcast::Sender`, and each subscriber applies its own
//!   [`FocusMatcher`] before writing to the wire.
//!
//! - **Transitions, not state — with one exception.** The bus carries
//!   deltas; authoritative state lives in REST endpoints. The sole
//!   exception is `resources.stone.*.snapshot` (future commit 4) for
//!   dashboard gauges.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};

/// Default capacity of the bus's broadcast channel. Large enough that
/// typical bursts don't lag subscribers; small enough that a stuck
/// subscriber is evicted quickly.
pub const BROADCAST_CAPACITY: usize = 2048;

/// Default history ring size. Approximately 30 seconds of activity at
/// expected event rates. Tunable via `EventBus::with_capacity`.
pub const HISTORY_CAPACITY: usize = 4096;

/// A published event.
///
/// Topics are immutable strings; payloads are serialized to
/// `serde_json::Value` at publish time so the bus remains
/// heterogeneous. Type safety lives at the publisher and subscriber
/// boundaries, not inside the channel.
#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub seq: u64,
    pub topic: String,
    pub at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

impl Event {
    pub fn new(seq: u64, topic: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            seq,
            topic: topic.into(),
            at: Utc::now(),
            payload,
        }
    }
}

/// The central event bus.
pub struct EventBus {
    seq: AtomicU64,
    history: RwLock<VecDeque<Event>>,
    history_capacity: usize,
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Arc<Self> {
        Self::with_capacity(BROADCAST_CAPACITY, HISTORY_CAPACITY)
    }

    pub fn with_capacity(broadcast_cap: usize, history_cap: usize) -> Arc<Self> {
        let (tx, _) = broadcast::channel(broadcast_cap);
        Arc::new(Self {
            seq: AtomicU64::new(0),
            history: RwLock::new(VecDeque::with_capacity(history_cap)),
            history_capacity: history_cap,
            tx,
        })
    }

    /// Publish a typed payload. The payload is serialized to JSON once
    /// at publish time. If serialization fails the event is dropped
    /// (this should never happen for well-formed domain types — serde
    /// errors on `Serialize` impls are an ICE-level bug in the
    /// publisher, not a runtime condition to handle).
    pub async fn publish<T: Serialize>(&self, topic: impl Into<String>, payload: &T) {
        let payload = match serde_json::to_value(payload) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "event bus: failed to serialize payload; dropping event");
                return;
            }
        };
        self.publish_raw(topic, payload).await;
    }

    /// Publish a pre-serialized payload. Prefer [`publish`] when the
    /// payload is a typed domain event.
    pub async fn publish_raw(&self, topic: impl Into<String>, payload: serde_json::Value) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let event = Event::new(seq, topic, payload);

        // Push to history, evicting the oldest entry if the ring is full.
        {
            let mut history = self.history.write().await;
            if history.len() == self.history_capacity {
                history.pop_front();
            }
            history.push_back(event.clone());
        }

        // Broadcast to live subscribers. If no subscribers are
        // connected, the send returns an error we silently ignore —
        // the event is still in history for future subscribers.
        let _ = self.tx.send(event);
    }

    /// Current (highest assigned) sequence number. The next event will
    /// carry `current_seq() + 1`.
    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Raw broadcast subscription — no focus filter, no replay, no
    /// gap detection. Used by internal consumers (like the Directory
    /// subscriber) that want every event and handle their own
    /// filtering. External clients should use [`subscribe`] instead.
    pub fn raw_subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Subscribe with a focus matcher and an optional resume point.
    ///
    /// Returns a [`Subscription`] that yields events matching the
    /// focus. If `since` is `Some(seq)`, the subscription first
    /// replays matching history entries with `event.seq > since`,
    /// then transitions to live tailing.
    ///
    /// If `since` is older than the oldest history entry, the
    /// subscription emits a synthetic [`Event::topic = "bus.resume.gap"`]
    /// event carrying `{requested, oldest}` before the live tail.
    pub async fn subscribe(
        self: &Arc<Self>,
        focus: FocusMatcher,
        since: Option<u64>,
    ) -> Subscription {
        let live_rx = self.tx.subscribe();
        let (replay, gap) = if let Some(since) = since {
            self.replay_since(since, &focus).await
        } else {
            (Vec::new(), None)
        };

        Subscription {
            bus: Arc::clone(self),
            focus,
            replay,
            gap,
            live_rx,
        }
    }

    async fn replay_since(
        &self,
        since: u64,
        focus: &FocusMatcher,
    ) -> (Vec<Event>, Option<Event>) {
        let history = self.history.read().await;
        let oldest = history.front().map(|e| e.seq);
        let gap = match oldest {
            Some(oldest_seq) if oldest_seq > since + 1 => Some(Event::new(
                self.seq.load(Ordering::SeqCst),
                "bus.resume.gap",
                serde_json::json!({
                    "requested": since,
                    "oldest": oldest_seq,
                }),
            )),
            _ => None,
        };
        let replay: Vec<Event> = history
            .iter()
            .filter(|e| e.seq > since && focus.matches(&e.topic))
            .cloned()
            .collect();
        (replay, gap)
    }
}

/// A live subscription to the bus.
///
/// Callers drive this by calling [`Subscription::recv`] in a loop.
/// The subscription owns its own replay buffer (drained first) and
/// then tails the broadcast channel.
pub struct Subscription {
    bus: Arc<EventBus>,
    focus: FocusMatcher,
    replay: Vec<Event>,
    gap: Option<Event>,
    live_rx: broadcast::Receiver<Event>,
}

/// What the subscription yielded.
pub enum SubscriptionEvent {
    /// An event (either from replay or from the live tail).
    Event(Event),
    /// The broadcast channel dropped one or more events because this
    /// subscriber couldn't keep up. The subscriber may want to
    /// re-sync via REST before trusting subsequent events.
    Lagged(u64),
    /// The bus was dropped (orchestrator shutting down).
    Closed,
}

impl Subscription {
    /// Yield the next matching event. Drains the gap marker, then the
    /// replay buffer, then tails the live channel.
    pub async fn recv(&mut self) -> SubscriptionEvent {
        // 1. Emit the gap marker first, if any.
        if let Some(gap) = self.gap.take() {
            return SubscriptionEvent::Event(gap);
        }

        // 2. Drain the replay buffer.
        if !self.replay.is_empty() {
            return SubscriptionEvent::Event(self.replay.remove(0));
        }

        // 3. Tail the live channel, filtering by focus.
        loop {
            match self.live_rx.recv().await {
                Ok(event) => {
                    if self.focus.matches(&event.topic) {
                        return SubscriptionEvent::Event(event);
                    }
                    // Not a match — keep waiting.
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    return SubscriptionEvent::Lagged(n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return SubscriptionEvent::Closed;
                }
            }
        }
    }
}

// ── Focus matcher ─────────────────────────────────────────────

/// A set of dotted-glob patterns compiled into a single matcher.
///
/// Supported glob syntax (per ORCH-0030 §1.2):
/// - `*` — matches any single segment (e.g., `skills.*.named`)
/// - `**` — matches zero or more segments (e.g., `skills.**`)
/// - `{a,b}` — not supported; use multiple patterns instead
/// - literal segments — match exactly
///
/// Multiple patterns are OR'd together: an event matches the set if it
/// matches any individual pattern.
#[derive(Debug, Clone, Default)]
pub struct FocusMatcher {
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
struct Pattern {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    Star,       // matches one segment
    DoubleStar, // matches zero or more segments
}

impl FocusMatcher {
    /// A matcher that matches everything. Used for internal consumers
    /// that want the full firehose.
    pub fn any() -> Self {
        Self::from_patterns(&["**"]).expect("`**` is a valid pattern")
    }

    /// Parse a comma-separated list of patterns into a matcher.
    ///
    /// Empty strings are skipped. If the result has no patterns, the
    /// matcher is empty (matches nothing).
    pub fn parse(raw: &str) -> Result<Self, FocusError> {
        let parts: Vec<&str> = raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        Self::from_patterns(&parts)
    }

    pub fn from_patterns(patterns: &[&str]) -> Result<Self, FocusError> {
        let mut compiled = Vec::with_capacity(patterns.len());
        for raw in patterns {
            compiled.push(parse_pattern(raw)?);
        }
        Ok(Self { patterns: compiled })
    }

    /// Is this matcher empty (no patterns)?
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Check whether a topic matches any of the compiled patterns.
    pub fn matches(&self, topic: &str) -> bool {
        let segs: Vec<&str> = topic.split('.').collect();
        self.patterns.iter().any(|p| match_pattern(&p.segments, &segs))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FocusError {
    #[error("empty pattern segment in `{0}`")]
    EmptySegment(String),
    #[error("invalid pattern `{0}`: {1}")]
    Invalid(String, String),
}

fn parse_pattern(raw: &str) -> Result<Pattern, FocusError> {
    if raw.is_empty() {
        return Err(FocusError::Invalid(
            raw.to_string(),
            "empty pattern".to_string(),
        ));
    }
    let mut segments = Vec::new();
    for seg in raw.split('.') {
        if seg.is_empty() {
            return Err(FocusError::EmptySegment(raw.to_string()));
        }
        segments.push(match seg {
            "*" => Segment::Star,
            "**" => Segment::DoubleStar,
            s => Segment::Literal(s.to_string()),
        });
    }
    Ok(Pattern { segments })
}

/// Greedy backtracking matcher for dotted segments.
fn match_pattern(pat: &[Segment], input: &[&str]) -> bool {
    match (pat.first(), input.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(Segment::DoubleStar), _) => {
            // `**` consumes 0..=input.len() segments; try each split.
            for i in 0..=input.len() {
                if match_pattern(&pat[1..], &input[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(_), None) => false,
        (Some(Segment::Star), Some(_)) => match_pattern(&pat[1..], &input[1..]),
        (Some(Segment::Literal(lit)), Some(seg)) => {
            if lit == seg {
                match_pattern(&pat[1..], &input[1..])
            } else {
                false
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matcher_literal() {
        let m = FocusMatcher::from_patterns(&["skills.foo.named"]).unwrap();
        assert!(m.matches("skills.foo.named"));
        assert!(!m.matches("skills.foo.state"));
        assert!(!m.matches("skills.bar.named"));
    }

    #[test]
    fn matcher_single_star() {
        let m = FocusMatcher::from_patterns(&["skills.*.named"]).unwrap();
        assert!(m.matches("skills.foo.named"));
        assert!(m.matches("skills.bar.named"));
        assert!(!m.matches("skills.foo.state"));
        assert!(!m.matches("skills.foo.bar.named")); // * is ONE segment
    }

    #[test]
    fn matcher_double_star_suffix() {
        let m = FocusMatcher::from_patterns(&["skills.**"]).unwrap();
        assert!(m.matches("skills.foo"));
        assert!(m.matches("skills.foo.named"));
        assert!(m.matches("skills.foo.models.progress"));
        assert!(!m.matches("jobs.foo"));
    }

    #[test]
    fn matcher_double_star_prefix() {
        let m = FocusMatcher::from_patterns(&["**.failed"]).unwrap();
        assert!(m.matches("jobs.abc.failed"));
        assert!(m.matches("skills.foo.failed"));
        assert!(m.matches("failed"));
        assert!(!m.matches("jobs.abc.completed"));
    }

    #[test]
    fn matcher_multi_pattern() {
        let m = FocusMatcher::from_patterns(&["skills.**", "jobs.**"]).unwrap();
        assert!(m.matches("skills.foo.named"));
        assert!(m.matches("jobs.abc.state"));
        assert!(!m.matches("catalog.version"));
    }

    #[test]
    fn matcher_parse_comma_separated() {
        let m = FocusMatcher::parse("skills.*, jobs.*").unwrap();
        assert!(m.matches("skills.foo"));
        assert!(m.matches("jobs.abc"));
        assert!(!m.matches("catalog.version"));
    }

    #[test]
    fn matcher_empty_is_empty() {
        let m = FocusMatcher::parse("").unwrap();
        assert!(m.is_empty());
        assert!(!m.matches("anything"));
    }

    #[test]
    fn matcher_any_matches_everything() {
        let m = FocusMatcher::any();
        assert!(m.matches("anything"));
        assert!(m.matches("skills.foo.bar.baz"));
        assert!(m.matches(""));
    }

    #[tokio::test]
    async fn bus_publishes_sequentially() {
        let bus = EventBus::new();
        bus.publish_raw("test.a", serde_json::json!({})).await;
        bus.publish_raw("test.b", serde_json::json!({})).await;
        bus.publish_raw("test.c", serde_json::json!({})).await;
        assert_eq!(bus.current_seq(), 3);
    }

    #[tokio::test]
    async fn bus_delivers_to_subscriber() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe(FocusMatcher::any(), None).await;

        bus.publish_raw("test.a", serde_json::json!({})).await;

        match sub.recv().await {
            SubscriptionEvent::Event(e) => {
                assert_eq!(e.topic, "test.a");
                assert_eq!(e.seq, 1);
            }
            _ => panic!("expected event"),
        }
    }

    #[tokio::test]
    async fn bus_filters_by_focus() {
        let bus = EventBus::new();
        let matcher = FocusMatcher::parse("skills.*").unwrap();
        let mut sub = bus.subscribe(matcher, None).await;

        bus.publish_raw("catalog.version", serde_json::json!({})).await;
        bus.publish_raw("skills.foo", serde_json::json!({"a": 1})).await;

        match sub.recv().await {
            SubscriptionEvent::Event(e) => {
                assert_eq!(e.topic, "skills.foo");
                assert_eq!(e.payload, serde_json::json!({"a": 1}));
            }
            _ => panic!("expected event"),
        }
    }

    #[tokio::test]
    async fn bus_replays_from_since() {
        let bus = EventBus::new();
        bus.publish_raw("a", serde_json::json!({"n": 1})).await;
        bus.publish_raw("b", serde_json::json!({"n": 2})).await;
        bus.publish_raw("c", serde_json::json!({"n": 3})).await;

        // Subscribe asking for everything after seq=1.
        let mut sub = bus.subscribe(FocusMatcher::any(), Some(1)).await;

        // Should replay seq=2 and seq=3.
        let e1 = match sub.recv().await {
            SubscriptionEvent::Event(e) => e,
            _ => panic!(),
        };
        assert_eq!(e1.topic, "b");
        assert_eq!(e1.seq, 2);

        let e2 = match sub.recv().await {
            SubscriptionEvent::Event(e) => e,
            _ => panic!(),
        };
        assert_eq!(e2.topic, "c");
        assert_eq!(e2.seq, 3);
    }

    #[tokio::test]
    async fn bus_resume_gap_when_history_exceeded() {
        let bus = EventBus::with_capacity(32, 3);
        for i in 0..10 {
            bus.publish_raw(format!("event.{i}"), serde_json::json!({"i": i}))
                .await;
        }

        // Subscribe from stale seq. History only has the last 3
        // events (seq 8, 9, 10); requesting since=2 should yield
        // a gap event first.
        let mut sub = bus.subscribe(FocusMatcher::any(), Some(2)).await;

        match sub.recv().await {
            SubscriptionEvent::Event(e) => {
                assert_eq!(e.topic, "bus.resume.gap");
                let p = e.payload.as_object().unwrap();
                assert_eq!(p["requested"], 2);
                assert_eq!(p["oldest"], 8);
            }
            _ => panic!("expected gap event"),
        }
    }

    #[tokio::test]
    async fn publish_with_typed_payload() {
        #[derive(Serialize)]
        struct Named {
            display_name: String,
        }

        let bus = EventBus::new();
        let mut sub = bus.subscribe(FocusMatcher::any(), None).await;

        bus.publish(
            "skills.foo.named",
            &Named {
                display_name: "Foo".to_string(),
            },
        )
        .await;

        match sub.recv().await {
            SubscriptionEvent::Event(e) => {
                assert_eq!(e.topic, "skills.foo.named");
                assert_eq!(e.payload["display_name"], "Foo");
            }
            _ => panic!("expected event"),
        }
    }
}
