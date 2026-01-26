# Topology Maintenance - Background Cleanup Task

**Date**: 2026-01-25  
**Status**: ✅ COMPLETE  
**Related**: Multicast-first discovery implementation  

---

## Problem

Topology cache was accumulating stale stone entries indefinitely. The `maintain_topology()` function existed but was never called, resulting in:
- Multiple old stone IDs appearing in `observe` output (e.g., 4+ leo-main entries with different IDs)
- Stones marked offline only when explicitly sending GOODBYE, never from timeout
- 90-second offline threshold too long (3 chirp cycles, hides brief outages)

---

## Solution

### 1. Reduced Offline Threshold (45s → faster detection)

**File**: `src/moss/src/domain/topology.rs`

```rust
// Before
const OFFLINE_THRESHOLD_SECS: i64 = 90;

// After
const OFFLINE_THRESHOLD_SECS: i64 = 45;  // 1.5 chirp cycles (30s each)
```

**Rationale**:
- Stones chirp every **30 seconds**
- **45 seconds** = 1.5 chirp cycles (tolerates 1 missed chirp)
- Balances responsiveness with network reliability
- Prevents false-positive offline detection from transient network issues

### 2. Periodic Maintenance Task

**File**: `src/moss/src/tasks/coordinator.rs`

Added `start_topology_maintenance()` function:

```rust
pub fn start_topology_maintenance(topology_cache: TopologyCache) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        interval.tick().await; // Skip first immediate tick

        loop {
            interval.tick().await;
            let (marked, evicted) = crate::domain::topology::maintain_topology(&topology_cache).await;
            if marked > 0 || evicted > 0 {
                tracing::debug!(
                    marked_offline = marked,
                    evicted = evicted,
                    "Topology maintenance complete"
                );
            }
        }
    });
}
```

**Behavior**:
- Runs every **30 seconds** (aligns with chirp interval)
- Calls `maintain_topology()` which:
  1. **Marks offline**: Stones not seen for >45s → `status = Offline`
  2. **Evicts old**: Offline stones >24h → removed from cache
  3. **Enforces cap**: If >64 offline stones, evict oldest (LRU)

**Integration**:
Called from `start_all_background_tasks()`:

```rust
let console = state.console.clone();

// Start topology maintenance (mark stale offline, evict old)
start_topology_maintenance(state.topology_cache.clone());

// Start UDP discovery (immediate - critical for stone visibility)
start_discovery_listener(...).await;
```

### 3. Export Function

**File**: `src/moss/src/tasks/mod.rs`

```rust
pub use coordinator::{
    start_all_background_tasks,
    start_discovery_listener, start_hardware_detection,
    start_registry_loader, start_catalog_builder,
    start_health_monitor, start_auto_adoption,
    start_lantern_registration, start_topology_maintenance,  // Added
};
```

---

## Impact

### Before
```
# Multiple stale leo-main entries (different stone IDs from past sessions)
leo-main    [thriving]    019bf83e-ec4d-7371-98f0-fad4acb5938b
leo-main    [thriving]    7906acb4-0eb8-532d-a9fa-8b3c179c4745
leo-main    [thriving]    019bf836-e47a-7263-9592-3f3e4a520a16
leo-main    [thriving]    019be8db-e11d-7481-b279-dd3ec204f76b

# These never got cleaned up (no background maintenance)
```

### After
```
# Only current stone ID (old entries marked offline within 45s, evicted after 24h)
leo-main    [thriving]    7906acb4-0eb8-532d-a9fa-8b3c179c4745

# Maintenance task logs every 30s (only if action taken):
# [DEBUG] Topology maintenance complete { marked_offline: 3, evicted: 0 }
```

---

## Configuration

### Tunable Constants

In `src/moss/src/domain/topology.rs`:

| Constant | Value | Purpose |
|----------|-------|---------|
| `OFFLINE_THRESHOLD_SECS` | 45 | Mark stone offline after this many seconds without chirp |
| `OFFLINE_EVICTION_HOURS` | 24 | Remove offline stones after this many hours |
| `MAX_OFFLINE_STONES` | 64 | Maximum offline stones to track (LRU eviction) |

### Maintenance Interval

In `start_topology_maintenance()`:
```rust
let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
```

**Recommendation**: Keep at 30s to align with stone chirp interval.

---

## Testing

### Verification Steps

1. **Start Moss** on stone:
   ```bash
   sudo systemctl start garden-moss
   ```

2. **Check initial topology** (should see stone chirping):
   ```bash
   garden-rake observe
   ```

3. **Stop Moss** on another stone:
   ```bash
   sudo systemctl stop garden-moss
   ```

4. **Wait 45 seconds**, then check topology:
   ```bash
   garden-rake observe
   # Stopped stone should now show [offline]
   ```

5. **Check logs** for maintenance activity:
   ```bash
   sudo journalctl -u garden-moss -f | grep "Topology maintenance"
   # [DEBUG] Topology maintenance complete { marked_offline: 1, evicted: 0 }
   ```

### Expected Behavior

| Time | Event | Cache State |
|------|-------|-------------|
| T+0s | Stone stops chirping | Still shows `[thriving]` |
| T+45s | Maintenance runs | Marked `[offline]` |
| T+24h | Maintenance runs | Evicted from cache |

---

## Related Changes

This maintenance task complements the multicast-first discovery implementation:

1. **Multicast discovery** (primary):
   - Stones send chirps to `239.255.42.99:7184`
   - Receivers join multicast on all interfaces
   - Solves multi-homed Windows discovery failures

2. **Topology maintenance** (secondary):
   - Cleans up stones that stop chirping
   - Prevents cache bloat
   - Ensures `observe` output shows accurate state

Together, these changes provide:
- ✅ Reliable stone discovery (multicast-first)
- ✅ Accurate topology state (periodic maintenance)
- ✅ Fast offline detection (45s threshold)
- ✅ Automatic cache cleanup (24h eviction)

---

## Files Changed

| File | Change |
|------|--------|
| `src/moss/src/domain/topology.rs` | `OFFLINE_THRESHOLD_SECS`: 90 → **45** |
| `src/moss/src/tasks/coordinator.rs` | Added `start_topology_maintenance()` |
| `src/moss/src/tasks/coordinator.rs` | Integrated into `start_all_background_tasks()` |
| `src/moss/src/tasks/mod.rs` | Exported `start_topology_maintenance` |

---

## Future Enhancements (Optional)

1. **Configurable thresholds**: Environment variables for `OFFLINE_THRESHOLD_SECS`
2. **Metrics**: Expose maintenance stats via `/metrics` endpoint
3. **Graceful eviction**: Notify Lantern when evicting stones
4. **Wake-on-LAN integration**: Preserve MAC addresses for >24h (already implemented via offline state)

---

## References

- **Main changelog**: [CHANGELOG-multicast-first.md](CHANGELOG-multicast-first.md)
- **Architecture docs**: [ARCHITECTURE-REFERENCE.md](ARCHITECTURE-REFERENCE.md#discovery-transport-multicast-first)
- **Design rationale**: [discovery-transport.md](discovery-transport.md)

---

**Status**: Production-ready ✅  
**Version**: 0.1.202601252313  
**Build Date**: 2026-01-25 23:13
