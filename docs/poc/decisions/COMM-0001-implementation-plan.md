# COMM-0001 Implementation Plan

**Decision Reference**: [COMM-0001-p2p-transport-singleton.md](./COMM-0001-p2p-transport-singleton.md)  
**Status**: Ready for Implementation  
**Priority**: CRITICAL (blocks election feature deployment - causing crash loops)

---

## Violation Audit Results

### Files with `UdpSocket::bind()` (MUST migrate)

1. ❌ **src/moss/src/tasks/election_service.rs**
   - **Severity**: CRITICAL (causing production crash loop)
   - **Current**: Binds to 7184 (conflicts with discovery listener)
   - **Action**: Remove socket, subscribe to `p2p::subscribe_to_events()`

2. ❌ **src/moss/src/announcement.rs**
   - **Severity**: HIGH
   - **Current**: Creates ephemeral socket on every send (lines 110, 162)
   - **Action**: Replace with `p2p::send_announcement()`

3. ❌ **src/moss/src/discovery.rs**
   - **Severity**: HIGH
   - **Current**: Mixed transport + domain logic, creates send socket (line 401)
   - **Action**: Split into `p2p.rs` (transport) + `discovery_handler.rs` (logic)

4. ⚠️ **src/moss/src/infra/network.rs**
   - **Severity**: MEDIUM
   - **Current**: Line 269 creates ephemeral socket for health checks
   - **Action**: Audit - may be non-P2P UDP (keep if not broadcast)

5. ✅ **src/moss/src/network_singletons.rs**
   - **Status**: OK (helper function, not direct usage)
   - **Action**: None

---

## Implementation Steps

### Phase 1: Create P2P Transport Layer ⏱️ 2 hours

**Files to Create:**
```
src/moss/src/infra/communications/
├── mod.rs                 # Module exports
├── p2p.rs                 # UDP singleton (NEW)
└── mdns.rs                # Moved from tasks/mdns_lurk.rs
```

**Tasks:**
1. Create `infra/communications/mod.rs`
   ```rust
   pub mod p2p;
   pub mod mdns;
   
   pub use p2p::{subscribe_to_events, send_announcement, UdpEvent};
   ```

2. Create `infra/communications/p2p.rs`
   - Extract UDP listener from `discovery.rs::ensure_udp_listener()`
   - Extract UdpEvent enum from `discovery.rs`
   - Add `send_announcement()` helper with sender socket singleton
   - Add comprehensive tracing

3. Move `tasks/mdns_lurk.rs` → `infra/communications/mdns.rs`
   - Update imports
   - Update `tasks/mod.rs`

4. Update `infra/mod.rs` to export `communications` module

**Validation:**
```bash
cargo check --package garden-moss
# Should compile with new module structure
```

---

### Phase 2: Refactor Announcement Module ⏱️ 30 minutes

**File to Modify:**
- `src/moss/src/announcement.rs`

**Changes:**
```rust
// BEFORE (lines 108-122):
async fn send_udp_announcement(entry: &TopologyEntry) -> Result<()> {
    use tokio::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;
    let announcement = UdpAnnouncement { ... };
    socket.send_to(&data, &broadcast_addr).await?;
}

// AFTER:
async fn send_udp_announcement(entry: &TopologyEntry) -> Result<()> {
    use crate::infra::communications::p2p;
    p2p::send_announcement(
        announcement_types::STONE_CHIRP,
        entry
    ).await
}
```

**Similar changes for:**
- `send_goodbye_announcement()` (line 162)

**Validation:**
```bash
cargo check --package garden-moss
grep "UdpSocket::bind" src/moss/src/announcement.rs
# Should return no results
```

---

### Phase 3: Refactor Discovery Module ⏱️ 1 hour

**Files to Create/Modify:**
- NEW: `src/moss/src/tasks/discovery_handler.rs`
- MODIFY: `src/moss/src/discovery.rs` (remove transport, keep helpers)
- MODIFY: `src/moss/src/tasks/coordinator.rs`

**Tasks:**

1. Extract domain logic from `discovery.rs` → `tasks/discovery_handler.rs`
   ```rust
   // NEW FILE: tasks/discovery_handler.rs
   pub async fn start_discovery_handler(
       topology_cache: TopologyCache,
       self_entry: Arc<RwLock<TopologyEntry>>,
   ) {
       let mut udp_rx = p2p::subscribe_to_events().await.unwrap();
       tokio::spawn(async move {
           loop {
               match udp_rx.recv().await {
                   Ok(UdpEvent::Request { request, .. }) => {
                       // Respond with chirp
                       let entry = self_entry.read().await.clone();
                       p2p::send_announcement(
                           announcement_types::STONE_CHIRP,
                           &entry
                       ).await.ok();
                   }
                   Ok(UdpEvent::Chirp { chirp, .. }) => {
                       upsert_from_chirp(&topology_cache, chirp).await;
                   }
                   Ok(UdpEvent::Goodbye { goodbye, .. }) => {
                       mark_stone_offline(&topology_cache, &goodbye.stone_id).await;
                   }
                   _ => {}
               }
           }
       });
   }
   ```

2. Keep helpers in `discovery.rs`:
   - `upsert_from_chirp()`
   - `mark_stone_offline()`
   - `send_discovery_request()` (migrate to use `p2p::send_announcement()`)

3. Update `tasks/coordinator.rs::start_discovery_listener()`
   - Replace with call to `discovery_handler::start_discovery_handler()`

**Validation:**
```bash
cargo check --package garden-moss
grep "ensure_udp_listener" src/moss/src/tasks/coordinator.rs
# Should return no results
```

---

### Phase 4: Refactor Election Service ⏱️ 1 hour

**File to Modify:**
- `src/moss/src/tasks/election_service.rs`

**Changes:**

1. Remove socket field from `ElectionService` struct
2. Remove socket binding from `new()` (make it non-async)
3. Refactor `run_listener()`:
   ```rust
   pub async fn run_listener(
       self: Arc<Self>,
       mut udp_rx: tokio::sync::broadcast::Receiver<UdpEvent>
   ) -> Result<()> {
       loop {
           match udp_rx.recv().await {
               Ok(UdpEvent::ElectionRequest { request, .. }) => {
                   self.handle_election_request(&request).await;
               }
               Ok(UdpEvent::ElectionResult { result, .. }) => {
                   self.handle_election_result(&result).await;
               }
               _ => {}
           }
       }
   }
   ```

4. Update send methods to use `p2p::send_announcement()`:
   ```rust
   async fn send_candidacy(&self, candidate: &CandidateAnnouncement) -> Result<()> {
       p2p::send_announcement(
           announcement_types::ELECTION_CANDIDATE,
           candidate
       ).await
   }
   ```

**Validation:**
```bash
cargo check --package garden-moss
grep "UdpSocket" src/moss/src/tasks/election_service.rs
# Should return no results
```

---

### Phase 5: Update Bootstrap ⏱️ 1 hour

**File to Modify:**
- `src/moss/src/bootstrap/run.rs`

**Changes:**

1. **Phase 1** - Initialize P2P transport early:
   ```rust
   // Phase 1: Start UDP listener EARLY
   tracing::info!("Initializing P2P transport layer");
   let udp_rx = crate::infra::communications::p2p::subscribe_to_events().await?;
   ```

2. **Phase 11.pre** - Remove election socket binding:
   ```rust
   // BEFORE:
   let election_service = ElectionService::new(
       stone_id.clone(),
       stone_name.clone(),
       7184, // ❌ REMOVE
       Box::new(PlaceholderStateProvider),
   ).await.expect("Failed..."); // ❌ REMOVE expect
   
   // AFTER:
   let election_service = ElectionService::new(
       stone_id.clone(),
       stone_name.clone(),
       Box::new(PlaceholderStateProvider),
   );
   ```

3. **Phase 11.post2** - Pass UDP receiver to election service:
   ```rust
   let election_udp_rx = p2p::subscribe_to_events().await?;
   tokio::spawn(async move {
       if let Err(e) = election_service_final.run_listener(election_udp_rx).await {
           tracing::error!(error = ?e, "Election service listener failed");
       }
   });
   ```

4. **Phase 11** - Replace discovery listener with handler:
   ```rust
   // BEFORE:
   start_discovery_listener(stone_id, stone_name, endpoint, topology_cache, ...);
   
   // AFTER:
   crate::tasks::discovery_handler::start_discovery_handler(
       topology_cache.clone(),
       self_entry.clone(),
   ).await;
   ```

**Validation:**
```bash
cargo check --package garden-moss
cargo build --package garden-moss --release
# Should succeed with no port binding conflicts
```

---

### Phase 6: Audit Network Module ⏱️ 30 minutes

**File to Audit:**
- `src/moss/src/infra/network.rs` (line 269)

**Questions:**
- Is this UDP for P2P broadcast, or point-to-point health checks?
- If P2P: Migrate to `p2p::send_announcement()`
- If P2P unicast: Keep as-is (not broadcast traffic)

**Decision Required Before Migration**

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_send_announcement_singleton() {
        // Verify socket reuse
        let result1 = send_announcement("test", &json!({})).await;
        let result2 = send_announcement("test", &json!({})).await;
        assert!(result1.is_ok() && result2.is_ok());
    }
    
    #[tokio::test]
    async fn test_subscribe_multiple_listeners() {
        let mut rx1 = subscribe_to_events().await.unwrap();
        let mut rx2 = subscribe_to_events().await.unwrap();
        // Both should receive same events
    }
}
```

### Integration Tests
```bash
# Deploy to stone-bronze-canyon
./installer/push-ssh-direct.ps1 stone-bronze-canyon

# SSH and verify
plink -batch -ssh "stone@stone-bronze-canyon" -pw stone "systemctl status garden-moss"
# Should start successfully (no crash loop)

plink -batch -ssh "stone@stone-bronze-canyon" -pw stone "sudo journalctl -u garden-moss -n 50"
# Should see "P2P transport layer initialized"
# Should NOT see "Failed to bind UDP socket" or panic traces
```

### Validation Commands
```bash
# Verify no UdpSocket in domain/tasks (except allowed modules)
rg "use tokio::net::UdpSocket" src/moss/src/tasks/ --glob '!election_service.rs'
rg "use tokio::net::UdpSocket" src/moss/src/domain/

# Verify no socket binding outside p2p.rs
rg "UdpSocket::bind" src/moss/src/ --glob '!**/p2p.rs' --glob '!**/network_singletons.rs'

# Verify all sends use p2p helper
rg "socket.send_to\(" src/moss/src/ --glob '!**/p2p.rs'
```

---

## Rollback Plan

If issues occur:
1. Revert commits: `git revert <commit-range>`
2. Redeploy previous version: `./installer/build.ps1; ./installer/deploy.ps1`
3. Document failure mode in COMM-0001 decision doc

---

## Success Criteria

- [x] Decision documented (COMM-0001)
- [ ] All UDP handling centralized in `infra/communications/p2p.rs`
- [ ] No `UdpSocket::bind()` outside `p2p.rs` and `network_singletons.rs`
- [ ] No domain modules import `tokio::net::UdpSocket`
- [ ] Election service starts without crash loop
- [ ] Discovery continues working (chirps, topology updates)
- [ ] All stones reachable via `garden-rake observe`
- [ ] Election command works: `garden-rake election start --election-type update_source`

---

## Timeline

**Total Estimated Time**: 6 hours  
**Recommended Approach**: Incremental phases with validation  
**Risk**: Medium (touches critical discovery path)

**Phase Order Priority**:
1. Phase 1 (P2P layer) - Foundation
2. Phase 4 (Election) - CRITICAL (fixes crash loop)
3. Phase 5 (Bootstrap) - Wire everything together
4. Phase 2 (Announcement) - Cleanup
5. Phase 3 (Discovery) - Final migration
6. Phase 6 (Network audit) - Optional cleanup
