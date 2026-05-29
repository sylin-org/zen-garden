//! Snapshot reclaimer (STORAGE-0020).
//!
//! Adapts the existing snapshot retention machinery
//! ([`reconcile_all_snapshots`](crate::infra::snapshot::reconcile_all_snapshots))
//! to the [`Reclaimable`] port. Reaping orphaned (manifest-less) captures
//! happens at every level; the retained-count tightens as pressure rises,
//! but never to zero — a stone under disk pressure still keeps its most
//! recent restore point.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use crate::domain::capacity::budget::ReclaimLevel;
use crate::domain::capacity::reclaimable::{Reclaimable, ReclaimPriority, Reclaimed};
use crate::infra::snapshot::reconcile_all_snapshots;

const NAME: &str = "snapshots";

/// Snapshots to retain per offering at each reclaim level. `Routine`
/// matches the standard keep-5; pressure tightens toward keep-1.
const KEEP_ROUTINE: usize = 5;
const KEEP_PRESSURE: usize = 3;
const KEEP_CRITICAL: usize = 1;

/// Reclaims disk by reaping orphaned captures and pruning old snapshots
/// under the local snapshots root (`<data_dir>/snapshots`).
pub struct SnapshotReclaimer {
    root: PathBuf,
}

impl SnapshotReclaimer {
    /// `root` is the local snapshots root — the parent of the per-offering
    /// directories, i.e. `<data_dir>/snapshots`.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Reclaimable for SnapshotReclaimer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn priority(&self) -> ReclaimPriority {
        // Backups are real data — reclaimed only after pure junk (images).
        ReclaimPriority::Normal
    }

    fn reclaim<'a>(
        &'a self,
        level: ReclaimLevel,
    ) -> Pin<Box<dyn Future<Output = Reclaimed> + Send + 'a>> {
        Box::pin(async move {
            let keep = match level {
                ReclaimLevel::Routine => KEEP_ROUTINE,
                ReclaimLevel::Pressure => KEEP_PRESSURE,
                ReclaimLevel::Critical => KEEP_CRITICAL,
            };

            match reconcile_all_snapshots(&self.root, keep).await {
                Ok(report) => {
                    let items = report.orphans_reaped + report.snapshots_pruned;
                    let mut notes = Vec::new();
                    if items > 0 {
                        notes.push(format!(
                            "reaped {} orphan(s), pruned {} snapshot(s) to keep-{keep} across {} offering(s)",
                            report.orphans_reaped, report.snapshots_pruned, report.offerings_seen
                        ));
                    }
                    Reclaimed {
                        reclaimer: NAME,
                        items_removed: items,
                        notes,
                    }
                }
                Err(e) => Reclaimed {
                    reclaimer: NAME,
                    items_removed: 0,
                    notes: vec![format!("snapshot reconcile failed (non-fatal): {e}")],
                },
            }
        })
    }
}
