# STORAGE-0004: Seed Bank Plug-and-Play Resilience

**Status:** Implemented
**Date:** 2026-01-30

## Context

Users expect a frictionless experience when working with seed banks (USB storage devices). They should be able to:
- Plug in a prepared seed bank at any time
- Have it automatically mount and announce to the garden
- Pull it out when done without causing system issues

The previous implementation had several gaps:
1. **False online status**: API reported `online: true` even when devices weren't mounted
2. **USB detection failures**: Some USB devices report `removable=0` in sysfs
3. **No hot-plug support**: Devices plugged in after startup weren't detected
4. **Device yanking**: Pulling out a device without unmounting caused stale mounts

## Decision

Implement a resilient, self-healing seed bank system with:

### 1. Mount Verification (registry.rs)

Before reporting a seed bank as online:
- Check `/proc/mounts` to verify device is actually mounted
- Perform liveness check via `get_disk_usage()` - capacity of 0 indicates stale mount
- Skip seed banks that fail these checks

```rust
// Get device from mount info - this tells us if actually mounted
let device_opt = Self::get_device_for_mount(&mount_path).await;
let is_mounted = device_opt.is_some();

if !is_mounted {
    continue; // Skip - not actually mounted
}

// Liveness check: if capacity is 0, mount is likely stale/dead
if capacity_bytes == 0 {
    Self::cleanup_stale_mount(&mount_path).await;
    continue;
}
```

### 2. Enhanced USB Detection (device.rs)

Six-method detection for maximum reliability:

| Method | Check | Fallback Reason |
|--------|-------|-----------------|
| 1 | `/sys/block/{dev}/removable` flag | Many USB drives report 0 |
| 2 | Canonical path contains `/usb` | Symlink may not resolve |
| 3 | uevent contains `DRIVER=usb-storage` or `uas` | Driver may vary |
| 4 | `/sys/block/{dev}/device/transport` = usb | Not always present |
| 5 | Sysfs link contains `/usb` | Virtualized devices |
| 6 | `lsblk -dno TRAN` = usb | Most reliable, external cmd |

Additionally, devices with `zen-seed` filesystem label are trusted (user explicitly prepared them).

### 3. Hot-Plug Detection (coordinator.rs)

Background task runs every 10 seconds:
- Scans for new `zen-seed` labeled devices
- Triggers auto-mount for unmounted devices
- Updates storage cache and broadcasts beacon

```rust
pub fn start_seedbank_hotplug_detection(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            // Scan triggers auto-mount for any new zen-seed devices
            if let Ok(registry) = SeedBankRegistry::scan().await {
                // Update storage cache and broadcast
            }
        }
    });
}
```

### 4. Stale Mount Cleanup (registry.rs)

When device is physically removed:
- Detected via I/O error or 0 capacity
- Lazy unmount (`umount -l`) used to avoid hanging
- Mount point cleaned up automatically

```rust
async fn cleanup_stale_mount(mount_path: &str) {
    // Use lazy unmount to avoid hanging on dead device
    Command::new("sudo")
        .args(["umount", "-l", mount_path])
        .output()
        .await;
}
```

## Resilience Matrix

| Scenario | Detection | Action | Time |
|----------|-----------|--------|------|
| Device plugged in | Hot-plug scan | Auto-mount, announce | ~10s |
| Moss restart | Bootstrap scan | Re-discover mounts | Immediate |
| System reboot | Bootstrap scan | Auto-mount via label | Immediate |
| Device yanked | I/O error or 0 capacity | Lazy unmount, notify garden | ~10s |
| USB detection fails | zen-seed label trusted | Proceed with mount | Immediate |

## File Changes

- `src/moss/src/infra/storage/registry.rs` - Mount verification, stale cleanup
- `src/moss/src/infra/storage/device.rs` - 6-method USB detection
- `src/moss/src/tasks/coordinator.rs` - Hot-plug detection task
- `src/probe/src/garden.rs` - Platform detection for physical tests
- `src/common/src/constants/paths.rs` - Linux-specific path constants

## Testing

Physical validation tests in `src/probe/src/tests/nurturing.rs`:
- Check Linux platform before running SSH-based tests
- Verify actual filesystem state via SSH to stones
- Use Linux-specific paths for cross-platform probe

## Consequences

### Positive
- Frictionless user experience: plug in and go
- Self-healing: recovers from failures automatically
- No manual intervention needed for hot-plug or device removal

### Negative
- 10-second detection delay for hot-plug (acceptable trade-off)
- Requires `sudo` for mount operations
- Extra `lsblk` calls for USB detection fallback
