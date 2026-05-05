//! Storage connectivity health (STORAGE-0019).
//!
//! The pipeline stage that sits between the storage listener and the
//! candidate classifier. Reads sysfs to decide whether a freshly-detected
//! block device is in a recoverable degraded state, attempts recovery
//! (SCSI rescan / USB re-authorization), and forwards the device with a
//! [`ConnectivityStatus`](garden_common::storage::ConnectivityStatus)
//! companion describing what happened.
//!
//! This module is **pre-adoption only**. STORAGE-0018 covers the
//! post-adoption observe loop on `Volume`s already in the managed map;
//! this module covers the moment between hotplug detection and the
//! user seeing a candidate in `garden-rake storage add`.
//!
//! See [STORAGE-0019](../../../../../../docs/decisions/STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md)
//! §"Pipeline architecture" for the full design.
//!
//! ## Platform support
//!
//! Linux is the only target where these modules do real work — the
//! `/sys/block`, `/sys/bus/usb` paths they read and write are Linux-
//! specific. On other platforms the production functions still
//! compile (they use plain `std::fs`), but they read paths that don't
//! exist and produce verdicts based on the absence of signals
//! (typically `EmptyEnclosure`). Pipeline orchestration on non-Linux
//! should skip recovery entirely; the modules are exposed
//! cross-platform mainly so the type system catches drift across
//! shared types.
//!
//! Tests that require synthetic sysfs trees with symlinks are
//! Linux-gated since `std::os::unix::fs::symlink` is the cleanest
//! cross-FS way to fake the topology.

pub mod probe;
pub mod recovery;

use std::path::Path;

use garden_common::storage::ConnectivityStatus;
use tokio_util::sync::CancellationToken;

use crate::domain::storage::platform_types::MediumSnapshot;

use self::probe::{probe, SYSFS_ROOT};
use self::recovery::{run_recovery, RecoveryBudget};

/// A medium snapshot enriched with the connectivity stage's verdict.
///
/// The classifier consumes this to decide what to surface to the
/// user — distinguishing `NoMedia` from `Unreachable` from a healthy
/// candidate that just needed a one-shot recovery.
#[derive(Debug, Clone)]
pub struct EnrichedMedium {
    pub snapshot: MediumSnapshot,
    pub status: ConnectivityStatus,
}

/// Evaluate a single medium snapshot through the connectivity stage.
///
/// On Linux: probes sysfs for the underlying block device, runs the
/// recovery escalation if needed (subject to the per-device budget),
/// and re-scans the medium after recovery to pick up any size /
/// partition info that materialized. Returns an [`EnrichedMedium`]
/// with the latest snapshot and the recovery summary.
///
/// On other platforms: returns the input snapshot unchanged with a
/// healthy [`ConnectivityStatus`]. The connectivity stage is sysfs-
/// driven; non-Linux platforms surface candidate health through their
/// own platform APIs (Windows: WMI, future).
///
/// `device_basename` is the bare block device name extracted from the
/// medium's `device_id` — `"sdc"` for `/dev/sdc`, `"PhysicalDrive3"`
/// for `\\.\PhysicalDrive3`, etc. The connectivity stage on non-Linux
/// ignores this argument.
pub async fn evaluate_candidate(
    snapshot: MediumSnapshot,
    device_basename: &str,
    budget: &RecoveryBudget,
    cancel: &CancellationToken,
) -> EnrichedMedium {
    #[cfg(target_os = "linux")]
    {
        evaluate_linux(snapshot, device_basename, budget, cancel).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (device_basename, budget, cancel);
        EnrichedMedium {
            snapshot,
            status: ConnectivityStatus::healthy(),
        }
    }
}

#[cfg(target_os = "linux")]
async fn evaluate_linux(
    snapshot: MediumSnapshot,
    device_basename: &str,
    budget: &RecoveryBudget,
    cancel: &CancellationToken,
) -> EnrichedMedium {
    let sysfs_root = Path::new(SYSFS_ROOT);
    let initial = probe(sysfs_root, device_basename);

    if !initial.recoverable() {
        // Healthy or empty enclosure — return immediately with the
        // probe's warnings (e.g. historical I/O errors) attached.
        return EnrichedMedium {
            snapshot,
            status: ConnectivityStatus {
                recoveries_attempted: 0,
                recovered_via: None,
                duration_ms: 0,
                residual_warnings: initial.warnings,
            },
        };
    }

    // Run recovery. Re-scan the medium afterward so the snapshot
    // reflects any post-recovery state changes (most importantly:
    // a non-zero `size_bytes` once the bridge starts responding).
    let status = run_recovery(sysfs_root, device_basename, initial, budget, cancel).await;
    let post_snapshot = if status.was_recovered() {
        match refresh_snapshot(&snapshot, device_basename) {
            Some(updated) => updated,
            None => snapshot,
        }
    } else {
        snapshot
    };

    EnrichedMedium {
        snapshot: post_snapshot,
        status,
    }
}

/// Re-scan a single medium after recovery succeeded so the snapshot
/// reflects the device's now-readable state. Returns `None` when the
/// device disappeared from the scan (e.g. unplugged mid-recovery).
///
/// This is best-effort: if the platform scan fails or the device
/// can't be matched, we keep the pre-recovery snapshot and let the
/// classifier decide what to surface.
#[cfg(target_os = "linux")]
fn refresh_snapshot(original: &MediumSnapshot, device_basename: &str) -> Option<MediumSnapshot> {
    let snapshots = crate::infra::storage::platform::scan_media();
    snapshots.into_iter().find(|m| {
        m.device_id == original.device_id
            || m.device_id.ends_with(device_basename)
            || extract_basename(&m.device_id).as_deref() == Some(device_basename)
    })
}

/// Extract the bare device basename from a Linux device id.
///
/// `"/dev/sdc"` → `Some("sdc")`. Returns `None` for non-Linux device
/// id formats.
pub fn extract_basename(device_id: &str) -> Option<String> {
    device_id
        .strip_prefix("/dev/")
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_basename_strips_dev_prefix() {
        assert_eq!(extract_basename("/dev/sdc").as_deref(), Some("sdc"));
        assert_eq!(extract_basename("/dev/sda1").as_deref(), Some("sda1"));
        assert_eq!(extract_basename("/dev/nvme0n1").as_deref(), Some("nvme0n1"));
    }

    #[test]
    fn extract_basename_rejects_non_dev_paths() {
        assert!(extract_basename("\\\\.\\PhysicalDrive3").is_none());
        assert!(extract_basename("sdc").is_none());
        assert!(extract_basename("/dev/").is_none());
        assert!(extract_basename("/dev/sub/path").is_none());
    }
}

