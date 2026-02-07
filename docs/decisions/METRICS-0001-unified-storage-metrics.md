# METRICS-0001: Unified Storage Metrics Collection

**Status:** Approved  
**Date:** 2026-01-26  
**Deciders:** Architecture Review

## Context

Storage metrics were collected in two parallel systems:

1. **Static Hardware Detection** (`HardwareCapabilities.storage`)
   - Collected once at boot via `detect_storage()`
   - Used platform-specific `df` subprocess calls
   - Stored in `Vec<StorageDevice>` with `used_percent` field
   - Never refreshed, causing stale data (e.g., "0% used" from boot time)

2. **Live Metrics Collection** (`StoneResources.disk`)
   - Collected every 30s via `sysinfo::Disks`
   - Stored in single `DiskMetrics` for root mount
   - Fresh data, used by presence protocol

This duplication caused:
- Stale disk usage shown in `observe` command
- Redundant code (~200 lines of `detect_storage()`)
- Subprocess spawning overhead (`df` calls)
- Confusion about which data source to use

## Decision

**Consolidate all storage metrics into `StoneResources`** as the single source of truth.

### Key Changes

1. **Remove** `HardwareCapabilities.storage` field and `detect_storage()` function
2. **Replace** `DiskMetrics` (single disk) with `Vec<StorageMetrics>` (all disks)
3. **Collect** full storage inventory every 30s via `sysinfo::Disks`
4. **Eliminate** all `df` subprocess calls

### New Structure

```rust
pub struct StoneResources {
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub storage: Vec<StorageMetrics>,  // Was: pub disk: DiskMetrics
    pub uptime_seconds: u64,
    pub uptime_friendly: String,
}

pub struct StorageMetrics {
    pub identifier: String,      // sda, nvme0n1, C:
    pub mount_point: String,      // /, /data, C:\
    pub total_gb: u64,
    pub used_gb: u64,
    pub available_gb: u64,
    pub used_percent: f32,
    pub disk_type: DiskType,
    pub filesystem: String,
}
```

## Rationale

### Why Storage is Semi-Dynamic (Not Static)

Storage state changes at multiple timescales:
- **Seconds:** Usage percentage
- **Minutes:** Mount/unmount operations
- **Hours:** Hot-swap drive insertion/removal
- **Static:** Individual disk physical size

There is no clean "static vs dynamic" split. The entire storage **state** is semi-dynamic.

### Why Consolidation Wins

1. **Single Source of Truth:** Display code reads one cache, always fresh
2. **Handles Hot-Swap:** Inventory naturally updates every 30s
3. **Simpler Code:** Eliminates 200+ lines of platform-specific `df` parsing
4. **Better Performance:** No subprocess spawning
5. **Consistent Freshness:** All storage data same age (30s max staleness)

### Trade-offs Accepted

- Storage inventory conceptually feels like "hardware" not "metrics"
- But pragmatically, it's semi-dynamic state that benefits from frequent refresh
- The complexity saved by eliminating duplication outweighs categorical purity

## Implementation Impact

### Files Modified
- `common/src/types.rs` - Remove `StorageDevice`, modify `StoneResources`
- `common/src/types.rs` - Add `StorageMetrics` struct
- `common/src/metrics/system.rs` - Remove `detect_storage()`, enhance `get_disk_metrics()`
- `moss/src/tasks/hardware_detection.rs` - Remove storage detection phase
- `moss/src/tasks/metrics_collector.rs` - Handle `Vec<StorageMetrics>`
- `rake/src/commands/discovery/observe.rs` - Read from `system_resources.storage`
- `rake/src/commands/discovery/status.rs` - Read from `system_resources.storage`

### Breaking Changes
- `HardwareCapabilities.storage` field removed
- API `/capabilities` response no longer includes storage inventory
- Clients must use `/metrics` endpoint for storage information

### Migration Path
- Display code must fetch both `/capabilities` (static hardware) and `/metrics` (live resources)
- For backward compatibility, consider adding `storage_available_via_metrics: true` flag to capabilities

## Alternatives Considered

### A. Periodic Re-scan of HardwareCapabilities
Keep both systems but periodically update `HardwareCapabilities.storage.used_percent`.

**Rejected:** Still maintains duplication, race conditions, mixing concerns.

### B. Split Static and Dynamic Storage Fields
Remove `used_percent` from `StorageDevice`, keep separate inventory.

**Rejected:** Requires two-source composition in display code, adds complexity, doesn't handle hot-swap elegantly.

## References

- [src/common/src/metrics/system.rs](../../src/common/src/metrics/system.rs) - Metrics collection
- [src/moss/src/tasks/metrics_collector.rs](../../src/moss/src/tasks/metrics_collector.rs) - Collection task
- [Components](../reference/components.md) - Platform paths and metrics
