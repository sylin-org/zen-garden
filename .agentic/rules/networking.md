---
globs: src/moss/src/infra/communications/**/*.rs, src/moss/src/tasks/**/*.rs
alwaysApply: false
---
# P2P Transport & Discovery

## P2P Transport Singleton (CRITICAL)
ALL UDP communication MUST go through `infra/communications/p2p.rs`.

### Rules
- ❌ NEVER import `tokio::net::UdpSocket` in domain/tasks modules
- ❌ NEVER call `UdpSocket::bind()` anywhere except `p2p.rs`
- ✅ ALWAYS use `p2p::subscribe_to_events()` for receiving
- ✅ ALWAYS use `p2p::send_announcement(type, payload)` for sending

### Pattern
```rust
// Receiving (in tasks/handlers)
let mut udp_rx = p2p::subscribe_to_events().await?;
loop {
    match udp_rx.recv().await {
        Ok(UdpEvent::ElectionRequest { request, .. }) => handle(request),
        Ok(UdpEvent::StoneChirp { chirp, .. }) => handle(chirp),
        _ => {}
    }
}

// Sending (from anywhere)
p2p::send_announcement(
    announcement_types::ELECTION_REQUEST,
    &election_request
).await?;
```

## Discovery Transport (Multicast-First)

### Default Configuration
- **Multicast group**: `239.255.42.99`
- **Port**: `7184`
- **TTL**: `1` (LAN-only)

### Environment Variables
- `DISCOVERY_PORT`: UDP port (default: 7184)
- `DISCOVERY_MCAST_GROUP`: Multicast group IP
- `DISCOVERY_ENABLE_BCAST_FALLBACK`: Enable directed broadcast (default: true)

## Reference
Decision: `docs/decisions/COMM-0001-p2p-transport-singleton.md`
