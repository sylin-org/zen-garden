//! Unit tests for the `Jobs` aggregate.
//!
//! Ch3 of ARCH-0021 (Book IV of ARCH-0017). Tests exercise the full
//! command + query + event surface against a real `Metrics` instance
//! and a fresh `EventBus`. No mocks — the aggregate's behaviour is
//! identical in prod and under test.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::RwLock;

use super::aggregate::Jobs;
use super::event::{ChangeKind, EvictionReason, JobsChanged};
use super::maintenance::DEFAULT_TERMINAL_TTL;
use super::state::JobsState;
use super::{Job, JobStatus};

use crate::domain::Metrics;
use crate::infra::EventBus;

// ─── Harness ───────────────────────────────────────────────────────────

async fn fresh() -> Jobs {
    let metrics = Arc::new(Metrics::new());
    let event_bus = EventBus::new();
    let state: Arc<RwLock<JobsState>> = Arc::new(RwLock::new(HashMap::new()));
    Jobs::with_shared_state(state, metrics, event_bus).await
}

async fn fresh_with_metrics() -> (Jobs, Arc<Metrics>) {
    let metrics = Arc::new(Metrics::new());
    let event_bus = EventBus::new();
    let state: Arc<RwLock<JobsState>> = Arc::new(RwLock::new(HashMap::new()));
    let jobs = Jobs::with_shared_state(state, metrics.clone(), event_bus).await;
    (jobs, metrics)
}

// ─── submit ────────────────────────────────────────────────────────────

#[tokio::test]
async fn submit_inserts_pending_job() {
    let jobs = fresh().await;
    let job = jobs
        .submit(
            "job-1".to_string(),
            "install",
            vec!["nginx".to_string(), "redis".to_string()],
        )
        .await;

    assert_eq!(job.id, "job-1");
    assert!(matches!(job.status, JobStatus::Pending));
    assert_eq!(
        job.targets,
        vec!["nginx".to_string(), "redis".to_string()]
    );
    assert!(job.completed.is_empty());
    assert!(job.failed.is_empty());
    assert!(job.completed_at.is_none());

    let stored = jobs.get("job-1").await.expect("job persisted");
    assert_eq!(stored.id, "job-1");
}

#[tokio::test]
async fn submit_emits_changes_event() {
    let jobs = fresh().await;
    let mut rx = jobs.changes();

    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;

    match rx.recv().await.expect("event received") {
        JobsChanged::Submitted {
            id,
            operation,
            target_count,
        } => {
            assert_eq!(id, "job-1");
            assert_eq!(operation, "install");
            assert_eq!(target_count, 1);
        }
        other => panic!("expected Submitted, got {:?}", other),
    }
}

// ─── start ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_transitions_pending_to_running() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;

    jobs.start("job-1", "install", "nginx").await;

    let job = jobs.get("job-1").await.unwrap();
    assert!(matches!(job.status, JobStatus::Running));
}

#[tokio::test]
async fn start_on_unknown_id_is_noop() {
    let jobs = fresh().await;
    // No panic, no insert.
    jobs.start("does-not-exist", "install", "nginx").await;
    assert!(jobs.get("does-not-exist").await.is_none());
}

// ─── record_item_* ─────────────────────────────────────────────────────

#[tokio::test]
async fn record_item_completed_appends() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;
    jobs.start("job-1", "install", "nginx").await;
    jobs.record_item_completed("job-1", "nginx".to_string())
        .await;

    let job = jobs.get("job-1").await.unwrap();
    assert_eq!(job.completed, vec!["nginx".to_string()]);
}

#[tokio::test]
async fn record_item_failed_inserts() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["redis".to_string()])
        .await;
    jobs.record_item_failed("job-1", "redis".to_string(), "boom".to_string())
        .await;

    let job = jobs.get("job-1").await.unwrap();
    assert_eq!(job.failed.get("redis").map(String::as_str), Some("boom"));
}

// ─── complete / fail ───────────────────────────────────────────────────

#[tokio::test]
async fn complete_sets_terminal_state_and_completed_at() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;
    jobs.start("job-1", "install", "nginx").await;
    jobs.complete("job-1", "nginx").await;

    let job = jobs.get("job-1").await.unwrap();
    assert!(matches!(job.status, JobStatus::Completed));
    assert!(job.completed_at.is_some());
}

#[tokio::test]
async fn fail_sets_terminal_state_and_records_last_error() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;
    jobs.start("job-1", "install", "nginx").await;
    jobs.fail(
        "job-1",
        "nginx",
        Some(("nginx".to_string(), "image pull failed".to_string())),
    )
    .await;

    let job = jobs.get("job-1").await.unwrap();
    assert!(matches!(job.status, JobStatus::Failed));
    assert!(job.completed_at.is_some());
    assert_eq!(
        job.failed.get("nginx").map(String::as_str),
        Some("image pull failed")
    );
}

#[tokio::test]
async fn fail_without_last_error_still_finalizes() {
    let jobs = fresh().await;
    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;
    jobs.fail("job-1", "nginx", None).await;

    let job = jobs.get("job-1").await.unwrap();
    assert!(matches!(job.status, JobStatus::Failed));
    assert!(job.completed_at.is_some());
    assert!(job.failed.is_empty());
}

// ─── maintain ──────────────────────────────────────────────────────────

#[tokio::test]
async fn maintain_evicts_terminal_jobs_past_ttl() {
    let jobs = fresh().await;
    jobs.submit("old".to_string(), "install", vec![]).await;
    jobs.complete("old", "nginx").await;

    // Rewrite completed_at to something ancient by reaching through the
    // strangler `pub(crate) state` field — this is the one place the
    // test harness pokes at internal state.
    {
        let mut guard = jobs.state.write().await;
        let job = guard.get_mut("old").unwrap();
        job.completed_at = Some(SystemTime::now() - Duration::from_secs(48 * 60 * 60));
    }

    let report = jobs
        .maintain_with(SystemTime::now(), DEFAULT_TERMINAL_TTL)
        .await;
    assert_eq!(report.evicted, 1);
    assert_eq!(report.kept, 0);
    assert!(jobs.get("old").await.is_none());
}

#[tokio::test]
async fn maintain_keeps_active_jobs_regardless_of_age() {
    let jobs = fresh().await;
    jobs.submit("still-running".to_string(), "install", vec![])
        .await;
    jobs.start("still-running", "install", "nginx").await;

    // Even if we backdate started_at, active jobs are never evicted.
    {
        let mut guard = jobs.state.write().await;
        let job = guard.get_mut("still-running").unwrap();
        job.started_at = SystemTime::now() - Duration::from_secs(7 * 24 * 60 * 60);
    }

    let report = jobs
        .maintain_with(SystemTime::now(), DEFAULT_TERMINAL_TTL)
        .await;
    assert_eq!(report.evicted, 0);
    assert_eq!(report.kept, 1);
    assert!(jobs.get("still-running").await.is_some());
}

#[tokio::test]
async fn maintain_keeps_fresh_terminal_jobs() {
    let jobs = fresh().await;
    jobs.submit("fresh".to_string(), "install", vec![]).await;
    jobs.complete("fresh", "nginx").await;

    // Default TTL; completed_at is ~now, so the job is well within TTL.
    let report = jobs.maintain().await;
    assert_eq!(report.evicted, 0);
    assert_eq!(report.kept, 1);
}

#[tokio::test]
async fn maintain_empty_sweep_is_silent() {
    let jobs = fresh().await;
    let mut rx = jobs.changes();

    let report = jobs.maintain().await;
    assert!(report.is_empty());
    // No JobsChanged event fires on an empty sweep.
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn maintain_emits_evicted_event() {
    let jobs = fresh().await;
    jobs.submit("old".to_string(), "install", vec![]).await;
    jobs.complete("old", "nginx").await;
    {
        let mut guard = jobs.state.write().await;
        guard.get_mut("old").unwrap().completed_at =
            Some(SystemTime::now() - Duration::from_secs(48 * 60 * 60));
    }

    // Drain prior Submitted + Completed events.
    let mut rx = jobs.changes();
    let _ = jobs.maintain().await;

    let event = rx.recv().await.expect("event received");
    match event {
        JobsChanged::Evicted { id, reason } => {
            assert_eq!(id, "old");
            assert_eq!(reason, EvictionReason::TtlExpired);
        }
        other => panic!("expected Evicted, got {:?}", other),
    }
}

// ─── queries ───────────────────────────────────────────────────────────

#[tokio::test]
async fn snapshot_returns_all_jobs() {
    let jobs = fresh().await;
    jobs.submit("a".to_string(), "install", vec![]).await;
    jobs.submit("b".to_string(), "install", vec![]).await;
    jobs.submit("c".to_string(), "install", vec![]).await;

    let snap = jobs.snapshot().await;
    assert_eq!(snap.len(), 3);
}

#[tokio::test]
async fn list_active_filters_terminal_jobs() {
    let jobs = fresh().await;
    jobs.submit("running".to_string(), "install", vec![]).await;
    jobs.start("running", "install", "nginx").await;

    jobs.submit("done".to_string(), "install", vec![]).await;
    jobs.complete("done", "nginx").await;

    jobs.submit("failed".to_string(), "install", vec![]).await;
    jobs.fail("failed", "nginx", None).await;

    let active = jobs.list_active().await;
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "running");
    assert_eq!(jobs.active_count().await, 1);
}

#[tokio::test]
async fn find_active_by_prefix_returns_active_match() {
    let jobs = fresh().await;
    jobs.submit(
        "add-capability-ollama-llama2-uuid1".to_string(),
        "add-capability",
        vec![],
    )
    .await;
    jobs.start(
        "add-capability-ollama-llama2-uuid1",
        "add-capability",
        "ollama",
    )
    .await;

    let found = jobs
        .find_active_by_prefix("add-capability-ollama-llama2")
        .await
        .expect("match");
    assert_eq!(found.id, "add-capability-ollama-llama2-uuid1");
}

#[tokio::test]
async fn find_active_by_prefix_skips_terminal_matches() {
    let jobs = fresh().await;
    jobs.submit(
        "refresh-capabilities-ollama-uuid1".to_string(),
        "refresh-capabilities",
        vec![],
    )
    .await;
    jobs.complete("refresh-capabilities-ollama-uuid1", "ollama")
        .await;

    let found = jobs
        .find_active_by_prefix("refresh-capabilities-ollama")
        .await;
    assert!(found.is_none(), "terminal jobs are not active matches");
}

// ─── events + metrics integration ──────────────────────────────────────

#[tokio::test]
async fn metrics_records_domain_event_on_submit() {
    let (jobs, metrics) = fresh_with_metrics().await;
    jobs.submit("job-1".to_string(), "install", vec![]).await;

    let snap = metrics.domain(Jobs::NAME).await.expect("domain registered");
    assert_eq!(snap.events_total, 1);
    assert_eq!(snap.events_by_kind.get("submitted").copied(), Some(1));
}

#[tokio::test]
async fn metrics_records_latency_on_mutation() {
    let (jobs, metrics) = fresh_with_metrics().await;
    jobs.submit("job-1".to_string(), "install", vec![]).await;

    let snap = metrics.domain(Jobs::NAME).await.expect("domain registered");
    // Any mutation registers at least one latency sample.
    assert!(snap.mutation_latency.count >= 1);
}

#[tokio::test]
async fn full_lifecycle_event_sequence() {
    let jobs = fresh().await;
    let mut rx = jobs.changes();

    jobs.submit("job-1".to_string(), "install", vec!["nginx".to_string()])
        .await;
    jobs.start("job-1", "install", "nginx").await;
    jobs.record_item_completed("job-1", "nginx".to_string())
        .await;
    jobs.complete("job-1", "nginx").await;

    let kinds: Vec<ChangeKind> = (0..4)
        .map(|_| rx.try_recv().expect("event in channel").kind())
        .collect();

    assert_eq!(
        kinds,
        vec![
            ChangeKind::Submitted,
            ChangeKind::Started,
            ChangeKind::ItemCompleted,
            ChangeKind::Completed,
        ]
    );
}

// ─── strangler: legacy raw-map path still works ────────────────────────

#[tokio::test]
async fn legacy_raw_map_mutations_visible_through_aggregate_queries() {
    // Simulates the Ch3 strangler state: a legacy raw-map caller
    // inserts into the shared `Arc<RwLock<HashMap>>` directly, and
    // the aggregate query methods see the insertion. Ch4/Ch5 migrate
    // these sites to typed commands; until then, observability through
    // the aggregate remains honest about what is in the map.
    let metrics = Arc::new(Metrics::new());
    let event_bus = EventBus::new();
    let shared: Arc<RwLock<JobsState>> = Arc::new(RwLock::new(HashMap::new()));
    let jobs = Jobs::with_shared_state(shared.clone(), metrics, event_bus).await;

    // Legacy write path — bypasses the aggregate entirely.
    shared.write().await.insert(
        "legacy".to_string(),
        Job {
            id: "legacy".to_string(),
            targets: vec![],
            status: JobStatus::Running,
            completed: vec![],
            failed: HashMap::new(),
            started_at: SystemTime::now(),
            completed_at: None,
        },
    );

    // Typed query sees it.
    let via_aggregate = jobs.get("legacy").await.expect("legacy entry visible");
    assert_eq!(via_aggregate.id, "legacy");
    assert_eq!(jobs.active_count().await, 1);
}
