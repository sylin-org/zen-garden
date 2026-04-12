//! Unit tests for the `Subsystems` aggregate.

use std::sync::Arc;

use super::aggregate::Subsystems;
use super::event::ChangeKind;
use crate::domain::Metrics;

fn test_metrics() -> Arc<Metrics> {
    Arc::new(Metrics::new())
}

async fn make_subsystems(names: &[&str]) -> Subsystems {
    let mut subs = Subsystems::new(test_metrics()).await;
    for name in names {
        subs.register(*name);
    }
    subs
}

#[tokio::test]
async fn register_and_query_initial_state() {
    let subs = make_subsystems(&["network", "docker"]).await;
    assert!(!subs.is_ready("network"));
    assert!(!subs.is_ready("docker"));
}

#[tokio::test]
async fn mark_ready_transitions() {
    let subs = make_subsystems(&["network"]).await;
    assert!(!subs.is_ready("network"));

    subs.mark_ready("network").await;
    assert!(subs.is_ready("network"));
}

#[tokio::test]
async fn mark_unready_transitions() {
    let subs = make_subsystems(&["docker"]).await;
    subs.mark_ready("docker").await;
    assert!(subs.is_ready("docker"));

    subs.mark_unready("docker", "daemon disconnected").await;
    assert!(!subs.is_ready("docker"));
}

#[tokio::test]
async fn mark_ready_idempotent_no_duplicate_event() {
    let subs = make_subsystems(&["network"]).await;
    let mut rx = subs.changes();

    subs.mark_ready("network").await;
    subs.mark_ready("network").await; // idempotent — should NOT fire second event

    // Should have exactly one event
    let event = rx.try_recv().expect("should have one event");
    assert!(matches!(event.kind, ChangeKind::Ready { ref name } if name == "network"));

    // No second event
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn mark_unready_idempotent_no_duplicate_event() {
    let subs = make_subsystems(&["docker"]).await;
    let mut rx = subs.changes();

    // Already not ready — no event
    subs.mark_unready("docker", "test").await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn ready_unready_cycle_fires_both_events() {
    let subs = make_subsystems(&["network"]).await;
    let mut rx = subs.changes();

    subs.mark_ready("network").await;
    subs.mark_unready("network", "cable unplugged").await;

    let e1 = rx.try_recv().expect("ready event");
    assert!(matches!(e1.kind, ChangeKind::Ready { ref name } if name == "network"));

    let e2 = rx.try_recv().expect("unready event");
    assert!(matches!(e2.kind, ChangeKind::Unready { ref name, ref reason }
        if name == "network" && reason == "cable unplugged"));
}

#[tokio::test]
async fn unknown_subsystem_is_ready_returns_false() {
    let subs = make_subsystems(&[]).await;
    assert!(!subs.is_ready("nonexistent"));
}

#[tokio::test]
async fn unknown_subsystem_mark_ready_is_noop() {
    let subs = make_subsystems(&[]).await;
    let mut rx = subs.changes();

    subs.mark_ready("nonexistent").await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn unknown_subsystem_mark_unready_is_noop() {
    let subs = make_subsystems(&[]).await;
    let mut rx = subs.changes();

    subs.mark_unready("nonexistent", "reason").await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn snapshot_returns_all_subsystems() {
    let subs = make_subsystems(&["network", "docker"]).await;
    subs.mark_ready("network").await;

    let snap = subs.snapshot();
    assert_eq!(snap.len(), 2);

    let network = snap.iter().find(|s| s.name == "network").unwrap();
    assert!(network.ready);

    let docker = snap.iter().find(|s| s.name == "docker").unwrap();
    assert!(!docker.ready);
}

#[tokio::test]
#[should_panic(expected = "already registered")]
async fn duplicate_registration_panics() {
    let mut subs = Subsystems::new(test_metrics()).await;
    subs.register("network");
    subs.register("network");
}

#[tokio::test]
async fn wait_ready_returns_immediately_when_ready() {
    let subs = make_subsystems(&["network"]).await;
    subs.mark_ready("network").await;

    let result = subs.wait_ready("network").await;
    assert!(result);
}

#[tokio::test]
async fn wait_ready_unknown_returns_false() {
    let subs = make_subsystems(&[]).await;
    let result = subs.wait_ready("nonexistent").await;
    assert!(!result);
}

#[tokio::test]
async fn wait_ready_resolves_on_transition() {
    let subs = Arc::new(make_subsystems(&["docker"]).await);
    let subs_clone = subs.clone();

    let handle = tokio::spawn(async move {
        subs_clone.wait_ready("docker").await
    });

    // Small delay to let the waiter subscribe
    tokio::task::yield_now().await;
    subs.mark_ready("docker").await;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        handle,
    )
    .await
    .expect("should not timeout")
    .expect("task should not panic");

    assert!(result);
}

#[tokio::test]
async fn change_kind_names() {
    assert_eq!(
        ChangeKind::Ready { name: "x".into() }.name(),
        "Ready"
    );
    assert_eq!(
        ChangeKind::Unready { name: "x".into(), reason: "y".into() }.name(),
        "Unready"
    );
    assert_eq!(ChangeKind::ALL_NAMES, &["Ready", "Unready"]);
}
