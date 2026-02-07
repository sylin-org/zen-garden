---
audience: [developer, contributor]
doc_type: spec
status: current
last_verified: 2026-02-07
---

# Topology Cache Specification

The topology cache tracks all known stones in the garden. A periodic maintenance task marks stale stones offline and evicts entries that have been offline for extended periods.

---

## Cache Behavior

Each stone maintains its own topology cache, populated by incoming chirp announcements over UDP discovery. When a stone chirps, every listening stone updates its cache entry.

### Lifecycle of a Cache Entry

| State | Meaning | Transition |
|-------|---------|------------|
| Thriving | Stone is actively chirping | Remains thriving while chirps arrive within 45s |
| Offline | Stone stopped chirping | Marked offline by maintenance after 45s of silence |
| Evicted | Entry removed | Removed by maintenance after 24h offline |

---

## Maintenance Task

A background task runs every **30 seconds** (aligned with the chirp interval) and performs three operations:

1. **Mark offline**: Stones not seen for >45 seconds are set to `status = Offline`
2. **Evict old**: Offline stones older than 24 hours are removed from the cache
3. **Enforce cap**: If >64 offline stones exist, the oldest are evicted (LRU)

### Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `OFFLINE_THRESHOLD_SECS` | 45 | Mark stone offline after this many seconds without chirp |
| `OFFLINE_EVICTION_HOURS` | 24 | Remove offline stones after this many hours |
| `MAX_OFFLINE_STONES` | 64 | Maximum offline entries to retain (LRU eviction) |
| Maintenance interval | 30s | How often the maintenance task runs |

### Why 45 Seconds

Stones chirp every 30 seconds. A 45-second threshold (1.5 chirp cycles) tolerates one missed chirp due to transient network issues while still detecting actual outages within a single maintenance cycle.

---

## Verification

Check topology state:
```bash
garden-rake observe
```

Check maintenance activity in logs:
```bash
sudo journalctl -u garden-moss -f | grep "Topology maintenance"
# [DEBUG] Topology maintenance complete { marked_offline: 1, evicted: 0 }
```

Test offline detection:
1. Stop Moss on a stone: `sudo systemctl stop garden-moss`
2. Wait 45 seconds
3. Run `garden-rake observe` — the stopped stone should show `[offline]`

---

## References

- [Discovery Transport spec](discovery-transport.md) — how chirps are delivered
- [COMM-0001: P2P Transport Singleton](../decisions/COMM-0001-p2p-transport-singleton.md)
