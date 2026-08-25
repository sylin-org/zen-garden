# COMM-0001: P2P Transport Singleton Pattern

**Status**: Accepted  
**Date**: 2026-01-25  
**Deciders**: Architecture Team  
**Related**: STATE-0001 (Stateless Moss), MDNS-0001 (mDNS Discovery)

---

## Context

Multiple modules (discovery, election, announcement) were creating their own UDP sockets, leading to:
- Port conflicts (multiple binds to 7184)
- Scattered UDP handling across codebase
- Domain-layer dependencies on transport (violates SoC/DDD)
- Difficult testing (mocking UDP in every module)
- Inconsistent socket options (SO_REUSEADDR, broadcast flags)

## Decision

**Create a centralized P2P communications layer that manages UDP lifecycle as a singleton.**

### Architecture

```
infra/communications/p2p.rs (INFRASTRUCTURE - PURE TRANSPORT):
├── UDP Receiver Socket (bound to 7184, singleton)
├── UDP Sender Socket (ephemeral port, singleton)
├── subscribe_to_events() → Receiver<UdpEvent>
└── send_announcement(type, payload) → Result<()>

tasks/*_handler.rs (DOMAIN - PURE LOGIC):
├── Election handler: receives events, sends announcements
├── Discovery handler: receives events, sends announcements
└── NO imports of tokio::net::UdpSocket
```

### API Contract

#### Receiving Events
```rust
// Domain subscribes to UDP events (filtered by type)
let mut events = p2p::subscribe_to_events().await?;
loop {
    match events.recv().await {
        Ok(UdpEvent::ElectionRequest { request, .. }) => handle(request),
        Ok(UdpEvent::StoneChirp { chirp, .. }) => handle(chirp),
        _ => {} // Ignore other types
    }
}
```

#### Sending Announcements
```rust
// Domain sends via simple helper (no socket visibility)
p2p::send_announcement(
    announcement_types::ELECTION_REQUEST,
    &election_request
).await?;
```

### Module Organization

```
src/moss/src/infra/communications/
├── mod.rs
├── p2p.rs       # UDP singleton (send/recv)
└── mdns.rs      # mDNS service discovery

src/moss/src/tasks/
├── discovery_handler.rs    # Domain logic for discovery
├── election_handler.rs     # Domain logic for elections
└── ceremony_handler.rs     # Domain logic for ceremonies
```

## Consequences

### Positive
- ✅ **Single Responsibility**: P2P owns transport, handlers own logic
- ✅ **DDD Compliant**: Infrastructure → Domain (correct dependency direction)
- ✅ **No Port Conflicts**: One receiver on 7184, reused sender socket
- ✅ **Testable**: Mock `p2p::send_announcement()` in tests
- ✅ **Discoverable**: "Where's UDP?" → "In infra/communications/p2p"
- ✅ **Extensible**: Add protocols (ceremony, backup) without touching transport
- ✅ **Consistent**: All UDP uses same socket options, tracing, error handling

### Negative
- ⚠️ **Migration Work**: Must refactor existing modules (see below)
- ⚠️ **Broadcast Channel Overhead**: All subscribers get all events (filtered in userspace)
  - *Acceptable*: UDP traffic is low volume, filtering is cheap

### Neutral
- ℹ️ **Two Sockets**: Receiver (7184) + Sender (ephemeral) instead of one
  - *Rationale*: Avoids contention, UDP sockets are cheap

## Compliance Requirements

**ALL modules using UDP MUST:**
1. ❌ **NOT** import `tokio::net::UdpSocket` in domain/tasks layer
2. ❌ **NOT** call `UdpSocket::bind()` anywhere except `p2p.rs`
3. ✅ **MUST** use `p2p::subscribe_to_events()` for receiving
4. ✅ **MUST** use `p2p::send_announcement()` for sending
5. ✅ **MUST** live in `tasks/` if domain logic, `infra/communications/` if transport

## Migration Checklist

### Modules Requiring Refactoring

- [ ] **src/moss/src/discovery.rs**
  - Current: Mixed UDP handling + discovery logic
  - Action: Split into `infra/communications/p2p.rs` + `tasks/discovery_handler.rs`
  
- [ ] **src/moss/src/announcement.rs**
  - Current: Creates ephemeral socket per send
  - Action: Replace with `p2p::send_announcement()`
  
- [ ] **src/moss/src/tasks/election_service.rs**
  - Current: Binds own UDP socket (causes crash loop)
  - Action: Subscribe to `p2p::subscribe_to_events()`, use `p2p::send_announcement()`
  
- [ ] **src/moss/src/tasks/coordinator.rs**
  - Current: Calls `discovery::ensure_udp_listener()`
  - Action: Call `p2p::subscribe_to_events()`, wire to discovery handler
  
- [ ] **src/moss/src/infra/network.rs**
  - Current: May have UDP sends for ping/health checks
  - Action: Audit and migrate to `p2p::send_announcement()` if applicable

### Bootstrap Changes

- [ ] **src/moss/src/bootstrap/run.rs**
  - Remove: `start_discovery_listener()` in Phase 1
  - Add: Initialize `p2p` transport singleton in Phase 1
  - Add: Wire `p2p` events → discovery handler in Phase 11
  - Add: Wire `p2p` events → election handler in Phase 11
  - Remove: Election service socket binding (Phase 11.pre)

## Validation

After refactor, verify:
```bash
# No UdpSocket imports in domain/tasks
rg "use tokio::net::UdpSocket" src/moss/src/tasks/
rg "use tokio::net::UdpSocket" src/moss/src/domain/

# No socket binding outside p2p.rs
rg "UdpSocket::bind" --glob '!**/p2p.rs' src/moss/

# All announcements go through helper
rg "send_to\(" --glob '!**/p2p.rs' src/moss/
```

## References

- ELECTION-0001: Distributed Election Protocol (specifies UDP on 7184)
- Clean Architecture: Infrastructure Companions calling domain
- Hexagonal Architecture: Ports & Companions pattern
