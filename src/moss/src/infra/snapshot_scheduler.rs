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

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use garden_common::offerings::OfferingFqn;
use tokio::task::JoinHandle;

use crate::Moss;
use crate::domain::offering_events::{EventActor, EventKind, EventLog};
use crate::domain::snapshot::LocalSnapshotStore;

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
        let mut interval = tokio::time::interval(tick);
        // First tick fires immediately; we want to start the
        // first sweep one full interval after launch so the
        // scheduler doesn't compete with bootstrap I/O.
        interval.tick().await;
        loop {
            interval.tick().await;
            run_periodic_sweep(&state, cadence).await;
        }
    })
}

/// One full sweep of the active Managed offering pool. Exposed
/// publicly so a sysadmin endpoint or a test can trigger an
/// immediate evaluation without waiting for the next tick.
pub async fn run_periodic_sweep(state: &Moss, cadence: Duration) {
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
        if let Err(e) = maybe_capture(state, &offering, now, cadence_chrono).await {
            tracing::warn!(
                offering = %offering.name,
                error = %e,
                "snapshot scheduler: maybe_capture failed"
            );
        }
    }
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
    crate::infra::snapshot::capture_snapshot(state, &fqn, &store, &log, actor)
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
