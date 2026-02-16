---
audience: [contributor, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-16
canonical: true
---

# COMM-0005: UDP Payload Hygiene — Chirp Slimming

**Status**: Accepted
**Date**: 2026-02-16
**Tags**: p2p, udp, performance, networking

---

## Context

Live traffic analysis on an 8-stone integration garden (65-second capture, 170 datagrams) revealed that `stone_chirp` messages — broadcast every 30 seconds by every stone — are significantly larger than necessary.

### Measured payload sizes

| Message Type    | Count | Min    | Max       | Avg       |
|-----------------|-------|--------|-----------|-----------|
| tools_beacon    | 32    | 991 B  | **4,776 B** | 2,552 B |
| stone_chirp     | 37    | 442 B  | **3,093 B** | 2,048 B |
| storage_beacon  | 39    | 266 B  | 578 B     | 378 B     |
| discovery_*     | 52    | 206 B  | 603 B     | 295 B     |
| stone_goodbye   | 10    | 165 B  | 477 B     | 290 B     |

Ethernet MTU is 1,500 bytes (1,472 payload after IP+UDP headers). Every chirp over this threshold is IP-fragmented — 7 of 8 stones exceeded it. Fragment loss on UDP is non-recoverable (losing any one fragment drops the entire datagram).

### Chirp anatomy (2,660-byte example from stone-coral-prairie)

| Section | Bytes | % | Consumed by receiver? |
|---------|-------|---|-----------------------|
| `cpu.features` (108 items) | **921** | **35%** | Never read from remote chirps |
| `capabilities` (rest) | 645 | 24% | Partially |
| Core fields | ~350 | 13% | Yes |
| `services` (5 offerings) | ~350 | 13% | Yes |
| Envelope overhead | ~80 | 3% | — |
| `discovered_at` + `last_seen` + `status` | ~90 | 3% | **Always overwritten** |
| Pond signing (when present) | ~274 | 10% | — |

### Field-by-field audit

A full code-level trace of every field from sender → receiver → downstream consumers identified several categories of dead weight in the chirp payload:

**Always overwritten by receiver** (chirped value discarded):
- `discovered_at` — receiver overwrites to `Utc::now()` on insert
- `last_seen` — receiver overwrites to `Utc::now()` on every update
- `status` — receiver overwrites to `StoneStatus::Online`

**Never read from remote chirps** (stored, sometimes passed through APIs as opaque JSON):
- `capabilities.hardware.cpu.features` — largest single field (35–46% of chirp), lists 40–108 CPU flags like `fpu`, `vme`, `sse4_2`. No Moss logic, Rake, Lantern, or API handler reads this from peer data.
- `capabilities.hardware.cpu.threads` — never consumed
- `capabilities.hardware.swap_mb` — never consumed
- `capabilities.runtime.docker_version` — never consumed

**Redundant with parent fields**:
- `capabilities.stone_id` — duplicate of top-level `stone_id`
- `capabilities.stone_name` — duplicate of top-level `stone_name`

### Impact on Windows

Windows returns `WSAEMSGSIZE` (error 10040) when a received datagram exceeds the buffer. Prior to the buffer-size fix (also done today), this caused silent datagram loss. Even with the buffer fix, large fragmented UDP datagrams remain fragile.

---

## Decision

Strip dead-weight fields from the chirp wire payload at the sender, before JSON serialization. The domain type (`TopologyEntry`) remains unchanged — only the broadcast representation is slimmed.

### Fields stripped from chirp broadcast

| Field | Bytes saved | Reason |
|-------|-------------|--------|
| `capabilities.hardware.cpu.features` | **~900 B** | Never read from remote chirps; 35–46% of payload |
| `capabilities.stone_id` | ~40 B | Redundant with top-level `stone_id` |
| `capabilities.stone_name` | ~25 B | Redundant with top-level `stone_name` (zeroed to `""`) |
| `capabilities.hardware.swap_mb` | ~12 B | Never consumed |
| `capabilities.hardware.cpu.threads` | ~5 B | Never consumed |
| `capabilities.runtime.docker_version` | ~8 B | Never consumed |
| `capabilities.detection_status` | ~12 B | Never consumed from peers (set to `complete` default) |

**Not stripped** (non-optional fields, cost exceeds risk of adding serde defaults):
- `discovered_at`, `last_seen`, `status` (~90 B total — always overwritten by receiver but required for deserialization)

**Estimated saving: ~1,000 B per chirp** (~50% reduction). Typical chirps drop from ~2,000 B to ~1,000 B — at or below the 1,472 B fragmentation threshold for most stones.

### Implementation approach

A `strip_for_chirp()` method on `TopologyEntry` produces a clone with dead-weight fields zeroed/removed. The `send_udp_announcement()` function calls this before serializing.

This is non-breaking: receivers already handle `None`/missing fields via `skip_serializing_if` and `default` serde attributes on all stripped fields.

---

## What this does NOT change

- **`TopologyEntry` struct**: No fields removed from the domain type. API responses, topology cache, and persistence continue to use the full struct.
- **Receiver logic**: No changes. `upsert_from_chirp()` already overwrites `status`, `discovered_at`, `last_seen`. Other stripped fields are `Option` types that default to `None`.
- **`tools_beacon` / `storage_beacon`**: Not addressed here (lower priority, separate concern).
- **Wire format**: Still JSON. Compression and format changes (MessagePack, compact keys) are future considerations.

---

## Future considerations

- **Tools beacon hygiene**: Remove redundant per-delta fields (`projection.stone_id`, `projection.tool_fqid`, `delta.cursor`, `beacon.endpoint`, `beacon.timestamp`). Saves ~120 B × N deltas.
- **Compression**: Deflate/zlib on JSON payloads (5–10× ratio) would bring all messages well under MTU. `miniz_oxide` is pure Rust and already in the dependency tree.
- **Dedicated wire types**: Separate `ChirpPayload` struct instead of reusing `TopologyEntry` for cleaner separation of wire format and domain model.

---

## References

- [COMM-0001: P2P Transport Singleton](COMM-0001-p2p-transport-singleton.md)
- [COMM-0004: Multicast-First Discovery](COMM-0004-multicast-first-discovery.md)
- Live traffic capture: 8-stone garden, 170 datagrams over 65 seconds, February 2026
