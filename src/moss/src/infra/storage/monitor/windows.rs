//! Windows volume monitor (STORAGE-0014)
//!
//! Polling task (5s interval) diffs scan_volumes() against known set.
//! Calls disk_usage() on each new volume BEFORE sending the Connected event.

use super::{PhysicalStorageEvent, VolumeMonitor};
use crate::infra::storage::platform;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::info;

/// Windows volume monitor using drive-letter polling.
pub struct WindowsVolumeMonitor;

impl VolumeMonitor for WindowsVolumeMonitor {
    fn start(
        self: Box<Self>,
        tx: tokio::sync::mpsc::Sender<PhysicalStorageEvent>,
        token: tokio_util::sync::CancellationToken,
    ) {
        tokio::spawn(async move {
            let mut known: HashSet<String> = HashSet::new();

            // Initialize with current state
            let initial = platform::scan_volumes();
            info!(
                count = initial.len(),
                drives = %initial.iter().map(|v| v.path.as_str()).collect::<Vec<_>>().join(", "),
                "Volume monitor initialized"
            );
            for v in initial {
                known.insert(v.path.clone());
            }

            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {}
                }

                let current = platform::scan_volumes();
                let current_paths: HashSet<String> =
                    current.iter().map(|v| v.path.clone()).collect();

                // New volumes — measure before emitting
                for v in &current {
                    if !known.contains(&v.path) {
                        let (used_bytes, _available) = match platform::disk_usage(&v.mount_path) {
                            Some(du) => (du.used_bytes, du.available_bytes),
                            None => (0, v.capacity_bytes),
                        };

                        info!(
                            path = %v.path,
                            label = ?v.label,
                            removable = v.removable,
                            capacity_gb = v.capacity_bytes / 1_000_000_000,
                            used_gb = used_bytes / 1_000_000_000,
                            "Volume appeared (monitor)"
                        );

                        let event = PhysicalStorageEvent::Connected {
                            device_path: v.path.clone(),
                            mount_path: PathBuf::from(&v.mount_path),
                            label: v.label.clone(),
                            capacity_bytes: v.capacity_bytes,
                            used_bytes,
                            removable: v.removable,
                        };
                        if tx.send(event).await.is_err() {
                            return; // channel closed
                        }
                    }
                }

                // Departed volumes
                for path in &known {
                    if !current_paths.contains(path) {
                        info!(path = %path, "Volume disappeared (monitor)");
                        if tx
                            .send(PhysicalStorageEvent::Disconnected {
                                path: path.clone(),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }

                known = current_paths;
            }
        });

        tracing::info!("Windows volume monitor started (polling)");
    }
}
