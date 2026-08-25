//! Per-offering event log — append-only GUIDV7-tagged provenance.
//!
//! Every meaningful state-changing operation on an offering
//! instance produces an [`OfferingEvent`] in a sidecar log file
//! that lives alongside the offering's data. Events form a chain
//! via `prev_event_id`, are time-ordered by their GUIDV7
//! (lexicographic = chronological), and are written one
//! per-line in JSON Lines format.
//!
//! The log answers two questions:
//!
//! 1. **What happened to this offering?** — read the chain for
//!    audit, debugging, or display in Pavilion's seed catalog.
//! 2. **Am I behind?** — each running instance persists its own
//!    `last_event_id` watermark; on startup it queries the set's
//!    canonical log (M2: from a snapshot's metadata; M3: live
//!    stream) and compares.
//!
//! Retention is **truncate-since-snapshot** ([ORCH-0039]):
//! after a snapshot is durably written, the events that produced
//! it are by definition reconstructable, so older events can be
//! pruned. [`EventLog::truncate_before`] handles the prune;
//! callers invoke it after each successful backup.
//!
//! ## File format
//!
//! - One [`OfferingEvent`] per line as canonical JSON, terminated
//!   by `\n`.
//! - Append-only: writes go via `OpenOptions::new().append(true)`
//!   serialised through an internal Mutex so `prev_event_id`
//!   chaining is consistent across concurrent appenders within
//!   the process.
//! - Reads load the whole file and parse every line. Malformed
//!   lines (process crash mid-write) cause [`EventLog::read_all`]
//!   to return an error so the caller can decide policy
//!   (typically: log + ignore the trailing partial line).
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Kind of state-changing operation an event records. M2 ships
/// the lifecycle subset; storage-touch events are explicitly
/// deferred to M3 ([ORCH-0039] §M3 cut).
///
/// [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// First event in a chain — set membership initialised on
    /// this instance. `details` carries the FQN's initial state.
    SetInitialized,
    /// A new instance with the same FQN joined the set.
    MemberJoined,
    /// A member instance was evicted from the set.
    MemberLeft,
    /// A snapshot was captured from this instance (success).
    /// `details.snapshot_id` carries the snapshot identifier.
    BackupTaken,
    /// A snapshot was applied to this instance.
    /// `details.from_snapshot_id` carries the source.
    RestoreApplied,
    /// Configuration change (manifest update, env vars, etc).
    Reconfig,
}

/// Who initiated the event. `user` is `None` for system-driven
/// events (periodic snapshots, orchestrator reconfigs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventActor {
    /// Stone where the event originated.
    pub stone: String,
    /// Optional user identifier — present when a user explicitly
    /// initiated the event (drag gesture, CLI command).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

impl EventActor {
    /// Construct a system-driven actor (no user attribution).
    pub fn system(stone: impl Into<String>) -> Self {
        Self {
            stone: stone.into(),
            user: None,
        }
    }

    /// Construct a user-driven actor.
    pub fn user(stone: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            stone: stone.into(),
            user: Some(user.into()),
        }
    }
}

/// One entry in an offering's append-only event log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferingEvent {
    /// Time-ordered unique identifier (GUIDV7). Lexicographic
    /// comparison = chronological comparison.
    pub event_id: String,
    /// Pointer to the previous event in the chain. `None` only
    /// for the first event after a truncate-since-snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_event_id: Option<String>,
    /// FQN of the offering this event applies to — duplicated
    /// across every entry so a single line is self-describing.
    pub fqn: String,
    /// Wall-clock time the event was recorded.
    pub at: DateTime<Utc>,
    /// What kind of operation produced this event.
    pub kind: EventKind,
    /// Who initiated the operation.
    pub actor: EventActor,
    /// Kind-specific detail payload. Empty by default; carries
    /// `snapshot_id` for `BackupTaken`, `from_snapshot_id` for
    /// `RestoreApplied`, etc.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub details: serde_json::Map<String, serde_json::Value>,
}

/// Append-only log of [`OfferingEvent`]s backed by a single file.
///
/// Writes serialise through an internal Mutex so concurrent
/// callers within the process produce a well-formed chain.
/// Cross-process serialisation is not provided (Moss is the
/// single writer for an offering's events).
pub struct EventLog {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl EventLog {
    /// Open or create an event log at `path`. Does not touch the
    /// filesystem until the first read or write.
    pub fn open(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    /// Path to the underlying file. Useful for diagnostics and
    /// for callers that need to copy or move the log alongside
    /// the offering's other state.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append one event. Caller is responsible for filling
    /// `event_id`, `prev_event_id`, and `at` — typically via
    /// [`new_event`] which derives them from the log's current
    /// latest entry.
    ///
    /// If the parent directory doesn't exist, it is created.
    pub async fn append(&self, event: &OfferingEvent) -> Result<()> {
        let _guard = self.write_lock.lock().await;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create event log parent: {}", parent.display()))?;
        }
        let mut line =
            serde_json::to_string(event).context("serialize OfferingEvent for append")?;
        line.push('\n');

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .with_context(|| format!("open event log for append: {}", self.path.display()))?;
        file.write_all(line.as_bytes())
            .await
            .with_context(|| format!("append to event log: {}", self.path.display()))?;
        file.flush()
            .await
            .with_context(|| format!("flush event log: {}", self.path.display()))?;
        Ok(())
    }

    /// Read every event in chronological order. Returns an empty
    /// vec if the file does not exist (a fresh offering with no
    /// events recorded yet is a valid state).
    pub async fn read_all(&self) -> Result<Vec<OfferingEvent>> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(anyhow::Error::from(e)
                    .context(format!("read event log: {}", self.path.display())));
            }
        };
        parse_lines(&raw, &self.path)
    }

    /// Read events whose `event_id` is strictly greater than
    /// `since`. Used by an instance catching up from its
    /// watermark — the watermark is included by reference but
    /// not in the returned slice.
    pub async fn read_since(&self, since: &str) -> Result<Vec<OfferingEvent>> {
        let all = self.read_all().await?;
        Ok(all
            .into_iter()
            .filter(|e| e.event_id.as_str() > since)
            .collect())
    }

    /// Return the most recent event, or `None` if the log is
    /// empty / missing.
    pub async fn latest(&self) -> Result<Option<OfferingEvent>> {
        let mut all = self.read_all().await?;
        Ok(all.pop())
    }

    /// Truncate every event with `event_id < retain_from`. The
    /// event with `event_id == retain_from` itself is kept; all
    /// strictly-older events are pruned. Returns the count
    /// removed.
    ///
    /// Used by the snapshot-capture flow to enforce
    /// truncate-since-snapshot retention. Callers invoke this
    /// with the `event_id` of the just-recorded `BackupTaken`
    /// event (or earlier — never-truncate-newest is correct).
    ///
    /// The implementation reads the whole log, filters in
    /// memory, then atomically replaces the file via a temp
    /// file + rename so a crash mid-truncate cannot lose
    /// history.
    pub async fn truncate_before(&self, retain_from: &str) -> Result<usize> {
        let _guard = self.write_lock.lock().await;
        let all = self.read_all().await?;
        let before = all.len();
        let kept: Vec<OfferingEvent> = all
            .into_iter()
            .filter(|e| e.event_id.as_str() >= retain_from)
            .collect();
        let removed = before - kept.len();
        if removed == 0 {
            return Ok(0);
        }

        // Drop prev_event_id on the new head — the events it
        // pointed to are gone. The chain semantics survive: the
        // first remaining event has prev_event_id = None,
        // matching what `set_initialized` looks like at the head
        // of a fresh log.
        let mut head_orphaned = kept;
        if let Some(first) = head_orphaned.first_mut() {
            first.prev_event_id = None;
        }

        // Atomic replace via temp-file + rename in the same
        // directory.
        let tmp_path = match self.path.parent() {
            Some(parent) => parent.join(format!(
                ".{}.truncate-tmp",
                self.path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("events.log")
            )),
            None => self.path.with_extension("truncate-tmp"),
        };

        let mut buf = String::new();
        for event in &head_orphaned {
            buf.push_str(&serde_json::to_string(event)?);
            buf.push('\n');
        }
        tokio::fs::write(&tmp_path, buf.as_bytes())
            .await
            .with_context(|| format!("write truncate temp file: {}", tmp_path.display()))?;
        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .with_context(|| format!("rename truncate temp file: {}", tmp_path.display()))?;

        Ok(removed)
    }
}

/// Build a new event chained off the log's latest entry. The
/// caller fills `kind`, `actor`, `details`, and `fqn`; the helper
/// fills `event_id`, `prev_event_id`, and `at`.
///
/// If the log is empty, `prev_event_id` is `None` — the new
/// event becomes the chain root.
pub async fn new_event(
    log: &EventLog,
    fqn: impl Into<String>,
    kind: EventKind,
    actor: EventActor,
    details: serde_json::Map<String, serde_json::Value>,
) -> Result<OfferingEvent> {
    let prev = log.latest().await?.map(|e| e.event_id);
    Ok(OfferingEvent {
        event_id: garden_common::utils::ids::generate_guidv7(),
        prev_event_id: prev,
        fqn: fqn.into(),
        at: Utc::now(),
        kind,
        actor,
        details,
    })
}

fn parse_lines(raw: &str, path: &Path) -> Result<Vec<OfferingEvent>> {
    let mut out = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let event: OfferingEvent = serde_json::from_str(line).with_context(|| {
            format!(
                "parse event log line {} in {}: '{}'",
                idx + 1,
                path.display(),
                line
            )
        })?;
        out.push(event);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn details() -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }

    async fn fresh_log() -> (TempDir, EventLog) {
        let dir = TempDir::new().unwrap();
        let log = EventLog::open(dir.path().join("events.log"));
        (dir, log)
    }

    #[tokio::test]
    async fn read_all_returns_empty_when_log_does_not_exist() {
        let (_dir, log) = fresh_log().await;
        assert!(log.read_all().await.unwrap().is_empty());
        assert!(log.latest().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn append_then_read_round_trips_event() {
        let (_dir, log) = fresh_log().await;
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            details(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();

        let all = log.read_all().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], event);
    }

    #[tokio::test]
    async fn prev_event_id_chains_consecutive_events() {
        let (_dir, log) = fresh_log().await;
        let e1 = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            details(),
        )
        .await
        .unwrap();
        log.append(&e1).await.unwrap();
        let e2 = new_event(
            &log,
            "mongodb::prd",
            EventKind::BackupTaken,
            EventActor::user("stone-alpha", "leo"),
            details(),
        )
        .await
        .unwrap();
        log.append(&e2).await.unwrap();

        // First event has no predecessor; second points back at
        // the first.
        assert!(e1.prev_event_id.is_none());
        assert_eq!(e2.prev_event_id.as_deref(), Some(e1.event_id.as_str()));

        // GUIDV7 ordering matches insertion order.
        assert!(e1.event_id < e2.event_id);
    }

    #[tokio::test]
    async fn read_since_returns_only_strictly_newer_events() {
        let (_dir, log) = fresh_log().await;
        let mut events = Vec::new();
        for kind in [
            EventKind::SetInitialized,
            EventKind::BackupTaken,
            EventKind::Reconfig,
            EventKind::RestoreApplied,
        ] {
            // Chrono::Utc::now resolution + GUIDV7 unique-by-100ns
            // make these strictly increasing within the test's
            // wall clock; sleep a tick to be defensive on fast
            // CI hardware.
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                details(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
            events.push(e);
        }

        // Cursor at the second event — read_since must return
        // events 3 and 4 only (strictly newer).
        let since = events[1].event_id.clone();
        let after = log.read_since(&since).await.unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].event_id, events[2].event_id);
        assert_eq!(after[1].event_id, events[3].event_id);
    }

    #[tokio::test]
    async fn latest_returns_most_recent_event() {
        let (_dir, log) = fresh_log().await;
        for kind in [EventKind::SetInitialized, EventKind::BackupTaken] {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                details(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
        }
        let latest = log.latest().await.unwrap().unwrap();
        assert_eq!(latest.kind, EventKind::BackupTaken);
    }

    #[tokio::test]
    async fn truncate_before_drops_older_events_and_orphans_new_head() {
        let (_dir, log) = fresh_log().await;
        let mut events = Vec::new();
        for kind in [
            EventKind::SetInitialized,
            EventKind::Reconfig,
            EventKind::BackupTaken, // <- the snapshot event we keep from
            EventKind::Reconfig,
        ] {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                details(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
            events.push(e);
        }

        // Truncate to keep events from index 2 (BackupTaken).
        let snapshot_event_id = events[2].event_id.clone();
        let removed = log.truncate_before(&snapshot_event_id).await.unwrap();
        assert_eq!(removed, 2, "two pre-snapshot events must be pruned");

        let remaining = log.read_all().await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].event_id, events[2].event_id);
        // The new head's prev_event_id is wiped — the events it
        // pointed to no longer exist.
        assert!(
            remaining[0].prev_event_id.is_none(),
            "post-truncate head must not point at a pruned event"
        );
        // The post-head entry still chains correctly.
        assert_eq!(
            remaining[1].prev_event_id.as_deref(),
            Some(remaining[0].event_id.as_str())
        );
    }

    #[tokio::test]
    async fn truncate_with_unknown_id_keeps_everything() {
        // If retain_from doesn't match any event in the log, the
        // filter (event_id >= retain_from) decides per-event:
        // some events may still pass. With a synthesised ID that
        // sorts *before* every real GUIDV7, truncate keeps all
        // events and removes none.
        let (_dir, log) = fresh_log().await;
        for kind in [EventKind::SetInitialized, EventKind::BackupTaken] {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                details(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
        }

        // GUIDV7s start with timestamp; the all-zeros ID sorts
        // before any real one.
        let removed = log
            .truncate_before("00000000-0000-7000-8000-000000000000")
            .await
            .unwrap();
        assert_eq!(removed, 0);
        assert_eq!(log.read_all().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn truncate_keeping_zero_events_empties_the_log() {
        // Edge case: retain_from sorts AFTER every event in the
        // log. Result should be a fully-empty log with zero
        // entries removed counted as `before - 0`.
        let (_dir, log) = fresh_log().await;
        let mut events = Vec::new();
        for kind in [EventKind::SetInitialized, EventKind::BackupTaken] {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let e = new_event(
                &log,
                "mongodb::prd",
                kind,
                EventActor::system("stone-alpha"),
                details(),
            )
            .await
            .unwrap();
            log.append(&e).await.unwrap();
            events.push(e);
        }

        // All-Fs sorts after every real GUIDV7.
        let removed = log
            .truncate_before("ffffffff-ffff-7fff-bfff-ffffffffffff")
            .await
            .unwrap();
        assert_eq!(removed, events.len());
        assert!(log.read_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_line_in_log_surfaces_as_parse_error() {
        // A torn last line from a process crash mid-write must
        // not silently disappear — read_all returns an error so
        // the caller can decide policy (typically: log + drop
        // the trailing partial).
        let (dir, log) = fresh_log().await;
        let good = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            details(),
        )
        .await
        .unwrap();
        log.append(&good).await.unwrap();

        // Append a non-JSON line to simulate corruption.
        tokio::fs::write(
            dir.path().join("events.log"),
            format!("{}\n{{not json", serde_json::to_string(&good).unwrap()),
        )
        .await
        .unwrap();

        let err = log.read_all().await.expect_err("malformed line must error");
        let msg = err.to_string();
        assert!(
            msg.contains("parse event log line"),
            "error must point at the bad line: {msg}"
        );
    }

    #[tokio::test]
    async fn details_payload_round_trips_arbitrary_json() {
        let (_dir, log) = fresh_log().await;
        let mut details = serde_json::Map::new();
        details.insert(
            "snapshot_id".into(),
            serde_json::Value::String("snap-abc".into()),
        );
        details.insert(
            "byte_count".into(),
            serde_json::Value::Number(serde_json::Number::from(1234567u64)),
        );
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::BackupTaken,
            EventActor::user("stone-alpha", "leo"),
            details.clone(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();

        let read = log.read_all().await.unwrap();
        assert_eq!(read[0].details, details);
        assert_eq!(read[0].actor.user.as_deref(), Some("leo"));
    }
}
