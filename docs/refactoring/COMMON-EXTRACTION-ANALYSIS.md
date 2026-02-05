# Common Extraction Analysis

**Date**: 2026-01-25  
**Scope**: Identify components in moss that should be in common  
**Goal**: Proper SoC/DRY/DDD - Common contains shared infrastructure, moss contains moss-specific orchestration

## Guiding Principles

**Common should contain:**
- ✅ Pure communication infrastructure (UDP, mDNS, HTTP client)
- ✅ Shared data types/models used by multiple binaries
- ✅ Protocol implementations (discovery, election, announcement)
- ✅ Network utilities (IP detection, socket creation)
- ✅ Platform-agnostic business logic
- ✅ Contracts/interfaces between components

**Moss should contain:**
- ✅ Moss daemon-specific orchestration
- ✅ HTTP API handlers (Axum routes)
- ✅ State management (Arc<RwLock<AppState>>)
- ✅ Docker integration (Bollard wrapper)
- ✅ Bootstrap/initialization specific to moss
- ✅ Domain handlers that consume common infrastructure

---

## CRITICAL: Must Move to Common

### 1. Communication Infrastructure

#### `moss/src/infra/communications/p2p.rs` → `common/src/infra/communications/p2p.rs`

**Why**: Both moss AND rake need UDP transport with envelope format.

**Current State**:
- Located in moss
- Handles UdpAnnouncement envelope wrapping/unwrapping
- Provides subscribe_to_events() and send_announcement()
- Uses moss-specific TopologyEntry in UdpEvent enum

**Required Changes**:
- Remove TopologyEntry from UdpEvent (moss-specific type)
- Make UdpEvent generic or use serde_json::Value for payloads
- Move to common/src/infra/communications/
- Both moss and rake can then use it

**Benefits**:
- Rake can use same transport for discovery
- No duplicate UDP socket code
- Consistent envelope handling everywhere
- Single source of truth for protocol

**Blockers**:
- UdpEvent::Chirp uses moss's domain::TopologyEntry
- UdpEvent::Request/Response use moss domain types

**Solution**:
- Make UdpEvent carry raw serde_json::Value payloads
- Handlers deserialize to their specific types
- OR: Move TopologyEntry to common/types/

---

#### `moss/src/mdns.rs` → `common/src/infra/communications/mdns.rs`

**Why**: mDNS service discovery is needed by:
- Moss (announce self)
- Rake (discover lantern)
- Future: Lantern (announce self)

**Current State**:
- Platform-specific implementations (#[cfg(target_os)])
- MdnsHandle struct
- announce_moss() function (moss-specific name)
- start_mdns_lurk_listener() for discovery

**Required Changes**:
- Rename announce_moss() → announce_service()
- Make generic: announce_service(service_type, name, port, txt)
- Move MdnsHandle to common
- Move lurk listener to common

**Benefits**:
- Rake can discover lantern via mDNS
- Lantern can announce itself via mDNS
- All binaries use same mDNS infrastructure
- No code duplication

---

#### `moss/src/network_singletons.rs` → `common/src/infra/communications/socket_helpers.rs`

**Why**: Socket creation utilities needed by any binary doing UDP/TCP.

**Current State**:
- create_reusable_udp_socket() - SO_REUSEADDR helper
- Platform-specific Windows WSAECONNRESET fix
- Generic, not moss-specific

**Required Changes**:
- Move to common/src/infra/communications/socket_helpers.rs
- Add create_reusable_tcp_socket() if needed
- Update imports in moss

**Benefits**:
- Rake can create proper UDP sockets
- Lantern can use same socket utilities
- DRY: Single implementation of Windows quirks

---

### 2. Network Utilities

#### `moss/src/infra/network.rs` → `common/src/infra/net/` (partial)

**Current Functions**:

**SHOULD MOVE TO COMMON**:
- `get_local_ip()` - IP address detection with priority
- `get_local_ip_with_priority()` - Priority-based LAN IP selection
- `get_mac_addresses()` - MAC address enumeration
- `find_mac_for_ip()` - IP→MAC mapping

**Why**: Rake and Lantern may need to:
- Detect their own IP for display
- Find MAC addresses for WoL
- Enumerate network interfaces

**SHOULD STAY IN MOSS**:
- `send_wake_on_lan()` - Moss-specific orchestration feature
- Docker network detection

**Required Changes**:
- Extract IP/MAC functions to common/src/infra/net/interfaces.rs
- Keep WoL in moss (it's a moss feature)
- Keep Docker-specific stuff in moss

---

### 3. Announcement System (Partial)

#### `moss/src/announcement.rs` → Split between common and moss

**Current State**:
- announce() - Sends TopologyEntry via UDP
- announce_with_change_detection() - Performance optimization
- send_udp_announcement() - Wraps p2p::send_announcement()
- send_goodbye() - Shutdown notification

**Analysis**:
- Uses moss's domain::TopologyEntry (moss-specific)
- Uses p2p transport (should be common)
- Change detection logic is moss-specific

**Decision**: KEEP IN MOSS
- This is moss-specific orchestration
- Uses moss domain types
- Once p2p moves to common, moss can import common::p2p

---

## Should Stay in Moss

### Domain Logic
- ✅ `moss/src/domain/` - Moss-specific business logic
- ✅ `moss/src/api/` - HTTP API handlers
- ✅ `moss/src/app_state.rs` - State management
- ✅ `moss/src/bootstrap/` - Initialization
- ✅ `moss/src/tasks/` - Background tasks (coordinator, discovery_handler)

### Infrastructure (Moss-Specific)
- ✅ `moss/src/infra/docker.rs` - Bollard wrapper
- ✅ `moss/src/infra/manifests/` - YAML manifest loading
- ✅ `moss/src/infra/harvest.rs` - Offering installation
- ✅ `moss/src/infra/firmware.rs` - fwupd integration
- ✅ `moss/src/infra/detection/` - Hardware detection

### Orchestration
- ✅ `moss/src/discovery.rs` - Moss's discover_peers() function
- ✅ `moss/src/console.rs` - Terminal UI
- ✅ `moss/src/metrics.rs` - Metrics collection

---

## Already in Common (Correct)

- ✅ `common/src/types/` - Shared data models
- ✅ `common/src/election.rs` - Election protocol types
- ✅ `common/src/nourishment.rs` - Update types
- ✅ `common/src/client.rs` - HTTP client
- ✅ `common/src/utils/` - Shared utilities
- ✅ `common/src/constants/` - Ports, timeouts, paths

---

## Migration Plan

### Phase 1: Communication Infrastructure (HIGH PRIORITY)

1. **Move p2p.rs to common**
   - Extract TopologyEntry to common/types/topology.rs OR
   - Make UdpEvent use serde_json::Value payloads
   - Update all moss imports
   - Add filtered subscription API: subscribe_to_announcement(type)

2. **Move network_singletons to common**
   - Rename to socket_helpers.rs
   - Move to common/src/infra/communications/
   - Update moss imports

3. **Move mdns.rs to common**
   - Generalize announce_moss() → announce_service()
   - Move to common/src/infra/communications/mdns.rs
   - Update moss, add mDNS to rake/lantern

### Phase 2: Network Utilities

4. **Extract network.rs IP/MAC functions**
   - Create common/src/infra/net/interfaces.rs
   - Move: get_local_ip, get_mac_addresses, find_mac_for_ip
   - Keep WoL in moss

### Phase 3: Rake Refactoring

5. **Remove bespoke UDP from rake**
   - Use garden_common::infra::communications::p2p
   - Remove duplicate socket creation
   - Use envelope format via common

6. **Test full stack**
   - deploy discovery
   - rake tend another
   - rake observe
   - mDNS discovery

---

## File Structure After Migration

```
src/common/src/
├── infra/
│   ├── communications/
│   │   ├── p2p.rs              # ← FROM moss (UDP transport)
│   │   ├── mdns.rs             # ← FROM moss (mDNS)
│   │   └── socket_helpers.rs  # ← FROM moss/network_singletons.rs
│   └── net/
│       └── interfaces.rs       # ← FROM moss/infra/network.rs (partial)
└── types/
    └── topology.rs             # ← FROM moss/domain/ (if needed)

src/moss/src/
├── domain/                     # STAYS (moss-specific)
├── api/                        # STAYS (HTTP handlers)
├── infra/
│   ├── docker.rs               # STAYS
│   ├── network.rs              # STAYS (WoL only)
│   └── ...                     # STAYS
├── tasks/                      # STAYS (uses common::p2p)
├── announcement.rs             # STAYS (uses common::p2p)
└── bootstrap/                  # STAYS

src/rake/src/
├── discovery.rs                # REFACTORED (uses common::p2p)
└── ...
```

---

## Benefits of This Refactoring

1. **DRY**: No duplicate UDP/mDNS code in rake
2. **SoC**: Communication is infrastructure, not moss-specific
3. **Testability**: Can test rake discovery without moss running
4. **Extensibility**: Lantern can use same communication stack
5. **Maintainability**: Single source of truth for protocols
6. **Correctness**: All binaries use same envelope format

---

## Risks & Mitigation

**Risk**: Breaking moss during migration  
**Mitigation**: 
- Move one file at a time
- Keep imports working with re-exports
- Test after each move

**Risk**: TopologyEntry coupling  
**Mitigation**:
- Option A: Move to common/types/ (if truly shared)
- Option B: Make UdpEvent generic with serde_json::Value

**Risk**: Platform-specific code in common  
**Mitigation**:
- Keep #[cfg(target_os)] in common (acceptable)
- Both moss and rake target same platforms

---

## Next Steps

**DECISION**: Move TopologyEntry to `common/types/topology.rs` (Option A)

**Rationale**:
- Topology is a common concern across the system
- Rake queries topology via HTTP API (part of API contract)
- Multiple components understand topology structure
- Enables p2p to use it without circular dependencies
- Clean domain modeling: topology is a shared domain concept

1. ✅ Architecture decision made: TopologyEntry → common
2. Start Phase 1: Move TopologyEntry to common/types/
3. Move p2p.rs to common with TopologyEntry import
4. Add filtered subscription API per COMM-0002
5. Update moss to use garden_common::infra::communications::p2p
6. Refactor rake to use common transport
7. Test full discovery flow
8. Document new architecture in ARCHITECTURE-REFERENCE.md

---

**Decision Authority**: Architecture team  
**Impact**: HIGH - Affects all UDP communication  
**Timeline**: 1-2 days for full migration  
**Testing Required**: Integration tests for discovery, election, topology
