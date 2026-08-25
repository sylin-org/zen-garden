---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-04-04
---

# STORAGE-0018: Device Health Monitor

**Date**: 2026-04-04
**Status**: Proposed
**Depends on**: STORAGE-0014 (Storage Platform Architecture), STORAGE-0017 (Volume State Machine)

## Context

### The Symptom

Unplugging a USB SSD from stone-golden-summit left a stale block device
reference in the kernel. The device disappeared physically but the kernel
retained `/dev/sdb1`, retrying I/O on logical block 36 every 60 seconds.
The tty1 console filled with `Buffer I/O error on dev sdb1` messages
indefinitely. Moss had no awareness of the problem — the VolumeMonitor
saw the disconnect and transitioned the Volume to Offline, but the kernel's
ghost device continued spamming errors unchecked.

### Problem Class

This is one instance of a broader category: **the OS has a device in a bad
state that Moss doesn't know about**. The same class includes:

| Scenario | Signal | Impact |
|----------|--------|--------|
| Stale block device reference | sysfs `state` = offline/transport-offline | Kernel I/O error spam, console noise |
| Filesystem remounted read-only | `/proc/mounts` `ro` flag | Silent data loss — writes silently fail |
| Failing drive (SMART) | Growing `ioerr_cnt` in sysfs | Data corruption risk |
| NAS mount gone unresponsive | `statvfs()` hangs or returns error | Blocked I/O, hung processes |

### What Exists Today

The existing architecture has every integration point needed:

- **`observe_all()`** runs on the periodic storage tick (~3s), probing disk
  usage via `platform.disk_usage()`. But it only checks capacity — it has no
  device health awareness.
- **`VolumeMonitor`** catches clean connect/disconnect via udev or polling.
  But once a device is Offline in the Volume map, Moss stops looking at it.
  A ghost device that the VolumeMonitor already processed (disconnect event
  received) gets no further attention.
- **`StorageHealth`** validates layout and writability but only on-demand
  (API call), not continuously.
- **`Volume::observe_metrics()`** transitions Online ↔ Degraded based on
  capacity. It has no health signal input.

### Design Principle

STORAGE-0017 established: OS facts flow into Volume methods; domain events
flow out as return values. Device health is an OS fact. It belongs in the
same flow as disk metrics — probed by infra, consumed by the domain.

## Decision

### Extend the existing observe cycle with device health probing.

No new background tasks. No new event channels. Four touch points:

### 1. `DeviceHealth` value type (`platform_types.rs`)

A platform-agnostic description of device health, produced by infra,
consumed by domain:

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceHealth {
    /// Basic I/O probe succeeded (statvfs or equivalent).
    pub responsive: bool,

    /// Filesystem mounted read-only (ext4 error recovery, hardware write-protect).
    pub read_only: bool,

    /// sysfs entry exists but physical device is gone (kernel ghost).
    /// Linux: /sys/block/{dev}/device/state is "offline" or "transport-offline".
    pub stale_reference: bool,

    /// Cumulative I/O error count from the device driver (if available).
    /// Linux: /sys/block/{dev}/device/ioerr_cnt.
    pub io_errors: u64,
}
```

### 2. `StoragePlatform` trait extension

One new sync method:

```rust
trait StoragePlatform {
    // ... existing methods ...

    /// Probe device health from OS-level signals.
    fn probe_device_health(&self, device_path: &str, mount_path: &str) -> DeviceHealth;

    /// Remove a stale block device reference from the kernel.
    /// Linux: writes to /sys/block/{dev}/device/delete.
    /// Only called for removable devices with stale_reference = true.
    fn remove_stale_device(&self, device_path: &str) -> anyhow::Result<()>;
}
```

### 3. `Volume` state machine: health-aware observation

`observe_metrics()` gains a `DeviceHealth` parameter. The Volume decides
what the health signals mean:

```rust
pub fn observe_metrics(
    &mut self,
    metrics: Option<DiskMetrics>,
    health: DeviceHealth,
) -> Vec<StorageChanged> {
    // Stale or unresponsive → disconnect (Offline)
    if health.stale_reference || !health.responsive {
        return self.disconnect();
    }

    // Read-only transition → degrade
    if health.read_only {
        // ... transition to Degraded("filesystem read-only")
    }

    // Existing capacity-based transitions
    // ...
}
```

A stale device triggers `disconnect()`, which is idempotent — if the
VolumeMonitor already disconnected it, the empty vec return means nothing
happens. If the VolumeMonitor missed it (race, udev failure), health
probing catches it.

### 4. `observe_all()`: probe health alongside metrics

```rust
pub async fn observe_all(volumes, platform) -> Vec<StorageChanged> {
    for vol in map.values_mut() {
        let health = platform.probe_device_health(vol.path(), mount_str);
        let metrics = platform.disk_usage(mount_str).map(/* ... */);
        events.extend(vol.observe_metrics(metrics, health));

        // Stale device remediation (removable only)
        if health.stale_reference && vol.removable() {
            if let Err(e) = platform.remove_stale_device(vol.path()) {
                tracing::warn!(path = %vol.path(), error = %e, "stale device cleanup failed");
            }
        }
    }
}
```

### Remediation Policy

| Condition | Device type | Action |
|-----------|-------------|--------|
| Stale reference | Removable (USB/SD) | Auto-cleanup: SCSI device delete |
| Stale reference | Fixed (SATA/NVMe) | Degrade + report only |
| Read-only remount | Any | Degrade + report |
| Growing I/O errors | Any | Degrade + report |
| Unresponsive | Removable | Disconnect + auto-cleanup |
| Unresponsive | Fixed | Degrade + report |

Automatic remediation is limited to removable devices where the user's
intent (unplug) is unambiguous. Fixed storage problems require operator
judgment — Moss reports but does not act.

### Platform Implementation

**Linux** — all from sysfs/procfs, no external tools:

| Signal | Source | Cost |
|--------|--------|------|
| `responsive` | `statvfs()` success (already done for `disk_usage`) | ~free |
| `read_only` | `/proc/mounts` ro flag (`is_mount_readonly()` exists) | 1 procfs read |
| `stale_reference` | `/sys/block/{dev}/device/state` = offline/transport-offline | 1 sysfs read |
| `io_errors` | `/sys/block/{dev}/device/ioerr_cnt` | 1 sysfs read |
| Remediation | `echo 1 > /sys/block/{dev}/device/delete` | 1 sysfs write |

**Windows** — equivalent signals:

| Signal | Source |
|--------|--------|
| `responsive` | `GetDiskFreeSpaceExW()` success |
| `read_only` | `GetVolumeInformationW()` `FILE_READ_ONLY_VOLUME` flag |
| `stale_reference` | Not applicable (Windows cleans up device references) |
| `io_errors` | WMI `Win32_DiskDrive.Status` (future enhancement) |

## Consequences

### Positive

- **Ghost devices detected and cleaned up automatically.** The
  stone-golden-summit scenario becomes self-healing for removable devices.
- **Read-only remounts surfaced immediately.** Not discovered hours later
  when a write fails.
- **Zero new infrastructure.** Extends the existing observe cycle, reuses
  existing types and channels.
- **Platform-aware remediation.** Linux sysfs cleanup is safe and
  well-understood. Windows doesn't need it (different device lifecycle).

### Negative

- **observe_all() does slightly more I/O per tick.** Two extra sysfs reads
  per online volume (~microseconds each). Negligible compared to the
  existing `df` subprocess call for disk_usage.
- **Auto-remediation for removable devices is opinionated.** If a device
  is physically present but the kernel thinks it's gone (cable issue, hub
  glitch), SCSI delete will prevent automatic re-detection. Mitigated:
  only removable devices, and re-plugging triggers udev re-detection.

### Neutral

- `StorageChanged` enum unchanged — stale/unresponsive devices emit the
  existing `Released`/`Reclassified` events via `disconnect()`.
- `StorageHealth` API response gains device-level health fields, providing
  richer diagnostic information through the existing endpoint.

## Files Affected

| File | Change |
|------|--------|
| `src/moss/src/domain/storage/platform_types.rs` | Add `DeviceHealth` value type |
| `src/moss/src/domain/traits/storage_platform.rs` | Add `probe_device_health()`, `remove_stale_device()` |
| `src/moss/src/domain/storage/volume.rs` | Extend `observe_metrics()` signature with `DeviceHealth` |
| `src/moss/src/domain/storage/collection.rs` | Update `observe_all()` to probe and pass health |
| `src/moss/src/domain/storage/health.rs` | Add device health fields to `SeedBankHealth` |
| `src/moss/src/infra/storage/platform.rs` | Implement Linux/Windows `probe_device_health()` and `remove_stale_device()` |
