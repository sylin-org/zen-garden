# STORAGE-0003: Storage Beacon Protocol

**Status:** Accepted  
**Date:** 2026-01-29  
**Author:** Architecture Team

---

## Context

Stones with seed banks need to announce their storage capabilities to enable cross-stone routing. The existing `STONE_CHIRP` broadcasts the full `TopologyEntry` (1-4KB) every 30 seconds, which is:
- Too heavy for storage-only changes
- Coupled with general topology information
- Inefficient for event-driven storage updates

## Decision

Implement a separate **Storage Beacon** (`STORAGE_BEACON`) announcement type with its own lightweight cache structure.

### Design Principles

1. **Event-driven, not periodic** - Beacons broadcast on state changes, not intervals
2. **Lightweight** - ~150-400 bytes vs 1-4KB for chirps
3. **Separate cache** - `StorageCache` references `TopologyCache` by `stone_id`
4. **Lurk-listen** - All stones listen for beacons and update their cache

### Beacon Structure

```rust
/// Storage capability announcement (broadcast on change)
pub struct StorageBeacon {
    pub stone_id: String,           // Links to TopologyEntry
    pub stone_name: String,         // Human-readable reference
    pub endpoint: String,           // HTTP endpoint for storage API
    pub seed_banks: Vec<SeedBankAnnouncement>,
    pub timestamp: DateTime<Utc>,
}

pub struct SeedBankAnnouncement {
    pub id: String,                 // "seed-nas-main" (GUIDv7)
    pub name: String,               // "NAS Main" (human name)
    pub protocols: Vec<String>,     // ["s3", "storage"]
    pub access: StorageAccess,      // Direct | Proxy { via: stone_id }
    pub visibility: String,         // "open" | "closed"
    pub health: String,             // "healthy" | "degraded" | "read-only"
    pub capacity_bytes: u64,
    pub used_bytes: u64,
}

pub enum StorageAccess {
    Direct,
    Proxy { via: String },  // stone_id of gateway
}
```

### Event Triggers

| Event | Action | Scope |
|-------|--------|-------|
| Seed bank mounted | Broadcast `STORAGE_BEACON` | Local stone only |
| Seed bank unmounted | Broadcast `STORAGE_BEACON` (updated list) | Local stone only |
| Seed bank visibility changed | Broadcast `STORAGE_BEACON` | Local stone only |
| Stone comes online | Receive `STONE_CHIRP` from new stone → All stones with seed banks broadcast `STORAGE_BEACON` | Garden-wide |
| Receive beacon from peer | Update local `StorageCache` for that `stone_id` | Local cache |
| Stone goes offline | Remove from `StorageCache` (topology-driven) | Local cache |

### Stone Online Trigger Flow

When a new stone joins the garden:

```
New Stone                    Existing Stones (with storage)
    │                                  │
    │ ─── STONE_CHIRP ───────────────> │
    │                                  │
    │     (all stones with seed banks  │
    │      hear the chirp and beacon)  │
    │                                  │
    │ <────── STORAGE_BEACON ───────── │ Stone A
    │ <────── STORAGE_BEACON ───────── │ Stone B
    │ <────── STORAGE_BEACON ───────── │ Stone C
    │                                  │
    │  (new stone now has full         │
    │   storage cache populated)       │
    │                                  │
```

### Cache Structure

```rust
/// Storage routing cache - separate from topology, references it
pub struct StorageCache {
    /// stone_id → StorageBeacon (last known state)
    beacons: HashMap<String, StorageBeacon>,
}

impl StorageCache {
    /// Get all stones with s3 capability
    pub fn find_s3_gateways(&self) -> Vec<(&str, &SeedBankAnnouncement)>;
    
    /// Get specific seed bank by name across all stones
    pub fn find_by_name(&self, name: &str) -> Option<(&str, &SeedBankAnnouncement)>;
    
    /// Remove entry when stone goes offline
    pub fn remove(&mut self, stone_id: &str);
    
    /// Check if stone_id exists in topology before using cached beacon
    pub fn is_valid(&self, stone_id: &str, topology: &TopologyCache) -> bool;
}
```

### Beacon Lifecycle

- **No periodic keep-alive** - Topology chirps handle stone liveness
- **Cache eviction** - When stone disappears from topology, remove from StorageCache
- **Immediate broadcast** - On any storage state change, beacon within 100ms
- **Deduplication** - Hash beacon content, skip broadcast if unchanged (like chirps)

### Announcement Type

```rust
// In announcement_types.rs
pub const STORAGE_BEACON: &str = "storage_beacon";
```

## Consequences

### Positive

- **Efficient** - Only stones with storage send beacons, only on changes
- **Fast cache population** - New stones get full storage map within seconds
- **Decoupled** - Storage routing independent of general topology updates
- **Scalable** - Works well with many stones, few having storage

### Negative

- **New protocol** - Another message type to handle
- **Coordination** - "Stone online" trigger requires all storage-having stones to respond
- **Potential storm** - If 10 stones have storage, new stone gets 10 beacons at once (acceptable)

## Implementation Notes

1. Add `STORAGE_BEACON` to `announcement_types.rs`
2. Add `StorageBeacon` and `SeedBankAnnouncement` to `garden_common::storage`
3. Create `StorageCache` in `moss/src/domain/storage_cache.rs`
4. Hook beacon broadcast to:
   - `SeedBankRegistry` mount/unmount events
   - Visibility change API
   - Coordinator receiving `STONE_CHIRP` (if local has storage)
5. Hook beacon receive to coordinator for cache updates

## References

- [STORAGE-0001](../specs/STORAGE-0001-seed-bank-onboarding.md) - Seed bank onboarding
- [STORAGE-0002](STORAGE-0002-api-structure.md) - API structure
- [Storage Capability Model](../proposals/ongoing/storage-capability-model.md) - Full storage capability design
