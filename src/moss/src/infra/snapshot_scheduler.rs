//! Periodic snapshot scheduler.
//!
//! ORCH-0039 §"Snapshot frequency" calls for both user-initiated
//! and periodic captures. The HTTP endpoint covers user-initiated;
//! this module is the periodic side. A single supervised tokio
//! task wakes up at [`PERIODIC_TICK_INTERVAL`], asks the offerings
//! aggregate for the active Managed pool, and captures any
//! offering whose latest [`EventKind::BackupTaken`] event is
//! older than [`PERIODIC_DEFAULT`] (or whose log has no
//! `BackupTaken` event at all).
//!
//! Intentionally simple for M2:
//!
//! - **Local-disk only.** Periodic snapshots don't pick a bank.
//!   Bank routing is for explicit user gestures; the scheduler's
//!   job is "make sure something fresh exists locally" so the
//!   plant flow can read from disk without a network round-trip.
//! - **No per-FQN cadence override.** The default is one number,
//!   the same for every offering. Per-offering manifest fields
//!   land in M3 alongside storage-touch events.
//! - **Sequential captures.** Each tick processes offerings one
//!   at a time. Captures are I/O-heavy (image save, volume
//!   archives) and running them in parallel for many offerings
//!   would saturate disk; serialising is the conservative call.
//! - **Skipped when Docker is not ready.** The capture would fail
//!   anyway, so the scheduler short-circuits and waits for the
//!   next tick.
//!
//! The scheduler does **not** retry failed captures within the
//! same tick — a transient failure logs and moves on, the next
//! tick re-evaluates. This avoids a poison-offering scenario
//! where one perpetually-failing capture blocks the rest of the
//! pool.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use garden_common::offerings::OfferingFqn;
use tokio::task::JoinHandle;

use crate::Moss;
use crate::domain::offering_events::{EventActor, EventKind, EventLog};
use crate::domain::snapshot::LocalSnapshotStore;
use crate::infra::snapshot::{RETENTION_KEEP, reconcile_all_snapshots};

/// How often the scheduler wakes up to evaluate the offering
/// pool. 30 minutes is a reasonable balance between freshness
/// and load — most offerings don't need finer-grained coverage,
/// and the offerings whose last_backup is barely-stale won't
/// re-snapshot until the next tick anyway.
pub const PERIODIC_TICK_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Target maximum age for the latest snapshot of any active
/// Managed offering. The scheduler captures whenever the latest
/// `BackupTaken` event is older than this, *or* when no
/// `BackupTaken` event exists at all.
pub const PERIODIC_DEFAULT: Duration = Duration::from_secs(4 * 60 * 60);

/// Base failure-backoff delay (one tick). After a capture fails, the
/// offering is skipped at least this long before the next attempt,
/// doubling per consecutive failure up to [`BACKOFF_MAX`]. Without it a
/// perpetually-failing capture retries every tick, burning a container
/// commit + image save each time (the load half of the runaway that
/// transactional cleanup fixed the disk half of).
const BACKOFF_BASE: Duration = PERIODIC_TICK_INTERVAL;

/// Cap on the failure-backoff delay — a persistently-failing offering is
/// retried at most once per day.
const BACKOFF_MAX: Duration = Duration::from_secs(24 * 60 * 60);

/// Bounded wait for Docker readiness before the startup reconcile. The
/// reconcile is filesystem-only and safe regardless, so on timeout we
/// run it anyway rather than block the scheduler indefinitely.
const STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(120);

/// Spawn the periodic snapshot scheduler. Returns the
/// `JoinHandle` so the caller (typically `bootstrap::run`) can
/// abort it on shutdown. The scheduler runs forever otherwise.
pub fn spawn_periodic_snapshot_scheduler(state: Moss) -> JoinHandle<()> {
    spawn_scheduler_with(state, PERIODIC_TICK_INTERVAL, PERIODIC_DEFAULT)
}

/// Test-friendly variant exposing the tick interval and cadence.
/// The default `spawn_periodic_snapshot_scheduler` calls this
/// with the production constants.
pub fn spawn_scheduler_with(
    state: Moss,
    tick: Duration,
    cadence: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Startup self-heal: reconcile the local snapshot store once the
        // offering subsystem's dependency (Docker) is up, before the
        // periodic loop begins capturing. Corrects debris a prior run
        // left behind — orphaned captures and over-retention.
        startup_reconcile(&state).await;

        let mut interval = tokio::time::interval(tick);
        // First tick fires immediately; we want to start the
        // first sweep one full interval after launch so the
        // scheduler doesn't compete with bootstrap I/O.
        interval.tick().await;

        // Per-offering failure backoff, owned by the loop so it persists
        // across ticks. In-memory only: a restart clears it, which is
        // fine — the startup reconcile and first sweep re-attempt anyway.
        let mut backoff: HashMap<String, FailureBackoff> = HashMap::new();
        loop {
            interval.tick().await;
            run_periodic_sweep_with_backoff(&state, cadence, &mut backoff).await;
        }
    })
}

/// One full sweep of the active Managed offering pool. Exposed
/// publicly so a sysadmin endpoint or a test can trigger an
/// immediate evaluation without waiting for the next tick. Manual
/// triggers ignore failure backoff — they always attempt.
pub async fn run_periodic_sweep(state: &Moss, cadence: Duration) {
    let mut no_backoff = HashMap::new();
    run_periodic_sweep_with_backoff(state, cadence, &mut no_backoff).await;
}

/// Sweep that consults and updates per-offering [`FailureBackoff`]. The
/// periodic loop uses this so a persistently-failing capture backs off
/// instead of retrying every tick. A success clears the offering's entry.
async fn run_periodic_sweep_with_backoff(
    state: &Moss,
    cadence: Duration,
    backoff: &mut HashMap<String, FailureBackoff>,
) {
    if !state.subsystems.is_ready("docker") {
        tracing::debug!("snapshot scheduler: docker not ready, skipping tick");
        return;
    }
    let cadence_chrono = match chrono::Duration::from_std(cadence) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(
                error = %e,
                "snapshot scheduler: cadence overflow, scheduler is misconfigured"
            );
            return;
        }
    };
    let now = Utc::now();
    let offerings = state.offerings.snapshot().await;
    for offering in offerings {
        if offering.managed_data().is_none() {
            continue; // Adopted / Borrowed offerings aren't snapshottable
        }
        let fqn_key = offering.name.fqn();

        // Skip offerings still inside their failure-backoff window.
        if let Some(entry) = backoff.get(&fqn_key)
            && now < entry.not_before
        {
            tracing::debug!(
                offering = %fqn_key,
                retry_at = %entry.not_before,
                "snapshot scheduler: offering in failure backoff, skipping"
            );
            continue;
        }

        match maybe_capture(state, &offering, now, cadence_chrono).await {
            Ok(()) => {
                // Healthy (captured or not yet due) — clear any prior backoff.
                backoff.remove(&fqn_key);
            }
            Err(e) => {
                let entry = backoff.entry(fqn_key.clone()).or_insert(FailureBackoff {
                    consecutive: 0,
                    not_before: now,
                });
                entry.consecutive += 1;
                entry.not_before = now + backoff_delay(entry.consecutive);
                tracing::warn!(
                    offering = %fqn_key,
                    error = %e,
                    consecutive = entry.consecutive,
                    retry_at = %entry.not_before,
                    "snapshot scheduler: maybe_capture failed; backing off"
                );
            }
        }
    }
}

/// Per-offering failure backoff, held in memory by the scheduler loop. A
/// capture that keeps failing is skipped until `not_before`, the delay
/// doubling per consecutive failure. A success clears the entry.
#[derive(Debug, Clone)]
struct FailureBackoff {
    consecutive: u32,
    not_before: DateTime<Utc>,
}

/// Backoff delay after `consecutive` failures: `BACKOFF_BASE * 2^(n-1)`,
/// capped at [`BACKOFF_MAX`]. Pure — unit tested.
fn backoff_delay(consecutive: u32) -> chrono::Duration {
    // Cap the shift so `1 << shift` can't overflow; saturating_mul +
    // the BACKOFF_MAX clamp bound the result regardless.
    let shift = consecutive.saturating_sub(1).min(20);
    let secs = BACKOFF_BASE
        .as_secs()
        .saturating_mul(1u64 << shift)
        .min(BACKOFF_MAX.as_secs());
    chrono::Duration::seconds(secs as i64)
}

/// Reconcile the local snapshot store on startup: reap orphaned captures
/// and enforce retention across every offering. Waits for Docker
/// readiness (the offering subsystem's dependency) so it runs as a
/// defined step in offering bring-up, then proceeds regardless — bounded
/// by [`STARTUP_READY_TIMEOUT`] so a slow or absent Docker can't block
/// it, since the reconcile is filesystem-only and safe to run anytime.
async fn startup_reconcile(state: &Moss) {
    match tokio::time::timeout(STARTUP_READY_TIMEOUT, state.subsystems.wait_ready("docker")).await
    {
        Ok(true) => {}
        Ok(false) => tracing::warn!(
            "snapshot scheduler: docker subsystem unavailable; running startup reconcile anyway"
        ),
        Err(_) => tracing::warn!(
            timeout_secs = STARTUP_READY_TIMEOUT.as_secs(),
            "snapshot scheduler: timed out awaiting docker readiness; running startup reconcile anyway"
        ),
    }

    let root = local_snapshots_root();
    match reconcile_all_snapshots(&root, RETENTION_KEEP).await {
        Ok(report) => tracing::info!(
            offerings = report.offerings_seen,
            orphans_reaped = report.orphans_reaped,
            snapshots_pruned = report.snapshots_pruned,
            "snapshot scheduler: startup reconcile complete"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            root = %root.display(),
            "snapshot scheduler: startup reconcile failed"
        ),
    }
}

/// Root of the local snapshot store — the parent of every offering's
/// [`local_snapshot_root`].
fn local_snapshots_root() -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir()).join("snapshots")
}

/// Decide whether to capture and do it if so. Failure here
/// is logged at the call site; this function bubbles errors
/// up so the caller can attribute them.
async fn maybe_capture(
    state: &Moss,
    offering: &garden_common::Offering,
    now: DateTime<Utc>,
    cadence: chrono::Duration,
) -> anyhow::Result<()> {
    let fqn = offering.name.clone();
    let log = open_log_for(&fqn);
    let last_backup_at = latest_backup_taken_at(&log).await?;
    if !should_snapshot(now, last_backup_at, cadence) {
        return Ok(());
    }

    tracing::info!(
        offering = %fqn.fqn(),
        last_backup_at = ?last_backup_at,
        "snapshot scheduler: capturing periodic snapshot"
    );

    let store = LocalSnapshotStore::new(local_snapshot_root(&fqn));
    let actor = EventActor::system(state.current.stone.name.clone());
    crate::infra::snapshot::capture_snapshot(state, &fqn, &store, &log, actor, None)
        .await
        .map(|_| ())
}

/// Pure decision: should we capture *now* given the last backup
/// time? True when no backup has ever been taken or the latest
/// is older than `cadence`.
pub fn should_snapshot(
    now: DateTime<Utc>,
    last_backup_at: Option<DateTime<Utc>>,
    cadence: chrono::Duration,
) -> bool {
    match last_backup_at {
        None => true,
        Some(at) => now - at >= cadence,
    }
}

/// Read the `at` of the most recent `BackupTaken` event in the
/// log. The log holds events of every kind; we walk backward
/// (the log is small after truncate-since-snapshot retention)
/// looking for the first `BackupTaken`.
async fn latest_backup_taken_at(log: &EventLog) -> anyhow::Result<Option<DateTime<Utc>>> {
    let events = log.read_all().await?;
    Ok(events
        .into_iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::BackupTaken))
        .map(|e| e.at))
}

fn open_log_for(fqn: &OfferingFqn) -> EventLog {
    EventLog::open(
        PathBuf::from(garden_common::constants::paths::data_dir())
            .join("offerings")
            .join(fqn.encoded_for_container())
            .join("events.log"),
    )
}

fn local_snapshot_root(fqn: &OfferingFqn) -> PathBuf {
    PathBuf::from(garden_common::constants::paths::data_dir())
        .join("snapshots")
        .join(fqn.encoded_for_container())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn should_snapshot_when_no_prior_backup() {
        // Cold start — no BackupTaken event exists yet. Always
        // snapshot.
        assert!(should_snapshot(t(0), None, chrono::Duration::hours(4)));
    }

    #[test]
    fn should_snapshot_when_latest_backup_is_older_than_cadence() {
        let cadence = chrono::Duration::hours(4);
        // Last backup 5 hours ago — past cadence, snapshot.
        let last = t(0);
        let now = t(5 * 3600);
        assert!(should_snapshot(now, Some(last), cadence));
    }

    #[test]
    fn should_skip_when_latest_backup_is_recent() {
        let cadence = chrono::Duration::hours(4);
        // Last backup 30 minutes ago — well within cadence.
        let last = t(0);
        let now = t(30 * 60);
        assert!(!should_snapshot(now, Some(last), cadence));
    }

    #[test]
    fn should_snapshot_at_exact_cadence_boundary() {
        // Boundary case: now - last == cadence. The condition
        // is `now - at >= cadence` so this triggers a snapshot.
        // Behaviour matters for predictability under exact
        // tick alignment.
        let cadence = chrono::Duration::hours(4);
        let last = t(0);
        let now = t(4 * 3600);
        assert!(should_snapshot(now, Some(last), cadence));
    }

    #[test]
    fn backoff_delay_doubles_per_consecutive_failure() {
        let base = BACKOFF_BASE.as_secs() as i64;
        // First failure → one base interval; then doubling.
        assert_eq!(backoff_delay(1).num_seconds(), base);
        assert_eq!(backoff_delay(2).num_seconds(), base * 2);
        assert_eq!(backoff_delay(3).num_seconds(), base * 4);
        assert_eq!(backoff_delay(4).num_seconds(), base * 8);
    }

    #[test]
    fn backoff_delay_is_capped_at_max() {
        let max = BACKOFF_MAX.as_secs() as i64;
        // A high failure count saturates at the cap, never overflows.
        assert_eq!(backoff_delay(100).num_seconds(), max);
        assert_eq!(backoff_delay(u32::MAX).num_seconds(), max);
    }

    #[test]
    fn backoff_delay_handles_degenerate_zero() {
        // Defensive: callers always pass ≥1, but 0 must not panic.
        assert_eq!(
            backoff_delay(0).num_seconds(),
            BACKOFF_BASE.as_secs() as i64
        );
    }

    #[tokio::test]
    async fn latest_backup_taken_at_returns_none_for_empty_log() {
        let dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::open(dir.path().join("events.log"));
        assert!(latest_backup_taken_at(&log).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn latest_backup_taken_at_returns_none_when_log_has_no_backups() {
        use crate::domain::offering_events::{EventActor, new_event};
        let dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::open(dir.path().join("events.log"));
        // Log has lifecycle events but no BackupTaken.
        for kind in [EventKind::SetInitialized, EventKind::Reconfig] {
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                serde_json::Map::new(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
        }
        assert!(latest_backup_taken_at(&log).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn latest_backup_taken_at_returns_most_recent_backup() {
        use crate::domain::offering_events::{EventActor, new_event};
        let dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::open(dir.path().join("events.log"));
        // Sequence: BackupTaken, then Reconfig (newer). The
        // function must return the BackupTaken's `at`, not the
        // Reconfig's — even though Reconfig is newer.
        let backup = new_event(
            &log,
            "mongodb::prd",
            EventKind::BackupTaken,
            EventActor::system("stone-alpha"),
            serde_json::Map::new(),
        )
        .await
        .unwrap();
        log.append(&backup).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let reconfig = new_event(
            &log,
            "mongodb::prd",
            EventKind::Reconfig,
            EventActor::system("stone-alpha"),
            serde_json::Map::new(),
        )
        .await
        .unwrap();
        log.append(&reconfig).await.unwrap();

        let at = latest_backup_taken_at(&log).await.unwrap().unwrap();
        assert_eq!(at, backup.at);
    }

    #[tokio::test]
    async fn latest_backup_taken_at_walks_back_to_most_recent_of_multiple_backups() {
        use crate::domain::offering_events::{EventActor, new_event};
        let dir = tempfile::TempDir::new().unwrap();
        let log = EventLog::open(dir.path().join("events.log"));
        let mut latest_seen = None;
        for _ in 0..3 {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                EventKind::BackupTaken,
                EventActor::system("stone-alpha"),
                serde_json::Map::new(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
            latest_seen = Some(e.at);
        }
        let got = latest_backup_taken_at(&log).await.unwrap().unwrap();
        assert_eq!(got, latest_seen.unwrap());
    }
}
