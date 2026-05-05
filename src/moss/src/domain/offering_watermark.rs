//! Per-instance event-log watermark.
//!
//! Each running offering instance persists a `Watermark` per FQN
//! it participates in: the highest `event_id` it considers itself
//! up-to-date against. On startup the instance compares its
//! watermark to the canonical log's latest entry; if the canonical
//! is newer, the instance is **behind** and should enter
//! sync mode.
//!
//! In M2 the canonical log *is* the local on-disk event log, so
//! `is_behind` is rarely true in practice — the only path that
//! produces a stale watermark is a crash between event append and
//! watermark advance. The function exists as the hook M3 plugs
//! into when the live event stream arrives from primary peers.
//!
//! Watermarks are stored as per-FQN JSON files at
//! `<root>/<fqn-encoded>.json`. The encoded FQN is the same
//! container-safe form used elsewhere
//! (`OfferingFqn::encoded_for_container`).
//!
//! See [ORCH-0039] §"Per-instance watermark" for the lifecycle.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use garden_common::offerings::OfferingFqn;
use serde::{Deserialize, Serialize};

use super::offering_events::EventLog;

/// One offering instance's view of "where I am" in the event log
/// chain for a given FQN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watermark {
    /// Canonical FQN string (e.g. `mongodb::prd`). Stored
    /// alongside the event_id so a single file is self-describing
    /// without relying on its filename.
    pub fqn: String,
    /// The highest `event_id` this instance considers itself
    /// caught up to. GUIDV7 — lexicographic = chronological.
    pub last_event_id: String,
}

/// Disk-backed store of per-FQN watermarks. One file per FQN
/// under `root`, atomic writes via temp-file + rename.
pub struct WatermarkStore {
    root: PathBuf,
}

impl WatermarkStore {
    /// Open the store rooted at `root`. The directory is created
    /// lazily on first write.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Read the watermark for `fqn`. Returns `Ok(None)` when no
    /// watermark has been recorded yet — a cold-start state for
    /// any instance that hasn't yet produced or seen an event.
    pub async fn read(&self, fqn: &OfferingFqn) -> Result<Option<Watermark>> {
        let path = self.path_for(fqn);
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => {
                let wm: Watermark = serde_json::from_str(&s)
                    .with_context(|| format!("parse watermark file: {}", path.display()))?;
                Ok(Some(wm))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(anyhow::Error::from(e)
                .context(format!("read watermark file: {}", path.display()))),
        }
    }

    /// Atomically write the watermark for `fqn` (parsed from
    /// `watermark.fqn`). Writes via temp-file + rename so a
    /// crash mid-write cannot leave a torn file.
    pub async fn write(&self, watermark: &Watermark) -> Result<()> {
        let fqn = OfferingFqn::parse(&watermark.fqn)
            .map_err(|e| anyhow::anyhow!("invalid fqn '{}' in watermark: {e}", watermark.fqn))?;
        let path = self.path_for(&fqn);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create watermark dir: {}", parent.display()))?;
        }

        let tmp = path.with_extension("tmp");
        let body = serde_json::to_vec_pretty(watermark).context("serialize watermark")?;
        tokio::fs::write(&tmp, &body)
            .await
            .with_context(|| format!("write watermark tmp: {}", tmp.display()))?;
        tokio::fs::rename(&tmp, &path)
            .await
            .with_context(|| format!("rename watermark tmp: {}", tmp.display()))?;
        Ok(())
    }

    /// Filename within `root` for a given FQN. Public so callers
    /// (e.g. tests, diagnostics) can locate the file.
    pub fn path_for(&self, fqn: &OfferingFqn) -> PathBuf {
        self.root
            .join(format!("{}.json", fqn.encoded_for_container()))
    }

    /// Root directory the store writes into.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Advance the watermark for `fqn` to the log's latest event.
/// Returns the new watermark's `last_event_id`, or `None` when
/// the log has no events yet (no advance possible).
///
/// Idempotent: calling repeatedly with no new appends in between
/// produces the same watermark and rewrites the file.
pub async fn advance_watermark(
    store: &WatermarkStore,
    log: &EventLog,
    fqn: &OfferingFqn,
) -> Result<Option<String>> {
    let Some(latest) = log.latest().await? else {
        return Ok(None);
    };
    let watermark = Watermark {
        fqn: fqn.fqn(),
        last_event_id: latest.event_id.clone(),
    };
    store.write(&watermark).await?;
    Ok(Some(latest.event_id))
}

/// Compare the persisted watermark against the local event log.
/// `true` means the log has events strictly newer than the
/// watermark — this instance is behind.
///
/// Cold-start cases (`true`):
/// - Watermark file missing but log has events. The instance
///   appended events without recording its position; treat as
///   behind.
///
/// Cold-start cases (`false`):
/// - Both watermark and log are empty/missing — nothing to do,
///   not behind because there's no canonical state to be
///   behind of.
/// - Watermark missing and log empty — same.
pub async fn is_behind(
    store: &WatermarkStore,
    log: &EventLog,
    fqn: &OfferingFqn,
) -> Result<bool> {
    let latest = log.latest().await?;
    let Some(latest) = latest else {
        return Ok(false);
    };

    let watermark = store.read(fqn).await?;
    let Some(watermark) = watermark else {
        return Ok(true);
    };

    Ok(latest.event_id > watermark.last_event_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::offering_events::{EventActor, EventKind, new_event};
    use tempfile::TempDir;

    fn fqn() -> OfferingFqn {
        OfferingFqn::parse("mongodb::prd").unwrap()
    }

    fn detail() -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }

    async fn fresh_pair() -> (TempDir, WatermarkStore, EventLog) {
        let dir = TempDir::new().unwrap();
        let store = WatermarkStore::new(dir.path().join("watermarks"));
        let log = EventLog::open(dir.path().join("events.log"));
        (dir, store, log)
    }

    #[tokio::test]
    async fn read_missing_watermark_returns_none() {
        let (_dir, store, _log) = fresh_pair().await;
        assert!(store.read(&fqn()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn write_then_read_round_trips_watermark() {
        let (_dir, store, _log) = fresh_pair().await;
        let wm = Watermark {
            fqn: fqn().fqn(),
            last_event_id: "01ABCDEF".into(),
        };
        store.write(&wm).await.unwrap();
        let read = store.read(&fqn()).await.unwrap().unwrap();
        assert_eq!(read, wm);
    }

    #[tokio::test]
    async fn watermark_filename_uses_encoded_fqn() {
        let (_dir, store, _log) = fresh_pair().await;
        let path = store.path_for(&fqn());
        // OfferingFqn::encoded_for_container replaces `::` with `--`.
        assert!(
            path.to_string_lossy().ends_with("mongodb--prd.json"),
            "expected mongodb--prd.json suffix, got {}",
            path.display()
        );
    }

    #[tokio::test]
    async fn advance_watermark_on_empty_log_returns_none() {
        let (_dir, store, log) = fresh_pair().await;
        let result = advance_watermark(&store, &log, &fqn()).await.unwrap();
        assert!(result.is_none());
        // No file must have been written.
        assert!(store.read(&fqn()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn advance_watermark_writes_latest_event_id() {
        let (_dir, store, log) = fresh_pair().await;
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();

        let advanced = advance_watermark(&store, &log, &fqn())
            .await
            .unwrap()
            .expect("advance must return the new id");
        assert_eq!(advanced, event.event_id);

        let stored = store.read(&fqn()).await.unwrap().unwrap();
        assert_eq!(stored.last_event_id, event.event_id);
        assert_eq!(stored.fqn, "mongodb::prd");
    }

    #[tokio::test]
    async fn advance_watermark_is_idempotent_when_no_new_events() {
        let (_dir, store, log) = fresh_pair().await;
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();

        let first = advance_watermark(&store, &log, &fqn())
            .await
            .unwrap()
            .unwrap();
        let second = advance_watermark(&store, &log, &fqn())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn is_behind_returns_false_when_log_and_watermark_are_empty() {
        let (_dir, store, log) = fresh_pair().await;
        assert!(!is_behind(&store, &log, &fqn()).await.unwrap());
    }

    #[tokio::test]
    async fn is_behind_returns_true_when_watermark_missing_but_log_has_events() {
        // Cold-start case: an instance that appended events but
        // never recorded its watermark (crash before advance).
        // Reading those events on next boot must signal "behind".
        let (_dir, store, log) = fresh_pair().await;
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();

        assert!(is_behind(&store, &log, &fqn()).await.unwrap());
    }

    #[tokio::test]
    async fn is_behind_returns_false_when_watermark_matches_latest() {
        let (_dir, store, log) = fresh_pair().await;
        let event = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&event).await.unwrap();
        advance_watermark(&store, &log, &fqn()).await.unwrap();

        assert!(!is_behind(&store, &log, &fqn()).await.unwrap());
    }

    #[tokio::test]
    async fn is_behind_returns_true_after_new_event_without_advance() {
        let (_dir, store, log) = fresh_pair().await;
        // First event + advance — caught up.
        let e1 = new_event(
            &log,
            "mongodb::prd",
            EventKind::SetInitialized,
            EventActor::system("stone-alpha"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&e1).await.unwrap();
        advance_watermark(&store, &log, &fqn()).await.unwrap();
        assert!(!is_behind(&store, &log, &fqn()).await.unwrap());

        // Second event but watermark not advanced — behind.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let e2 = new_event(
            &log,
            "mongodb::prd",
            EventKind::BackupTaken,
            EventActor::user("stone-alpha", "leo"),
            detail(),
        )
        .await
        .unwrap();
        log.append(&e2).await.unwrap();
        assert!(is_behind(&store, &log, &fqn()).await.unwrap());

        // Advance closes the gap.
        advance_watermark(&store, &log, &fqn()).await.unwrap();
        assert!(!is_behind(&store, &log, &fqn()).await.unwrap());
    }

    #[tokio::test]
    async fn watermark_write_is_atomic_via_temp_file() {
        // After a successful write, no `.tmp` artifact should
        // remain in the watermark directory.
        let (dir, store, _log) = fresh_pair().await;
        store
            .write(&Watermark {
                fqn: fqn().fqn(),
                last_event_id: "01XYZ".into(),
            })
            .await
            .unwrap();

        let watermarks_dir = dir.path().join("watermarks");
        let entries: Vec<_> = std::fs::read_dir(&watermarks_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            entries.iter().all(|n| !n.ends_with(".tmp")),
            "no .tmp artifacts must remain after write: {entries:?}"
        );
    }

    #[tokio::test]
    async fn separate_fqns_have_independent_watermarks() {
        let (_dir, store, _log) = fresh_pair().await;
        let fqn_a = OfferingFqn::parse("mongodb::prd").unwrap();
        let fqn_b = OfferingFqn::parse("mongodb::staging").unwrap();

        store
            .write(&Watermark {
                fqn: fqn_a.fqn(),
                last_event_id: "id-a".into(),
            })
            .await
            .unwrap();
        store
            .write(&Watermark {
                fqn: fqn_b.fqn(),
                last_event_id: "id-b".into(),
            })
            .await
            .unwrap();

        assert_eq!(
            store.read(&fqn_a).await.unwrap().unwrap().last_event_id,
            "id-a"
        );
        assert_eq!(
            store.read(&fqn_b).await.unwrap().unwrap().last_event_id,
            "id-b"
        );
    }
}
