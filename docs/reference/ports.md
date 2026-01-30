---
audience: [contributor, operator, developer]
doc_type: reference
status: current
last_verified: 2026-01-19
canonical: true
note: "Authoritative port registry for all Zen Garden services."
---

# Zen Garden Port Allocation

**Baseline Port:** 7184 (GRDN - phone keypad mapping G=7, R=18, D=4)

**Date:** January 16, 2026  
**Status:** Active  
**Purpose:** Centralized port registry for all Zen Garden network services

---

## Port Allocation Table

| Port | Service | Protocol | Purpose | Status |
|------|---------|----------|---------|--------|
| **7184** | P2P Discovery | UDP | Stone-to-Stone peer discovery broadcasts | ✅ Active |
| **7185** | Garden-Moss HTTP API | HTTP/TCP | Stone management API endpoint | ✅ Active |
| **7186** | Garden-Lantern Registry | HTTP/TCP | Centralized service registry and topology API | 🔜 Planned |
| **7187-7199** | Moss Companions | HTTP/TCP | Companion command servers (Cricket, Firefly, OLED, etc.) | ✅ Active |

**Companion Port Allocation:**
- **Base:** 7187 (ASCII sum "moss Companion" = 1187 + 6000)
- **Range:** 7187-7199 (13 Companions maximum)
- **Assignment:** Managed by Moss via `companion-ports.json` ledger, incremental from base
- **Current:** Cricket (7187), Firefly (planned), OLED (planned)

---

## Port Details

### 7184 - P2P Discovery (UDP)

**Current Name:** "UDP port 7184 (Discovery)"  
**Function:** Peer-to-peer stone discovery via multicast + broadcast  
**Listeners:** All moss instances  
**Message Types:**
- `DiscoveryRequest` - Broadcast from rake to find available stones
- `DiscoveryResponse` - Unicast response from moss with stone capabilities
- `StoneChirp` - Periodic state broadcast (30s interval)

**Implementation Files:**
- Listener: `src/moss/src/tasks/discovery.rs` - P2P transport subscriber
- Sender: `src/rake/src/discovery.rs` - `discover_moss()` function
- Transport: `src/common/src/infra/communications/p2p.rs` - Multicast + broadcast

**Example Traffic:**
```
Rake → 239.255.42.99:7184 (multicast) or 255.255.255.255:7184 (broadcast fallback)
  {"discover": "moss", "request_id": "uuid", "requester": "rake-client"}

Moss → Rake IP:ephemeral (unicast)
  {"stone_id": "01936e8a-...", "stone_name": "stone-01", "stone_endpoint": "http://192.168.1.100:7185"}
```

---

### 7185 - Garden-Moss HTTP API (TCP)

**Current Name:** "HTTP port 7185 (Moss API)"  
**Function:** Primary stone management HTTP API  
**Protocol:** HTTP/1.1  
**Endpoints:**
- `/health` - Liveness probe
- `/capabilities` - Hardware capabilities query
- `/metrics` - Prometheus metrics
- `/api/v1/services` - List running services
- `/api/v1/offerings` - List/install offerings
- `/api/v1/stone/companions` - Companion management
- `/api/v1/garden/topology` - Cross-stone topology

**Configuration Priority:**
1. CLI argument: `garden-moss --port 7185`
2. Environment variable: `PORT=7185`
3. Config file: `/etc/zen-garden/moss.toml` → `port = 7185`
4. Default: `7185`

**Implementation Files:**
- Server: `src/linux/moss/src/main.rs` - Axum HTTP server
- Default: Line ~2036 → `.unwrap_or(7185)`
- Config: Line ~81-100 → `MossConfig` struct

**Security:**
- Bind: `0.0.0.0:7185` (all interfaces)
- Authentication: Bearer tokens (HMAC-SHA256 JWT) when pond mode active
- CORS: Disabled (internal network only)

---

### 7187-7199 - Moss Companions (TCP)

**Function:** HTTP command servers for Moss Companions (Cricket, Firefly, OLED, etc.)  
**Protocol:** HTTP/1.1  
**Port Assignment:** Managed by Moss via persistent ledger (`{data_dir}/companion-ports.json`)

**Companion Discovery Protocol:**
1. Moss scans `{data_dir}/companions/` for executables
2. Assigns port from ledger (incremental from 7187)
3. Invokes `{Companion} --dump-commands --port {assigned}` to get manifest
4. Starts Companion with `--stone {moss_endpoint} --port {assigned}`
5. Companion binds HTTP server on assigned port

**Command Routing:**
```
Rake → POST /api/v1/stone/companions/{id}/command
  → Moss → POST http://127.0.0.1:{assigned_port}/command
  → Companion executes, returns JSON response (5s timeout)
```

**Currently Allocated:**
- **7187:** Cricket audio Companion (4-channel mixer, tune system)
- **7188+:** Available for Firefly (LEDs), OLED (display), future Companions

**Endpoints (per Companion):**
- `POST /command` - Execute Companion command with args array
- `GET /health` - Health check (optional)
- `GET /manifest` - Return command manifest (optional, Moss caches from --dump-commands)

**Implementation Files:**
- Registry: `src/moss/src/infra/Companions.rs` - Port ledger, Companion registration
- API: `src/moss/src/api/v1/Companions.rs` - Command forwarding endpoints
- Example: `src/cricket/src/main.rs` - Cricket Companion implementation

**Security:**
- Bind: `127.0.0.1:{port}` (localhost only, not exposed to network)
- Authentication: None (Companions trusted as local services)
- Timeout: 5000ms for command execution

**Reference:**
- [Companion-COMMAND-PROTOCOL.md](../specs/Companion-COMMAND-PROTOCOL.md)
- [Companion-SERVICE-REGISTRY.md](../specs/Companion-SERVICE-REGISTRY.md)
- [CRICKET-0001-audio-Companion-spec.md](../decisions/CRICKET-0001-audio-Companion-spec.md)

---

### Deprecated Port Assignments

**7187 - Garden-Lantern Election (UDP)** - ❌ Superseded by Companion framework

**Status:** Planned (Phase 1 implementation)  
**Function:** Centralized service registry and topology management  
**Protocol:** HTTP/1.1  
**Endpoints:**
- `POST /api/register` - Stone heartbeat registration
- `GET /api/resolve?service={type}` - Service discovery
- `GET /api/stones` - Full topology query
- `GET /api/topology` - Topology sync (active Lantern only)
- `GET /api/health` - Health check
- `GET /api/events/stream` - SSE event stream

**Configuration:**
- Default port: `7186`
- Environment variable: `LANTERN_PORT=7186`
- Config file: `/etc/zen-garden/lantern.toml` → `port = 7186`

**Implementation Files:**
- Planned: `src/lantern/src/main.rs`
- Proposal: `docs/proposals/LANTERN-SERVICE-PROPOSAL.md`

---

### 7187 - Garden-Lantern Election (UDP)

**Status:** Planned (Phase 1 implementation)  
**Function:** Multi-active Garden-Lantern Election and health announcements  
**Protocol:** UDP broadcast  
**Message Types:**
- `LANTERN_ANNOUNCEMENT` - Active Lantern health signal (every 10s)
- `LANTERN_DISCOVERY` - New primary requests stone re-registration

**Election Protocol:**
- Dormant Lanterns listen on 7187
- Active Lanterns broadcast on 7187 every 10s
- Election delay: `blake3::hash(lantern_name + lan_ip + announcement_id)[0] * 10ms`
- Suppression: Candidates hearing announcement suppress their own

**Implementation Files:**
- Planned: `src/lantern/src/election.rs`

---

## Migration Notes

### Current Ports → New Ports

| Old Port | New Port | Service | Migration Action |
|----------|----------|---------|------------------|
| 3004 | 7184 | UDP Discovery | Update bind and broadcast addresses |
| 3001 | 7185 | Moss HTTP | Update default, docs, config examples |
| 3000 | 7186 | Lantern HTTP | Use in implementation (not yet deployed) |
| N/A | 7187 | Garden-Lantern Election | New allocation for Phase 1 |

### Files Requiring Updates

**Code:**
- `src/linux/moss/src/discovery.rs` - Line 7: `bind("0.0.0.0:7184")`
- `src/windows/garden-rake/src/discovery.rs` - Line 20: `send_to("255.255.255.255:7184")`
- `src/linux/moss/src/main.rs` - Line ~2036: `.unwrap_or(7185)`
- `src/linux/moss/src/main.rs` - Line ~1931: Graceful shutdown HTTP request to `7185`

**Documentation:**
- `docs/TECHNICAL-SPEC.md` - All references to ports 3001, 3002, 3003, 3004, 3000
- `docs/proposals/LANTERN-SERVICE-PROPOSAL.md` - All references to port 3000 → 7186
- `DEVELOPMENT-PLAN.md` - Docker Compose examples with old ports

**Configuration Examples:**
- `docs/TECHNICAL-SPEC.md` - Line 297: `port = 7185` in TOML example
- Installer scripts referencing default ports
- Docker Compose files in test directories

---

## Port Range Rationale

**7184-7199 (16 ports)** reserved for Zen Garden infrastructure:
- **7184**: GRDN baseline (phone keypad)
- **7185-7187**: Core services (Moss, Lantern HTTP, Lantern UDP)
- **7188-7199**: Future expansion (e.g., metrics aggregator, distributed logs, federation)

**Semantic Meaning:**
- 7184 = GRDN (Garden)
- Memorable, easy to communicate
- Avoids common conflicts (PostgreSQL 5432, Redis 6379, MongoDB 27017, Grafana 3000)

---

## Firewall Rules

**For stone-to-stone communication:**
```bash
# UDP P2P Discovery (broadcast)
sudo ufw allow 7184/udp comment "Zen Garden P2P discovery"

# Garden-Moss HTTP API (TCP)
sudo ufw allow 7185/tcp comment "Zen Garden Moss API"

# Garden-Lantern Registry (TCP)
sudo ufw allow 7186/tcp comment "Zen Garden Garden-Lantern Registry"

# Garden-Lantern Election (UDP)
sudo ufw allow 7187/udp comment "Zen Garden Garden-Lantern Election"
```

**Windows Firewall (PowerShell):**
```powershell
# UDP P2P Discovery
New-NetFirewallRule -DisplayName "Zen Garden P2P Discovery" -Direction Inbound -Protocol UDP -LocalPort 7184 -Action Allow

# Garden-Moss HTTP API
New-NetFirewallRule -DisplayName "Zen Garden Moss API" -Direction Inbound -Protocol TCP -LocalPort 7185 -Action Allow

# Garden-Lantern Registry
New-NetFirewallRule -DisplayName "Zen Garden Garden-Lantern Registry" -Direction Inbound -Protocol TCP -LocalPort 7186 -Action Allow

# Garden-Lantern Election
New-NetFirewallRule -DisplayName "Zen Garden Garden-Lantern Election" -Direction Inbound -Protocol UDP -LocalPort 7187 -Action Allow
```

---

## Testing Port Availability

**Check if port is in use:**
```bash
# Linux
sudo ss -tulpn | grep 7185
sudo lsof -i :7185

# Windows
netstat -ano | findstr :7185
```

**Test UDP broadcast:**
```bash
# Sender (rake simulation)
echo '{"request_id":"test"}' | nc -u -b 255.255.255.255 7184

# Receiver (moss simulation)
nc -ul 7184
```

**Test HTTP endpoint:**
```bash
curl http://localhost:7185/health
```

---

## Reserved Port Expansion Ideas

**Future Services (7188-7199):**
- **7188**: Metrics aggregator (Prometheus exporter)
- **7189**: Distributed logging (centralized logs collector)
- **7190**: Federation gateway (multi-garden coordination)
- **7191**: Backup coordinator (snapshot orchestration)
- **7192**: MCP gateway (Model Context Protocol proxy)
- **7193-7199**: Reserved for future infrastructure

---

## Version History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-01-16 | 1.0 | Initial port allocation (7184-7187) | Architecture Team |

---

## See Also

- [Security Specification](../specs/security.md) - Bearer token authentication
- [Moss Daemon Lifecycle](../specs/moss-daemon-lifecycle.md) - Moss API reference
