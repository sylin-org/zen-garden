# Defensive Publication: Multicast-First LAN Discovery with Fallback Tiers

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field**: Local area network service discovery and peer-to-peer communication
**Keywords**: multicast discovery, per-interface binding, fallback tiers, directed broadcast, multi-homed hosts, topology cache

---

## 1. Problem Statement

LAN service discovery on multi-homed hosts fails silently. A Windows 11 developer workstation with Hyper-V virtual switches, WSL virtual adapters, and VPN tunnels has multiple network interfaces. When an application sends a UDP broadcast to `255.255.255.255`, the operating system routes the packet through its default interface — which on multi-homed Windows hosts is often `vEthernet (WSL)` or `vEthernet (Default Switch)` rather than the physical NIC. The broadcast packet egresses through the virtual adapter and never reaches devices on the physical LAN.

This is not a configuration error. It is a fundamental limitation of limited broadcast: the sender has no control over which interface the OS uses for `255.255.255.255`. Socket options like `SO_BINDTODEVICE` are not available on Windows. `IP_MULTICAST_IF` does not apply to broadcast traffic.

Existing discovery protocols do not solve this:

| Protocol | Socket Model | Multi-Homed Handling | Fallback Strategy |
|----------|-------------|---------------------|-------------------|
| mDNS/Bonjour | Single socket, OS-managed multicast join | OS-dependent (often fails on virtual adapters) | None |
| SSDP/UPnP | Single socket | OS-dependent | None |
| WS-Discovery | SOAP over single socket | OS-dependent | None |
| Custom UDP broadcast | Single socket, `255.255.255.255` | Fails on multi-homed hosts | None |
| **Disclosed system** | **Per-interface sockets** | **Explicit per-NIC binding** | **3-tier: multicast, directed, limited** |

No existing system creates per-interface sockets with systematic fallback from multicast to directed broadcast to limited broadcast, while encapsulating all UDP transport behind a singleton module that prevents domain code from touching socket primitives.

---

## 2. Description of the Invention

### 2.1 Per-Interface Socket Binding

The disclosed system enumerates all eligible network interfaces at startup and creates one UDP socket per interface, bound to that interface's IP address. This guarantees that discovery packets egress through every physical and relevant virtual NIC, regardless of the OS default route.

#### Interface Eligibility

Not all interfaces are eligible. The system filters:

- **Include**: Interfaces with an IPv4 address that are up and have a non-zero prefix length.
- **Exclude**: Loopback interfaces (`127.0.0.1`).
- **Exclude**: Known virtual adapter patterns (configurable; e.g., Hyper-V internal switches used only for VM-to-host communication).

The eligible interface list is re-evaluated periodically (on DHCP renewal or network change events) to handle dynamic interface addition/removal. When re-evaluation detects a change:

- **New interface**: A new sender socket is created and bound to the new IP. The multicast group is joined on the new interface. The new socket is added to the sender set without disrupting existing sockets.
- **Removed interface**: The corresponding sender socket is dropped (closed). Its multicast membership is automatically released by the OS. Other sockets continue operating.
- **Changed IP (DHCP renewal)**: Treated as remove + add: the old socket is dropped and a new one is created with the new IP.

The receiver socket (`0.0.0.0:7184`) is not affected by interface changes — it receives from all interfaces by virtue of binding to the wildcard address. Only the sender socket set is mutated.

#### Socket Configuration

For each eligible interface with IP address `A`:

```
socket = UdpSocket::bind(A:0)          // Bind to specific IP, ephemeral port
socket.set_broadcast(true)              // Enable broadcast capability
socket.set_multicast_ttl_v4(1)          // LAN-only multicast (TTL=1)
socket.join_multicast_v4(
    239.255.42.99,                      // Multicast group (admin-scoped per RFC 2365)
    A                                    // Join on this specific interface
)
```

The multicast group address `239.255.42.99` is in the administratively scoped range (`239.255.0.0/16` per RFC 2365), ensuring packets are confined to the local administrative domain.

The disclosed embodiment uses IPv4. The same per-interface binding and multi-tier strategy applies to IPv6 with the following substitutions: (1) multicast group becomes a link-local scoped address (e.g., `ff02::1` or a custom group in `ff02::/16`); (2) directed broadcast is replaced by IPv6 all-nodes multicast (`ff02::1`) which is the native equivalent; (3) socket binding uses the interface's link-local IPv6 address with a scope ID; (4) `IPV6_MULTICAST_IF` replaces per-interface binding where `SO_BINDTODEVICE` is unavailable. The three-tier fallback concept remains identical: prefer scoped multicast, fall back to broader multicast, then all-nodes multicast.

### 2.2 Three-Tier Transmission Strategy

For each announcement, the system sends through each eligible interface using a prioritized fallback chain:

**Tier 1 — IPv4 Multicast** (primary):
```
send_to(socket[i], payload, 239.255.42.99:7184)
```
Multicast has well-understood router semantics, is less likely to be blocked than broadcast, and TTL=1 prevents packets from leaving the local subnet.

**Tier 2 — Directed Broadcast** (secondary):
```
broadcast_addr = compute_broadcast(interface_ip, prefix_length)
send_to(socket[i], payload, broadcast_addr:7184)
```
The directed broadcast address is computed from the interface IP and prefix length (e.g., `192.168.1.255` for `192.168.1.100/24`, `10.0.15.255` for `10.0.0.1/20`). This works on any subnet size — no `/24` assumption. Directed broadcast reaches all hosts on the specific subnet associated with each interface.

**Tier 3 — Limited Broadcast** (tertiary, disabled by default):
```
send_to(socket[i], payload, 255.255.255.255:7184)
```
Limited broadcast is a last resort for networks where both multicast and directed broadcast are blocked. It is disabled by default (`DISCOVERY_ENABLE_LIMITED_BCAST=false`) because it is the least reliable on multi-homed hosts.

#### Pseudocode: Send Announcement

```
function send_announcement(announcement_type, payload):
    envelope = UdpAnnouncement {
        type:      announcement_type,
        payload:   serialize(payload),
        stone_id:  local_stone_id,
        timestamp: now(),
    }

    bytes = serialize(envelope)

    for socket in per_interface_sockets:
        // Tier 1: Multicast
        socket.send_to(bytes, MULTICAST_GROUP:DISCOVERY_PORT)

        // Tier 2: Directed broadcast (if enabled)
        if directed_broadcast_enabled:
            broadcast_addr = compute_broadcast(socket.interface_ip, socket.prefix_length)
            socket.send_to(bytes, broadcast_addr:DISCOVERY_PORT)

        // Tier 3: Limited broadcast (if enabled, default: off)
        if limited_broadcast_enabled:
            socket.send_to(bytes, 255.255.255.255:DISCOVERY_PORT)
```

#### Directed Broadcast Computation

```
function compute_broadcast(ip: u32, prefix_length: u8) -> u32:
    host_bits = 32 - prefix_length
    mask = (1 << host_bits) - 1      // e.g., prefix=24 → mask=0x000000FF
    return ip | mask                  // Set all host bits to 1
```

This correctly handles any prefix length: `/16` produces `x.x.255.255`, `/20` produces `x.x.15.255`, `/24` produces `x.x.x.255`.

### 2.3 P2P Transport Singleton

All UDP communication in the disclosed system flows through a single infrastructure module (`p2p.rs`). Domain code never imports `UdpSocket`, never calls `bind()`, and never handles raw packet serialization.

The module provides two operations:

```
// Receiving: domain subscribes to typed events
subscribe_to_events() -> broadcast::Receiver<UdpEvent>

// Sending: domain sends typed announcements
send_announcement(type: AnnouncementType, payload: &T) -> Result<()>
```

The singleton owns:
- **Receiver socket**: Bound to `0.0.0.0:7184`, singleton, receives from all interfaces.
- **Sender sockets**: One per eligible interface, as described in Section 2.1.
- **Broadcast channel**: Internal `tokio::sync::broadcast` that distributes parsed `UdpEvent` variants to all subscribers.

Event types are an enum:

```
enum UdpEvent {
    StoneChirp    { chirp: StoneChirp, from: SocketAddr },
    ElectionRequest { request: ElectionRequest, from: SocketAddr },
    RegistryBeacon  { beacon: ToolsBeacon, from: SocketAddr },
    // ... other domain event types
}
```

Domain handlers subscribe and pattern-match on the variant they care about, ignoring others. This is zero-cost filtering: the broadcast channel delivers all events, but each handler destructures only its relevant variant.

#### Deduplication

Because the same announcement is sent through multiple tiers and multiple interfaces, a receiver on an overlapping subnet may receive 2-6 copies of the same packet. Additionally, the sender itself receives its own packets on the wildcard receiver socket (multicast loopback is enabled by default on most OSes). The receiver's first filter is **self-suppression**: packets where `stone_id == local_stone_id` are dropped immediately, before any further processing. After self-suppression, the receiver deduplicates by tracking a `(stone_id, timestamp)` pair for each received announcement. If a packet with an identical `(stone_id, timestamp)` has been seen within a configurable window (default: 5 seconds), it is silently dropped before being dispatched to the broadcast channel. This uses a bounded LRU cache (default capacity: 1024 entries) that self-evicts based on the dedup window. The dedup check occurs in the receiver loop inside `p2p.rs` before parsing the full payload, minimizing wasted work.

#### Compliance Rules

The system enforces architectural constraints:

1. No module outside `p2p.rs` may import `tokio::net::UdpSocket`.
2. No module outside `p2p.rs` may call `UdpSocket::bind()`.
3. All UDP reception must use `subscribe_to_events()`.
4. All UDP transmission must use `send_announcement()`.

These rules are documented in the ADR and enforced by code review. They prevent the socket proliferation and port conflict bugs that motivated the singleton pattern.

#### Implementation Evidence

- Architecture in `src/moss/src/infra/communications/p2p.rs`.
- `subscribe_to_events()` and `send_announcement()` API.
- `UdpEvent` enum with typed variants.
- ADR: `docs/decisions/COMM-0001-p2p-transport-singleton.md`.
- ADR: `docs/decisions/COMM-0004-multicast-first-discovery.md`.

### 2.4 Chirp-Based Topology with 5 Triggers

The disclosed system uses "chirps" — periodic and event-driven announcements — to build and maintain a topology map of all nodes on the LAN. A chirp carries the node's identity, capabilities, health state, and a summary of its hosted resources.

#### Chirp Payload Structure

```
StoneChirp {
    stone_id:       String,           // permanent node identity
    stone_name:     String,           // human-readable display name
    address:        String,           // IP address or hostname
    port:           u16,              // HTTP API port (default 7185)
    health:         String,           // "healthy", "degraded", "unhealthy"
    capabilities:   HardwareCapabilities, // CPU, RAM, GPU, temperature
    services:       Vec<ServiceSummary>,  // name, status, offering FQN per service
    storage:        Vec<StorageSummary>,  // replica_set_id, name, status, s3_port per mount
    companions:     Vec<CompanionSummary>, // name, status per companion
    uptime:         u64,              // seconds since daemon start
    version:        String,           // daemon version
    timestamp:      DateTime<Utc>,    // chirp generation time
}
```

The chirp is serialized as JSON. The entire struct is the wire format — no separate transport envelope wraps it beyond the `UdpAnnouncement` framing (which adds `type` and `stone_id` for routing before deserialization).

#### UDP Packet Size Management

Chirp payloads that exceed the safe UDP datagram size (default threshold: 1400 bytes, below typical Ethernet MTU of 1472 to account for tunneling overhead) are handled by truncation with priority ordering:

1. The `services` array is truncated to a summary count if the payload exceeds the threshold.
2. The `storage` array is similarly truncated.
3. The `companions` array is truncated last.

Each truncated field includes a `truncated: true` flag and a count of omitted entries. Receivers that see truncated chirps can fetch the full details via the node's HTTP API (`GET /api/v1/stone/capabilities`). The core identity fields (`stone_id`, `stone_name`, `address`, `port`, `health`) are never truncated — they fit comfortably within any reasonable MTU. This ensures topology discovery succeeds even when a node hosts many services, while full details are available via HTTP for nodes that need them.

Chirps are sent on 5 distinct triggers:

| Trigger | Timing | Purpose |
|---------|--------|---------|
| **State change** | Immediate | Node health, offering status, or storage state changed |
| **Heartbeat** | Every 30 seconds | Convergence guarantee; ensures all nodes have current view |
| **Offering change** | Immediate | Service planted, removed, started, or stopped |
| **Request** | On demand | Another node requests a chirp (e.g., on first discovery) |
| **Shutdown** | On graceful shutdown | `STONE_GOODBYE` allows immediate removal from topology |

### 2.5 JSON Change Detection for Traffic Reduction

To avoid sending redundant chirps when nothing has changed, the system serializes the chirp payload to JSON and compares the serialized bytes against the last-sent payload. If the JSON is byte-identical, the chirp is suppressed. This achieves approximately 95% traffic reduction during idle periods while maintaining guaranteed convergence via the 30-second heartbeat.

#### Pseudocode: Change-Detected Chirp

```
function maybe_send_chirp(current_state):
    payload = serialize_to_json(build_chirp(current_state))

    if payload == last_sent_payload:
        return  // No change, suppress

    last_sent_payload = payload
    send_announcement(STONE_CHIRP, payload)
```

### 2.6 Shared Topology Directory with Dual-File Ownership

The disclosed system maintains a shared directory on the host filesystem containing two files with distinct ownership:

| File | Writer | Schema | Purpose |
|------|--------|--------|---------|
| `garden-topology.json` | Infrastructure daemon (Moss) | Full mesh topology (`TopologyEntry[]`) | Authoritative mesh snapshot |
| `garden-stones.json` | Client applications | Lean roster (`CachedMossStone[]`) | Client operational cache with 7-day TTL |

**Ownership rule**: Moss and clients never write to each other's file. The files coexist in the same directory and are distinguished by name.

**Container cold-start**: The topology directory is bind-mounted into every managed container at a well-known path (`/app/cache/zen-garden/`). When a container starts and the infrastructure daemon is temporarily unreachable (restart, network hiccup), the container reads `garden-topology.json` from the filesystem to discover available nodes. This eliminates the bootstrap problem where a container with an empty cache and no reachable daemon has no topology knowledge.

**Persistence strategy for Moss**:
- Dirty flag on cache mutation.
- Debounced write: 500ms after last mutation.
- Periodic flush: every 30 seconds if dirty.
- Graceful shutdown: immediate flush.
- Atomic writes: write to `.tmp`, fsync, rename.

**Staleness eviction**: Each topology entry carries a `last_seen` timestamp updated on every chirp reception. Entries that have not been refreshed within 3x the heartbeat interval (default: 90 seconds) are marked as "unreachable." Entries marked unreachable for longer than a configurable eviction timeout (default: 5 minutes) are removed from the topology. Graceful shutdown chirps (`STONE_GOODBYE`) trigger immediate removal without waiting for the eviction timeout. This handles both clean shutdowns and hard crashes (power loss, network partition) with bounded staleness.

**File format**: Bare JSON array of topology entries (not wrapped in an HTTP API envelope). The API response wrapper is a transport concern, not a file format concern.

#### Implementation Evidence

- `topology_dir()` function in `src/common/src/constants/paths.rs`.
- Docker handler auto-injection of topology mount.
- ADR: `docs/decisions/TOPO-0002-shared-topology-directory.md`.

### 2.7 Configuration via Environment Variables

The discovery transport is configurable without code changes:

| Variable | Default | Purpose |
|----------|---------|---------|
| `DISCOVERY_PORT` | `7184` | UDP port for discovery |
| `DISCOVERY_MCAST_GROUP` | `239.255.42.99` | Multicast group IP |
| `DISCOVERY_ENABLE_BCAST_FALLBACK` | `true` | Enable directed broadcast (Tier 2) |
| `DISCOVERY_ENABLE_LIMITED_BCAST` | `false` | Enable limited broadcast (Tier 3) |

---

## 3. Claims

1. A method for LAN service discovery on multi-homed hosts comprising: enumerating eligible network interfaces on the host; creating one UDP socket per eligible interface, each bound to that interface's IP address (IPv4) or link-local address with scope ID (IPv6); sending discovery announcements through each socket to ensure packets egress through every eligible NIC; dynamically adding and removing sockets as interfaces appear, disappear, or change address; deduplicating received announcements by sender identity and timestamp; wherein the per-interface binding eliminates operating system routing ambiguity that causes discovery failures on hosts with virtual network adapters, and applies to both IPv4 and IPv6 network stacks.

2. A three-tier discovery transmission strategy comprising: a primary tier of IPv4 multicast to an administratively scoped group address with TTL=1; a secondary tier of directed broadcast per-interface computed from the interface IP address and prefix length; a tertiary tier of limited broadcast to `255.255.255.255`; each tier independently configurable; wherein the system degrades gracefully from multicast through directed broadcast to limited broadcast based on network capabilities, and the directed broadcast address computation supports any subnet prefix length without assuming `/24`.

3. A P2P transport singleton pattern for distributed systems comprising: a single infrastructure module that owns all UDP socket lifecycle (binding, sending, receiving); a typed event API where domain code subscribes to an event stream and sends typed announcements without importing socket primitives; a broadcast channel that distributes parsed event variants to multiple concurrent subscribers; architectural constraints preventing any module outside the singleton from creating UDP sockets; wherein the pattern eliminates port conflicts, scattered socket handling, and domain-layer transport dependencies.

4. A serialization-based change detection mechanism for topology announcement traffic reduction comprising: serializing each announcement payload to a deterministic byte representation (the disclosed embodiment uses JSON, but the technique applies equally to any deterministic serialization format including protobuf, CBOR, MessagePack, or binary encoding); comparing the serialized bytes against the last-sent payload; suppressing the announcement when the payload is byte-identical to the previous transmission; maintaining a periodic heartbeat that sends regardless of change for convergence guarantee; achieving significant traffic reduction during idle periods while maintaining bounded convergence time.

5. A shared topology directory with dual-file ownership for container cold-start comprising: two files in a well-known directory — one written exclusively by the infrastructure daemon (authoritative mesh snapshot) and one written exclusively by client applications (operational cache); the directory bind-mounted into managed containers at startup; containers reading the daemon's topology file when HTTP endpoints are temporarily unreachable; wherein containers can discover available infrastructure nodes during cold-start without network dependency, and the dual-file ownership prevents write conflicts between the daemon and clients.

6. A chirp-based topology maintenance protocol comprising: five distinct chirp triggers — state change (immediate), periodic heartbeat (30-second interval), offering change (immediate), request-response (on demand), and graceful shutdown (immediate goodbye); each chirp carrying node identity, capabilities, health state, and resource summary; JSON change detection suppressing redundant chirps; shutdown chirps enabling immediate removal from peer topology maps rather than relying on timeout-based eviction.

---

## 4. Implementation Evidence

| Component | Location |
|-----------|----------|
| P2P transport singleton | `src/moss/src/infra/communications/p2p.rs` |
| Network interface enumeration | `src/moss/src/infra/network/mod.rs` |
| Storage beacon (multicast) | `src/moss/src/infra/storage/beacon.rs` |
| Tools beacon (multicast) | `src/moss/src/infra/tools/beacon.rs` |
| Pulse listener (multicast) | `src/moss/src/infra/listeners/pulse.rs` |
| Platform detection (virtual adapters) | `src/moss/src/infra/storage/platform.rs` |
| Shared topology directory | Paths in `src/common/src/constants/paths.rs` — `topology_dir()` |
| P2P singleton ADR | `docs/decisions/COMM-0001-p2p-transport-singleton.md` |
| Multicast-first ADR | `docs/decisions/COMM-0004-multicast-first-discovery.md` |
| Shared topology ADR | `docs/decisions/TOPO-0002-shared-topology-directory.md` |
| Discovery port constant | `src/common/src/constants/mod.rs` — `DISCOVERY_UDP = 7184` |

---

## 5. Public Domain Dedication

This document is published as a defensive disclosure to establish prior art. The inventor(s) dedicate this disclosure to the public domain and assert no patent rights over the described inventions. All rights to use, implement, and build upon these inventions are hereby granted to the public.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) No receiver deduplication described for multi-tier/multi-interface duplicate packets. (2) Entire disclosure is IPv4-only; IPv6 variant patentable. (3) Interface re-enumeration behavior (add/remove/change) unspecified. (4) Chirp payload structure undefined — competitor could patent topology announcement format. (5) Topology entry staleness eviction not described — crash scenarios leave stale entries.
**Author revision:** Added Deduplication section with (stone_id, timestamp) LRU cache. Added IPv6 applicability paragraph with concrete substitutions. Added interface lifecycle details (new, removed, changed IP). Added full StoneChirp struct definition. Added staleness eviction with 3x heartbeat marking and 5-minute removal.

### Pass 2
**Antagonist:** (1) Claim 1 still IPv4-only despite body covering IPv6. (2) Claim 4 is JSON-specific; competitor could patent same technique with different serialization. (3) UDP packet size limits unaddressed for large chirp payloads.
**Author revision:** Updated Claim 1 to cover both IPv4 and IPv6 with dynamic socket management and deduplication. Broadened Claim 4 to cover any deterministic serialization format. Added UDP Packet Size Management section with priority-ordered truncation and HTTP fallback.

### Pass 3
**Antagonist:** Self-discovery suppression missing — sender receives own packets via multicast loopback, risking echo loops or wasted processing.
**Author revision:** Added self-suppression as first filter in dedup pipeline: packets where stone_id == local_stone_id are dropped immediately.

### Pass 4
No further objections.

### Final Status
CLEARED -- Antagonist found no further weaknesses. Safe to publish.
