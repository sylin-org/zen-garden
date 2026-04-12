---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0018, ARCH-0020, ARCH-0023]
completed: 2026-04-12
---

# ARCH-0028: Discovery Aggregate — mDNS Registration, Koi Handle, and Network Monitor

**Date**: 2026-04-12
**Status**: Accepted
**Book**: X of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Discovery

## Context

ARCH-0017 Book X specifies: "Consolidate mDNS, UDP chirp, koi discovery,
and network interface monitoring into three distinct but co-located
bounded contexts: Discovery, Announcement, Networking."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (8 findings)

1. **`domain/discovery/mod.rs` is a 19-line struct** with two `pub`
   fields: `mdns: Option<Arc<MdnsHandle>>` and `koi: Arc<KoiHandle>`.
   No behavior, no events, no ports, no privacy. Just a bag of two
   handles.

2. **`domain/announcement.rs` is pure stateless functions** (125 lines).
   `should_announce()`, `announcement_name()`, `infer_service_type()` are
   decision helpers with no runtime state. There is nothing for an
   aggregate to own.

3. **`domain/network.rs` is pure value objects** (415 lines).
   `NetworkMode`, `StaticIpState`, `NetworkError`, `ProbeResult` — domain
   types for static IP management. No runtime state.

4. **`tasks/network_monitor.rs` holds the `Network` struct** (~335 lines)
   with IP polling, event broadcasting (`NetworkEvent`), and Subsystems
   integration. This is infrastructure — it polls for IP changes and
   emits events. It already feeds Subsystems cleanly through
   `mark_ready("network")` / `mark_unready("network")`.

5. **`mdns.rs` is an infrastructure adapter** (~457 lines). `MdnsHandle`
   wraps Koi's mDNS sub-handle for service registration/deregistration.
   `start_mdns_lurk_listener()` wraps Koi's browse API. Both are
   infrastructure, not domain logic.

6. **The Koi handle (`state.discovery.koi`) is misplaced.** Of 9 call
   sites, 7 access `.certmesh()` (Security concern: pond handlers,
   adoption vault, s3 presign). Only 1 uses `.mdns()` indirectly
   (coordinator startup check). 1 uses `.vault()` (adoption). The Koi
   handle is a multi-capability embedded service; its placement under
   `discovery` is a historical accident from when Koi was primarily a
   discovery library. It belongs on Security (its dominant consumer) or
   remains a shared infrastructure handle.

7. **Announcement has no state.** The periodic announcer task
   (`tasks/announcer.rs`) is a 30s timer that calls
   `topology.chirp()` — the chirp transport is already owned by Topology
   (Book III). The announcer task does not need an aggregate; it is a
   timer that reads from multiple domains and delegates to an existing
   aggregate's command.

8. **The `MdnsHandle` is consumed by 3 sites:**
   - `topology::composition::announce_resolution_change` — mDNS
     re-registration on IP change.
   - `tasks/task_defs/mdns_health_listener.rs` — mDNS TXT update on
     health transition.
   - `domain/security/pond_lifecycle.rs:246` — mDNS offering announce
     after enrollment.
   These are all "re-register the service" calls — not domain decisions.

### Plan change: 3 contexts → 1 aggregate + relocations

The original plan of 3 bounded contexts (Discovery, Announcement,
Networking) does not match the code:

- **Announcement** has no state. Chirp scheduling is a timer; chirp
  transport is owned by Topology. Pure functions stay as free functions.
  No aggregate needed.
- **Networking** is infrastructure, not a domain. `Network` monitor feeds
  Subsystems. `NetworkMode` / `StaticIpState` are value objects consumed
  by offerings. No aggregate needed.
- **Discovery** is a 19-line bag. The Koi handle serves Security more
  than Discovery.

**What Book X actually does:**

1. **Discovery aggregate**: encapsulates mDNS handle, Koi handle, and
   mDNS lurk-listener state. Typed commands for
   registration/deregistration. `MdnsTransport` port for testability.
   Exposes `koi()` accessor for cross-domain consumers (Security,
   Storage) until those consumers relocate the capability they need.
2. **Relocate `mdns.rs`** from `src/moss/src/` top-level into
   `domain/discovery/` as infrastructure adapter.
3. **Network monitor stays** in `tasks/network_monitor.rs` and
   `domain/platform/`. No aggregate — it is infrastructure that feeds
   Subsystems readiness.
4. **Announcement stays** as `domain/announcement.rs` free functions.
   Periodic announcer task is unchanged — it is a timer, not a domain.

## Decision

### Aggregate shape

```
domain/discovery/
├── mod.rs          — module root + re-exports
├── aggregate.rs    — Discovery aggregate root
├── event.rs        — DiscoveryChanged event
└── tests.rs        — unit tests
```

### State

```rust
pub struct Discovery {
    /// Koi embedded handle — mDNS, DNS, certmesh, vault capabilities.
    koi: Arc<KoiHandle>,
    /// mDNS registration handle (None if mDNS unavailable at startup).
    mdns: Option<Arc<MdnsHandle>>,
    /// Lurk-listener broadcast receiver source (mDNS browse).
    lurk_tx: Option<broadcast::Sender<DiscoveredStone>>,
    /// Metrics integration.
    metrics: Arc<Metrics>,
}
```

The aggregate is **ephemeral** (no persistence, no `Store` port). mDNS
state is volatile — registrations are re-created on every process start.

### Commands (write)

| Command | Effect |
|---------|--------|
| `reregister(ip, mac)` | Re-register mDNS `_moss._tcp` + `_http._tcp` |
| `update_health(health)` | Update mDNS TXT record with new health status |
| `register_certmesh(port)` | Register `_certmesh._tcp` service |

### Queries (read)

| Query | Returns |
|-------|---------|
| `koi()` | `&Arc<KoiHandle>` — shared Koi handle |
| `mdns_registered()` | `bool` — whether mDNS is currently registered |
| `lurk_stream()` | `broadcast::Receiver<DiscoveredStone>` |
| `changes()` | `broadcast::Receiver<DiscoveryChanged>` |

### Events

```rust
pub enum DiscoveryChangeKind {
    Registered,   // mDNS service registered or re-registered
    Unregistered, // mDNS service unregistered (shutdown)
    PeerDiscovered { stone_name: String },  // lurk-listener found a peer
}
```

### Ports

No new ports. The `MdnsHandle` is already an adapter wrapping Koi's
mDNS sub-handle. It moves into `domain/discovery/` as the concrete
implementation. A `MdnsTransport` trait is not warranted — Koi's embedded
handle is the only implementation and is unlikely to be swapped. Testing
uses the existing `Option<Arc<MdnsHandle>>` pattern (None = no mDNS).

### What stays unchanged

- `domain/announcement.rs` — pure functions, no state, no aggregate.
- `domain/network.rs` — value objects for static IP management.
- `tasks/network_monitor.rs` — infrastructure, feeds Subsystems.
- `tasks/announcer.rs` — timer task using Topology's chirp command.
- `tasks/discovery.rs` — UDP event listener, stays as is.
- `tasks/discovery_handler.rs` — UDP discovery request responder.

### Migration plan

1. Relocate `mdns.rs` into `domain/discovery/mdns.rs` (pure `git mv`).
2. Build aggregate with private state, typed commands, `changes()`.
3. Migrate `topology::composition::announce_resolution_change` mDNS call
   to `discovery.reregister()`.
4. Migrate `mdns_health_listener` to `discovery.update_health()`.
5. Migrate `pond_lifecycle.rs` certmesh mDNS call to
   `discovery.register_certmesh()`.
6. Migrate `state.discovery.koi` call sites to `state.discovery.koi()`.
7. Migrate `state.discovery.mdns` call sites to aggregate commands.
8. Make `Discovery` fields private.

### Pattern deviations

- **Ephemeral**: no persistence, no `Store` port (matches Books I, IV, VI).
- **Infallible mutations**: commands return `()` or `bool`; mDNS errors
  are logged and swallowed (registration failure is non-fatal).
- **No typed error enum**: mDNS operations are best-effort; errors are
  logged at the call site. Introducing `DiscoveryError` would add
  ceremony without benefit since no caller matches on error variants.

### Exit criteria

- `rg 'pub mdns:' src/moss/src/domain/discovery/` returns 0 matches
  (field is private).
- `rg 'pub koi:' src/moss/src/domain/discovery/` returns 0 matches
  (field is private).
- `rg 'state\.discovery\.mdns\b' src/moss/src/` returns 0 matches
  (all migrated to aggregate commands).
- `src/moss/src/mdns.rs` no longer exists (relocated).
- All existing tests pass + new Discovery unit tests.

## Consequences

- **Reduced**: `Discovery` goes from a public-field bag to a proper
  aggregate with encapsulated state and typed commands.
- **Preserved**: Network monitoring, announcement functions, and periodic
  announcer remain unchanged — they are correctly placed.
- **Deferred**: Koi handle relocation to Security is out of scope. The
  handle is multi-capability (mDNS + DNS + certmesh + vault) and
  genuinely serves multiple domains. It stays on Discovery with a typed
  accessor until a future book (XII or later) determines the right home.
- **Plan change**: ARCH-0017 planned 3 contexts; 1 aggregate is
  sufficient. The other 2 planned contexts (Announcement, Networking)
  are not warranted — their code is already correctly placed as free
  functions and infrastructure.
