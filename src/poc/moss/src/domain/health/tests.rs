//! Unit tests for the Health bounded context.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use garden_common::{OfferingStatus, ServiceHealthStatus};
use tokio::sync::Mutex;

use super::aggregate::{Health, ProbeOutcome};
use super::event::HealthChangeKind;
use super::probe::{HealthProbe, HealthProbeResult};
use crate::domain::Metrics;

// ============================================================================
// Fake probe adapter
// ============================================================================

struct FakeHealthProbe {
    responses: Mutex<Vec<Result<HealthProbeResult>>>,
}

impl FakeHealthProbe {
    fn new(responses: Vec<Result<HealthProbeResult>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }

    fn healthy() -> Self {
        Self::new(vec![Ok(HealthProbeResult {
            status: OfferingStatus::Running,
            health: ServiceHealthStatus::Healthy,
        })])
    }

    fn degraded() -> Self {
        Self::new(vec![Ok(HealthProbeResult {
            status: OfferingStatus::Running,
            health: ServiceHealthStatus::Degraded,
        })])
    }

    fn failing() -> Self {
        Self::new(vec![Err(anyhow::anyhow!("connection refused"))])
    }
}

impl HealthProbe for FakeHealthProbe {
    fn probe<'a>(
        &'a self,
        _name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<HealthProbeResult>> + Send + 'a>> {
        Box::pin(async move {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                Ok(HealthProbeResult {
                    status: OfferingStatus::Stopped,
                    health: ServiceHealthStatus::Offline,
                })
            } else {
                responses.remove(0)
            }
        })
    }
}

// ============================================================================
// Test helpers
// ============================================================================

fn test_metrics() -> Arc<Metrics> {
    Arc::new(Metrics::new())
}

async fn test_health(probe: impl HealthProbe + 'static) -> Health {
    Health::new(test_metrics(), Arc::new(probe)).await
}

async fn test_offerings() -> Arc<crate::domain::Offerings> {
    Arc::new(
        crate::domain::Offerings::new(
            Vec::new(),
            Vec::new(),
            Arc::new(crate::domain::offerings::NoopOfferingStore),
            test_metrics(),
        )
        .await,
    )
}

fn make_offering(
    name: &str,
    status: OfferingStatus,
    health: ServiceHealthStatus,
) -> garden_common::Offering {
    // Build via serde roundtrip — only required fields in JSON, rest
    // use serde defaults. Avoids coupling to every struct field.
    let json = serde_json::json!({
        "offering_id": format!("test-{}", name),
        "name": name,
        "offering": name,
        "status": status,
        "health": health,
        "location": { "host": "localhost", "port": 0, "protocol": "tcp" },
        "mode_data": { "mode": "managed" },
        "registered_at": chrono::Utc::now().to_rfc3339(),
    });
    serde_json::from_value(json).expect("test offering construction")
}

async fn seed_offering(
    offerings: &Arc<crate::domain::Offerings>,
    name: &str,
    status: OfferingStatus,
    health: ServiceHealthStatus,
) -> String {
    let offering = make_offering(name, status, health);
    let id = offering.offering_id.clone();
    offerings.upsert(offering).await;
    id
}

// ============================================================================
// Event classification tests
// ============================================================================

#[test]
fn classify_same_state_returns_none() {
    assert!(
        super::event::classify_transition(
            &ServiceHealthStatus::Healthy,
            &ServiceHealthStatus::Healthy
        )
        .is_none()
    );
    assert!(
        super::event::classify_transition(
            &ServiceHealthStatus::Offline,
            &ServiceHealthStatus::Offline
        )
        .is_none()
    );
}

#[test]
fn classify_healthy_to_degraded() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Healthy,
        &ServiceHealthStatus::Degraded,
    );
    assert_eq!(kind, Some(HealthChangeKind::Degraded));
}

#[test]
fn classify_healthy_to_offline() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Healthy,
        &ServiceHealthStatus::Offline,
    );
    assert_eq!(kind, Some(HealthChangeKind::Failed));
}

#[test]
fn classify_offline_to_healthy() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Offline,
        &ServiceHealthStatus::Healthy,
    );
    assert_eq!(kind, Some(HealthChangeKind::Recovered));
}

#[test]
fn classify_degraded_to_healthy() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Degraded,
        &ServiceHealthStatus::Healthy,
    );
    assert_eq!(kind, Some(HealthChangeKind::Recovered));
}

#[test]
fn classify_degraded_to_offline() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Degraded,
        &ServiceHealthStatus::Offline,
    );
    assert_eq!(kind, Some(HealthChangeKind::Failed));
}

#[test]
fn classify_offline_to_degraded() {
    let kind = super::event::classify_transition(
        &ServiceHealthStatus::Offline,
        &ServiceHealthStatus::Degraded,
    );
    assert_eq!(kind, Some(HealthChangeKind::Degraded));
}

// ============================================================================
// HealthChangeKind names
// ============================================================================

#[test]
fn all_kind_names_match_variants() {
    assert_eq!(HealthChangeKind::Recovered.name(), "recovered");
    assert_eq!(HealthChangeKind::Degraded.name(), "degraded");
    assert_eq!(HealthChangeKind::Failed.name(), "failed");
    assert!(HealthChangeKind::ALL_NAMES.contains(&"probed"));
}

// ============================================================================
// Aggregate probe tests
// ============================================================================

#[tokio::test]
async fn probe_no_change_returns_unchanged() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let outcome = health
        .probe_offering(
            &offerings,
            "redis",
            &id,
            OfferingStatus::Running,
            &ServiceHealthStatus::Healthy,
        )
        .await;

    assert_eq!(outcome, ProbeOutcome::Unchanged);
}

#[tokio::test]
async fn probe_health_change_emits_event() {
    let health = test_health(FakeHealthProbe::degraded()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "mongo",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let mut rx = health.changes();

    let outcome = health
        .probe_offering(
            &offerings,
            "mongo",
            &id,
            OfferingStatus::Running,
            &ServiceHealthStatus::Healthy,
        )
        .await;

    assert!(outcome.is_changed());

    let event = rx.try_recv().expect("should have received event");
    assert_eq!(event.kind, HealthChangeKind::Degraded);
    assert_eq!(event.offering, "mongo");
}

#[tokio::test]
async fn probe_failure_marks_offline() {
    let health = test_health(FakeHealthProbe::failing()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let mut rx = health.changes();

    let outcome = health
        .probe_offering(
            &offerings,
            "redis",
            &id,
            OfferingStatus::Running,
            &ServiceHealthStatus::Healthy,
        )
        .await;

    assert!(outcome.is_changed());
    if let ProbeOutcome::Changed {
        new_status,
        new_health,
    } = outcome
    {
        assert_eq!(new_status, OfferingStatus::Stopped);
        assert_eq!(new_health, ServiceHealthStatus::Offline);
    }

    let event = rx.try_recv().expect("should have received event");
    assert_eq!(event.kind, HealthChangeKind::Failed);
}

#[tokio::test]
async fn probe_recovery_emits_recovered() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "postgres",
        OfferingStatus::Stopped,
        ServiceHealthStatus::Offline,
    )
    .await;

    let mut rx = health.changes();

    let outcome = health
        .probe_offering(
            &offerings,
            "postgres",
            &id,
            OfferingStatus::Stopped,
            &ServiceHealthStatus::Offline,
        )
        .await;

    assert!(outcome.is_changed());

    let event = rx.try_recv().expect("should have received event");
    assert_eq!(event.kind, HealthChangeKind::Recovered);
}

// ============================================================================
// Docker event application tests
// ============================================================================

#[tokio::test]
async fn apply_docker_event_start() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "nginx",
        OfferingStatus::Stopped,
        ServiceHealthStatus::Offline,
    )
    .await;

    let mut rx = health.changes();

    let changed = health
        .apply_docker_event(
            &offerings,
            &id,
            "nginx",
            &ServiceHealthStatus::Offline,
            OfferingStatus::Running,
            ServiceHealthStatus::Healthy,
        )
        .await;

    assert!(changed);

    let event = rx.try_recv().expect("should have received event");
    assert_eq!(event.kind, HealthChangeKind::Recovered);
}

#[tokio::test]
async fn apply_docker_event_no_change() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let changed = health
        .apply_docker_event(
            &offerings,
            &id,
            "redis",
            &ServiceHealthStatus::Healthy,
            OfferingStatus::Running,
            ServiceHealthStatus::Healthy,
        )
        .await;

    assert!(!changed);
}

#[tokio::test]
async fn apply_docker_event_die() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let id = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let mut rx = health.changes();

    let changed = health
        .apply_docker_event(
            &offerings,
            &id,
            "redis",
            &ServiceHealthStatus::Healthy,
            OfferingStatus::Stopped,
            ServiceHealthStatus::Offline,
        )
        .await;

    assert!(changed);

    let event = rx.try_recv().expect("should have received event");
    assert_eq!(event.kind, HealthChangeKind::Failed);
}

// ============================================================================
// Notification tests
// ============================================================================

#[tokio::test]
async fn notification_set_when_degraded() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let _ = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Degraded,
    )
    .await;

    let notifications = garden_common::notifications::NotificationRegistry::new();
    health.update_notification(&offerings, &notifications).await;

    // Should have the degraded tag set
    let tags = notifications.compile();
    assert!(!tags.is_empty(), "expected degraded notification tag");
}

#[tokio::test]
async fn notification_cleared_when_all_healthy() {
    let health = test_health(FakeHealthProbe::healthy()).await;
    let offerings = test_offerings().await;
    let _ = seed_offering(
        &offerings,
        "redis",
        OfferingStatus::Running,
        ServiceHealthStatus::Healthy,
    )
    .await;

    let notifications = garden_common::notifications::NotificationRegistry::new();
    // Set the degraded tag first
    notifications.set(
        garden_common::notifications::NOTIF_SOURCE_OFFERINGS_DEGRADED,
        garden_common::notifications::NotificationTag::Attention,
    );
    assert!(!notifications.compile().is_empty());

    // Run notification update — should clear
    health.update_notification(&offerings, &notifications).await;

    let tags = notifications.compile();
    assert!(
        tags.is_empty(),
        "expected no notification tags after clearing"
    );
}

// ============================================================================
// System health tests (moved from domain/health.rs)
// ============================================================================

#[test]
fn determine_overall_status_all_healthy() {
    use garden_common::ComponentHealth;
    use std::collections::HashMap;

    let mut components = HashMap::new();
    components.insert("disk".to_string(), ComponentHealth::healthy(HashMap::new()));
    components.insert(
        "memory".to_string(),
        ComponentHealth::healthy(HashMap::new()),
    );

    let status = super::system::determine_overall_status(&components);
    assert_eq!(status, garden_common::constants::HEALTH_HEALTHY);
}

#[test]
fn determine_overall_status_one_degraded() {
    use garden_common::ComponentHealth;
    use std::collections::HashMap;

    let mut components = HashMap::new();
    components.insert("disk".to_string(), ComponentHealth::healthy(HashMap::new()));
    components.insert(
        "memory".to_string(),
        ComponentHealth::degraded(HashMap::new()),
    );

    let status = super::system::determine_overall_status(&components);
    assert_eq!(status, garden_common::constants::HEALTH_DEGRADED);
}

#[test]
fn determine_overall_status_one_unhealthy() {
    use garden_common::ComponentHealth;
    use std::collections::HashMap;

    let mut components = HashMap::new();
    components.insert(
        "disk".to_string(),
        ComponentHealth::degraded(HashMap::new()),
    );
    components.insert(
        "memory".to_string(),
        ComponentHealth::unhealthy(HashMap::new()),
    );

    let status = super::system::determine_overall_status(&components);
    assert_eq!(status, garden_common::constants::HEALTH_UNHEALTHY);
}
