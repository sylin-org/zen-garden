//! Linux volume monitor (STORAGE-0014)
//!
//! Primary: udev blocking thread detects device add/remove.
//! Fallback: polling task (10s) diffs scan_volumes() against known set.
//! Both paths measure disk_usage() BEFORE sending Connected events.

use super::{PhysicalStorageEvent, StorageResources, VolumeMonitor};
use crate::infra::storage::platform;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Linux volume monitor using udev + polling fallback.
pub struct LinuxVolumeMonitor;

impl VolumeMonitor for LinuxVolumeMonitor {
    fn start(
        self: Box<Self>,
        tx: tokio::sync::mpsc::Sender<PhysicalStorageEvent>,
        token: tokio_util::sync::CancellationToken,
    ) {
        // Primary: udev blocking thread
        let udev_tx = tx.clone();
        let udev_token = token.clone();
        std::thread::spawn(move || {
            if let Err(e) = run_udev_watcher(udev_tx, udev_token) {
                warn!(error = %e, "udev watcher failed, polling fallback active");
            }
        });

        // Fallback: polling task (10s interval)
        tokio::spawn(async move {
            let mut known: HashSet<String> = HashSet::new();

            // Initialize with current state
            for v in platform::scan_volumes() {
                known.insert(v.path.clone());
            }

            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {}
                }

                let current = platform::scan_volumes();
                let current_paths: HashSet<String> =
                    current.iter().map(|v| v.path.clone()).collect();

                // New volumes — measure before emitting
                for v in &current {
                    if !known.contains(&v.path) {
                        debug!(path = %v.path, "Volume appeared (polling)");
                        let (used_bytes, _) = measure_usage(&v.mount_path, v.capacity_bytes);

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

                // Departed volumes.  Also catches NTFS-3G stale FUSE mounts:
                // after USB removal the mount may linger in /proc/mounts, but
                // the block device node (/dev/sdbN) disappears immediately.
                for path in &known {
                    let in_proc = current_paths.contains(path);
                    let dev_present = !path.starts_with("/dev/") || Path::new(path).exists();
                    if !in_proc || !dev_present {
                        debug!(path = %path, "Volume disappeared (polling)");
                        if tx
                            .send(PhysicalStorageEvent::Disconnected { path: path.clone() })
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

        tracing::info!("Linux volume monitor started (udev + polling)");
    }
}

/// Measure disk usage. Returns (used_bytes, available_bytes).
/// Falls back to (0, capacity) if measurement fails.
fn measure_usage(mount_path: &str, capacity_bytes: u64) -> (u64, u64) {
    match platform::disk_usage(mount_path) {
        Some(du) => (du.used_bytes, du.available_bytes),
        None => (0, capacity_bytes),
    }
}

/// Build a StorageResources from disk_usage, falling back to zeros.
fn resources_for_device(mount_path: &str, capacity_bytes: u64) -> StorageResources {
    let (used_bytes, available_bytes) = measure_usage(mount_path, capacity_bytes);
    StorageResources {
        capacity_bytes,
        used_bytes,
        available_bytes,
    }
}

fn run_udev_watcher(
    tx: tokio::sync::mpsc::Sender<PhysicalStorageEvent>,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::os::unix::io::AsRawFd;

    let socket = udev::MonitorBuilder::new()
        .context("Failed to create udev monitor")?
        .match_subsystem("block")
        .context("Failed to set subsystem filter")?
        .listen()
        .context("Failed to start udev monitor")?;

    tracing::info!("udev volume watcher started");

    loop {
        if token.is_cancelled() {
            break Ok(());
        }

        let mut pollfd = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };

        // SAFETY: `pollfd` is stack-allocated and valid for the duration of the call.
        // `fd` is a valid file descriptor owned by `socket`. Timeout (5000ms) is positive.
        // The function writes only to `pollfd.revents`, which is in our stack frame.
        let ret = unsafe { libc::poll(&mut pollfd, 1, 5000) };
        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err.into());
        }
        if ret == 0 {
            continue;
        }

        while let Some(event) = socket.iter().next() {
            let devnode = match event.devnode() {
                Some(node) => node.to_string_lossy().to_string(),
                None => continue,
            };

            match event.event_type() {
                udev::EventType::Add => {
                    debug!(device = %devnode, "udev: block device added");
                    // Wait briefly for the device to settle
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Try to build a snapshot and measure usage before emitting
                    if let Some(snap) = build_snapshot_for_device(&devnode) {
                        let disk_snapshot =
                            resources_for_device(&snap.mount_path, snap.capacity_bytes);
                        let event = PhysicalStorageEvent::Connected {
                            device_path: snap.path.clone(),
                            mount_path: PathBuf::from(&snap.mount_path),
                            label: snap.label,
                            capacity_bytes: disk_snapshot.capacity_bytes,
                            used_bytes: disk_snapshot.used_bytes,
                            removable: snap.removable,
                        };
                        let _ = tx.blocking_send(event);
                    }
                }
                udev::EventType::Remove => {
                    debug!(device = %devnode, "udev: block device removed");
                    let _ = tx.blocking_send(PhysicalStorageEvent::Disconnected { path: devnode });
                }
                _ => {}
            }
        }
    }
}

/// Build a VolumeSnapshot for a device if it's currently mounted.
fn build_snapshot_for_device(device: &str) -> Option<platform::VolumeSnapshot> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == device {
            let mount_path = parts[1];
            let fs_type = parts[2];
            return Some(platform::VolumeSnapshot {
                path: device.to_string(),
                mount_path: mount_path.to_string(),
                label: platform::device_label(device),
                capacity_bytes: platform::device_capacity(device),
                removable: platform::is_removable(device),
                // STORAGE-0019: feeds FsCapabilities lookup downstream.
                filesystem: Some(fs_type.to_ascii_lowercase()),
            });
        }
    }
    None
}
