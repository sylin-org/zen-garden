//! Unit tests for the Metrics aggregate.
//!
//! Per [domain-aggregates.md](../../../../../docs/specs/domain-aggregates.md),
//! every aggregate has a `tests.rs` with minimum coverage. Metrics has no
//! ports (no `Store`, no `Metrics`-recording-itself injection), so the
//! fake-port harness is simpler than usual: just construct a real
//! `Metrics` and exercise its methods.

use super::Metrics;
use super::event::MetricsChanged;
use super::state::{BUCKET_BOUNDS_MS, BUCKET_LABELS};
use std::sync::Arc;
use std::time::Duration;

const TEST_KINDS: &[&'static str] = &["upserted", "removed", "updated"];

// ─── Construction ──────────────────────────────────────────────────────

#[tokio::test]
async fn new_aggregate_starts_empty() {
    let m = Metrics::new();
    let snap = m.snapshot().await;

    assert!(snap.domains.is_empty(), "fresh aggregate has no domains");
    assert!(snap.tasks.is_empty(), "fresh aggregate has no tasks");
    assert_eq!(snap.global.events_total, 0);
    assert_eq!(snap.global.lag_total, 0);
    assert!(snap.global.uptime_seconds >= 0);
}

// ─── Domain registration ───────────────────────────────────────────────

#[tokio::test]
async fn register_domain_adds_entry_and_emits_event() {
    let m = Metrics::new();
    let mut rx = m.changes();

    m.register_domain("offerings", TEST_KINDS).await;

    let snap = m.domain("offerings").await.expect("domain registered");
    assert_eq!(snap.name, "offerings");
    assert_eq!(snap.events_total, 0);
    assert_eq!(snap.events_by_kind.len(), TEST_KINDS.len());
    for kind in TEST_KINDS {
        assert_eq!(snap.events_by_kind.get(*kind).copied(), Some(0));
    }

    let event = rx.recv().await.expect("event received");
    match event {
        MetricsChanged::DomainRegistered { domain } => {
            assert_eq!(domain, "offerings");
        }
        other => panic!("expected DomainRegistered, got {:?}", other),
    }
}

#[tokio::test]
async fn register_domain_twice_is_idempotent_no_duplicate_event() {
    let m = Metrics::new();
    let mut rx = m.changes();

    m.register_domain("offerings", TEST_KINDS).await;
    m.register_domain("offerings", TEST_KINDS).await;
    m.register_domain("offerings", TEST_KINDS).await;

    // Only one DomainRegistered event should fire.
    let first = rx.recv().await.expect("first event");
    assert!(matches!(first, MetricsChanged::DomainRegistered { .. }));

    // Second and third calls should produce no events. Verify by a
    // zero-delay try_recv.
    assert!(
        rx.try_recv().is_err(),
        "duplicate registration should not re-fire the event"
    );

    // State should still show exactly one domain entry.
    let snap = m.snapshot().await;
    assert_eq!(snap.domains.len(), 1);
}

// ─── Task registration ────────────────────────────────────────────────

#[tokio::test]
async fn register_task_adds_entry_and_emits_event() {
    let m = Metrics::new();
    let mut rx = m.changes();

    m.register_task("offerings-projection").await;

    let snap = m
        .task("offerings-projection")
        .await
        .expect("task registered");
    assert_eq!(snap.name, "offerings-projection");
    assert_eq!(snap.events_received_total, 0);
    assert_eq!(snap.events_lagged_total, 0);
    assert!(snap.ready_at.is_none());
    assert!(snap.last_event_at.is_none());

    let event = rx.recv().await.expect("event received");
    assert!(matches!(
        event,
        MetricsChanged::TaskRegistered {
            task: "offerings-projection"
        }
    ));
}

#[tokio::test]
async fn register_task_twice_is_idempotent() {
    let m = Metrics::new();
    let mut rx = m.changes();

    m.register_task("t1").await;
    m.register_task("t1").await;

    let _ = rx.recv().await.expect("first event");
    assert!(rx.try_recv().is_err());

    let snap = m.snapshot().await;
    assert_eq!(snap.tasks.len(), 1);
}

// ─── Domain event recording ───────────────────────────────────────────

#[tokio::test]
async fn record_domain_event_increments_total_and_kind_counters() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;

    m.record_domain_event("offerings", "upserted").await;
    m.record_domain_event("offerings", "upserted").await;
    m.record_domain_event("offerings", "removed").await;

    let snap = m.domain("offerings").await.unwrap();
    assert_eq!(snap.events_total, 3);
    assert_eq!(snap.events_by_kind.get("upserted").copied(), Some(2));
    assert_eq!(snap.events_by_kind.get("removed").copied(), Some(1));
    assert_eq!(snap.events_by_kind.get("updated").copied(), Some(0));

    let global = m.global().await;
    assert_eq!(global.events_total, 3);
}

#[tokio::test]
async fn record_domain_event_does_not_fire_events() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;
    let mut rx = m.changes();

    // Drain any domain-registered event (subscribed after registration,
    // so nothing should be pending — but drain defensively).
    let _ = rx.try_recv();

    m.record_domain_event("offerings", "upserted").await;
    m.record_domain_event("offerings", "upserted").await;
    m.record_domain_event("offerings", "removed").await;

    assert!(
        rx.try_recv().is_err(),
        "counter increments must not fire MetricsChanged events"
    );
}

#[tokio::test]
async fn record_domain_event_unknown_domain_is_silent_noop() {
    let m = Metrics::new();

    // No domain registered; this must not panic.
    m.record_domain_event("nonexistent", "upserted").await;

    let snap = m.snapshot().await;
    assert!(snap.domains.is_empty());
    assert_eq!(snap.global.events_total, 0);
}

#[tokio::test]
async fn record_domain_event_unknown_kind_still_increments_total() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;

    // Unknown kind — total increments, per-kind does not.
    m.record_domain_event("offerings", "mysterious").await;

    let snap = m.domain("offerings").await.unwrap();
    assert_eq!(snap.events_total, 1);
    assert!(snap.events_by_kind.get("mysterious").is_none());
    assert_eq!(snap.events_by_kind.get("upserted").copied(), Some(0));
}

// ─── Latency histogram ────────────────────────────────────────────────

#[tokio::test]
async fn record_mutation_latency_places_observations_in_correct_buckets() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;

    // One observation in each bucket.
    m.record_mutation_latency("offerings", Duration::from_millis(0))
        .await; // → 1ms
    m.record_mutation_latency("offerings", Duration::from_millis(3))
        .await; // → 5ms
    m.record_mutation_latency("offerings", Duration::from_millis(8))
        .await; // → 10ms
    m.record_mutation_latency("offerings", Duration::from_millis(30))
        .await; // → 50ms
    m.record_mutation_latency("offerings", Duration::from_millis(80))
        .await; // → 100ms
    m.record_mutation_latency("offerings", Duration::from_millis(300))
        .await; // → 500ms
    m.record_mutation_latency("offerings", Duration::from_millis(800))
        .await; // → 1s
    m.record_mutation_latency("offerings", Duration::from_millis(3000))
        .await; // → 5s
    m.record_mutation_latency("offerings", Duration::from_millis(10_000))
        .await; // → +Inf

    let snap = m.domain("offerings").await.unwrap();
    let lat = snap.mutation_latency;
    assert_eq!(lat.count, 9);
    for label in BUCKET_LABELS {
        assert_eq!(
            lat.buckets.get(label).copied(),
            Some(1),
            "bucket {} should have exactly 1 observation",
            label
        );
    }
    assert!(lat.mean_ms.is_some());
}

#[tokio::test]
async fn record_mutation_latency_tracks_count_and_total() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;

    m.record_mutation_latency("offerings", Duration::from_millis(10))
        .await;
    m.record_mutation_latency("offerings", Duration::from_millis(20))
        .await;
    m.record_mutation_latency("offerings", Duration::from_millis(30))
        .await;

    let snap = m.domain("offerings").await.unwrap();
    let lat = snap.mutation_latency;
    assert_eq!(lat.count, 3);
    assert_eq!(lat.total_ms, 60);
    assert_eq!(lat.mean_ms, Some(20.0));
}

#[test]
fn bucket_bounds_and_labels_are_consistent() {
    // There must be exactly one more label than bucket bound (the
    // extra one is "+Inf").
    assert_eq!(BUCKET_LABELS.len(), BUCKET_BOUNDS_MS.len() + 1);
    assert_eq!(*BUCKET_LABELS.last().unwrap(), "+Inf");
}

// ─── Task lifecycle transitions ───────────────────────────────────────

#[tokio::test]
async fn record_task_ready_fires_event_once() {
    let m = Metrics::new();
    m.register_task("t1").await;
    let mut rx = m.changes();
    let _ = rx.try_recv(); // drain TaskRegistered

    m.record_task_ready("t1").await;

    let event = rx.recv().await.expect("event received");
    match event {
        MetricsChanged::TaskReady { task, ready_at: _ } => assert_eq!(task, "t1"),
        other => panic!("expected TaskReady, got {:?}", other),
    }

    let snap = m.task("t1").await.unwrap();
    assert!(snap.ready_at.is_some());

    // Calling ready again is idempotent — no duplicate event.
    m.record_task_ready("t1").await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn record_task_transition_fires_event() {
    let m = Metrics::new();
    m.register_task("t1").await;
    let mut rx = m.changes();
    let _ = rx.try_recv();

    m.record_task_transition("t1", "running").await;
    m.record_task_transition("t1", "completed").await;

    let e1 = rx.recv().await.unwrap();
    assert!(matches!(
        e1,
        MetricsChanged::TaskStateChanged {
            task: "t1",
            state: "running"
        }
    ));
    let e2 = rx.recv().await.unwrap();
    assert!(matches!(
        e2,
        MetricsChanged::TaskStateChanged {
            task: "t1",
            state: "completed"
        }
    ));
}

#[tokio::test]
async fn record_subscriber_lag_increments_counters_and_fires_event() {
    let m = Metrics::new();
    m.register_task("t1").await;
    let mut rx = m.changes();
    let _ = rx.try_recv();

    m.record_subscriber_lag("t1", 42).await;
    m.record_subscriber_lag("t1", 7).await;

    let snap_task = m.task("t1").await.unwrap();
    assert_eq!(snap_task.events_lagged_total, 49);

    let snap_global = m.global().await;
    assert_eq!(snap_global.lag_total, 49);

    // Both lag events fire MetricsChanged::SubscriberLagDetected.
    let e1 = rx.recv().await.unwrap();
    match e1 {
        MetricsChanged::SubscriberLagDetected { task, skipped } => {
            assert_eq!(task, "t1");
            assert_eq!(skipped, 42);
        }
        other => panic!("expected SubscriberLagDetected, got {:?}", other),
    }
    let e2 = rx.recv().await.unwrap();
    assert!(matches!(
        e2,
        MetricsChanged::SubscriberLagDetected {
            task: "t1",
            skipped: 7
        }
    ));
}

// ─── Snapshot completeness ────────────────────────────────────────────

#[tokio::test]
async fn snapshot_includes_all_registered_domains_and_tasks() {
    let m = Metrics::new();

    m.register_domain("offerings", TEST_KINDS).await;
    m.register_domain("tool", &["upserted"]).await;
    m.register_task("t1").await;
    m.register_task("t2").await;

    let snap = m.snapshot().await;
    assert_eq!(snap.domains.len(), 2);
    assert_eq!(snap.tasks.len(), 2);

    let domain_names: Vec<&str> = snap.domains.iter().map(|d| d.name.as_str()).collect();
    assert!(domain_names.contains(&"offerings"));
    assert!(domain_names.contains(&"tool"));

    let task_names: Vec<&str> = snap.tasks.iter().map(|t| t.name.as_str()).collect();
    assert!(task_names.contains(&"t1"));
    assert!(task_names.contains(&"t2"));
}

#[tokio::test]
async fn snapshot_is_deterministic_for_events_by_kind_order() {
    let m = Metrics::new();
    m.register_domain("offerings", TEST_KINDS).await;
    m.record_domain_event("offerings", "upserted").await;
    m.record_domain_event("offerings", "removed").await;

    let snap = m.domain("offerings").await.unwrap();
    let keys: Vec<&String> = snap.events_by_kind.keys().collect();
    // BTreeMap is sorted — deterministic for JSON output.
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}

// ─── Concurrency ──────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_record_from_multiple_tasks_no_contention() {
    let m = Arc::new(Metrics::new());
    m.register_domain("offerings", TEST_KINDS).await;

    let mut handles = Vec::new();
    const TASKS: usize = 10;
    const RECORDS_PER_TASK: usize = 100;

    for _ in 0..TASKS {
        let m = m.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..RECORDS_PER_TASK {
                m.record_domain_event("offerings", "upserted").await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let snap = m.domain("offerings").await.unwrap();
    assert_eq!(snap.events_total, (TASKS * RECORDS_PER_TASK) as u64);
    assert_eq!(
        snap.events_by_kind.get("upserted").copied(),
        Some((TASKS * RECORDS_PER_TASK) as u64)
    );
}

// ─── Unregistered access returns None ─────────────────────────────────

#[tokio::test]
async fn domain_query_for_unregistered_name_returns_none() {
    let m = Metrics::new();
    assert!(m.domain("nonexistent").await.is_none());
}

#[tokio::test]
async fn task_query_for_unregistered_name_returns_none() {
    let m = Metrics::new();
    assert!(m.task("nonexistent").await.is_none());
}
