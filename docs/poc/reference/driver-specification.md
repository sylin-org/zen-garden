# Zen Garden Driver Specification

**Version:** 2.0  
**Status:** Reference Implementation  
**Last Updated:** 2026-05-04

> **Build applications that discover infrastructure automatically.**  
> This guide helps you create drivers, SDKs, and integrations for Zen Garden.

---

## Quick Start

**Discover a Stone in 10 lines of Python:**

```python
import socket, json, time

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
sock.settimeout(3.0)

request = {"discover": "moss", "request_id": f"req-{int(time.time()*1000)}", "requester": "quickstart"}
sock.sendto(json.dumps(request).encode(), ('255.255.255.255', 7184))

data, _ = sock.recvfrom(2048)
stone = json.loads(data)
print(f"Found: {stone['stone_name']} at {stone['stone_endpoint']}")
```

**Then query its services:**

```bash
curl -s http://stone-topaz-basin.local:7185/api/v1/stone/services | jq '.data'
```

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Stone Discovery](#2-stone-discovery)
3. [Real-World Scenarios](#3-real-world-scenarios)
4. [Endpoint Resolution](#4-endpoint-resolution)
5. [Tending (Stone Pinning)](#5-tending-stone-pinning)
6. [HTTP API Reference](#6-http-api-reference)
7. [Service Discovery](#7-service-discovery)
8. [Companion Integration](#8-Companion-integration)
9. [Connection Strings](#9-connection-strings)
10. [Caching Strategy](#10-caching-strategy)
11. [Error Handling](#11-error-handling)
12. [Type Definitions](#12-type-definitions)
13. [Implementation Checklist](#13-implementation-checklist)
14. [Troubleshooting](#14-troubleshooting)

---

## 1. Architecture Overview

### 1.1 Network Topology

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           ZEN GARDEN                                    │
│                                                                         │
│   ┌─────────────┐         ┌─────────────┐         ┌─────────────┐      │
│   │   STONE A   │◄───────►│   STONE B   │◄───────►│   STONE C   │      │
│   │ (MongoDB)   │  UDP    │ (Redis)     │  UDP    │ (Ollama)    │      │
│   │ Moss :7185  │  7184   │ Moss :7185  │  7184   │ Moss :7185  │      │
│   └──────┬──────┘         └──────┬──────┘         └──────┬──────┘      │
│          │                       │                       │              │
│          └───────────────────────┼───────────────────────┘              │
│                                  │                                      │
│                         ┌────────▼────────┐                             │
│                         │    YOUR APP     │                             │
│                         │   (Driver SDK)  │                             │
│                         └─────────────────┘                             │
│                                                                         │
│   Optional:  ┌─────────────┐                                            │
│              │   LANTERN   │  Cross-subnet registry                     │
│              │    :7186    │  (for larger gardens)                      │
│              └─────────────┘                                            │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Component Responsibilities

| Component | Port | Role |
|-----------|------|------|
| **Moss** | 7185 (HTTP) | Per-stone daemon. Manages containers, announces services, handles API requests. |
| **P2P Transport** | 7184 (UDP) | Discovery broadcasts, chirps, elections. Multicast + directed broadcast. |
| **Lantern** | 7186 (HTTP) | Optional registry for cross-subnet discovery or Windows without mDNS. |
| **Companions** | 7187-7199 | Optional presence Companions (Cricket audio, Firefly LEDs, OLED display). |

### 1.3 Key Concepts

| Term | Description |
|------|-------------|
| **Stone** | A device running Moss (old laptop, thin client, Raspberry Pi). Physical infrastructure you can touch. |
| **Moss** | Daemon on each Stone. Manages Docker containers, broadcasts presence, serves HTTP API. |
| **Chirp** | Periodic UDP broadcast (30s) announcing Stone state and services. |
| **Tending** | Pinning a client to a specific Stone. Persists across sessions. |
| **Offering** | Service template (MongoDB, Redis, Ollama). Curated configs with compatibility rules. |
| **Lantern** | Optional central registry. Required only for cross-subnet or Windows discovery. |

---

## 2. Stone Discovery

### 2.1 Transport Architecture (Multicast-First)

**Problem:** Traditional UDP broadcast (`255.255.255.255`) fails on multi-homed systems (Windows 11 with WSL/Hyper-V). The OS routes broadcasts through virtual interfaces, never reaching the physical LAN.

**Solution:** Zen Garden uses **multicast-first** with **directed broadcast fallback**.

| Transport | Address | Purpose |
|-----------|---------|---------|
| **Primary** | `239.255.42.99:7184` (multicast) | Works on all interfaces, explicit NIC targeting |
| **Secondary** | `<subnet>.255:7184` (directed broadcast) | Per-interface, computed from IP+prefix |
| **Legacy** | `255.255.255.255:7184` (limited broadcast) | Disabled by default, unreliable on multi-homed |

**Environment Variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `DISCOVERY_PORT` | `7184` | UDP port for all discovery |
| `DISCOVERY_MCAST_GROUP` | `239.255.42.99` | IPv4 multicast group |
| `DISCOVERY_ENABLE_BCAST_FALLBACK` | `true` | Enable directed broadcast |
| `DISCOVERY_ENABLE_LIMITED_BCAST` | `false` | Enable legacy 255.255.255.255 |

### 2.2 Discovery Request

**Send a JSON message to `239.255.42.99:7184` (multicast) or `255.255.255.255:7184` (broadcast):**

```json
{
  "discover": "moss",
  "request_id": "01936e8a-7b2c-7def-8123-456789abcdef",
  "requester": "my-driver-v1.0"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `discover` | `string` | Always `"moss"`. Reserved for future discovery types. |
| `request_id` | `string` | Unique ID (UUIDv7 recommended). Used for election delay calculation. |
| `requester` | `string` | Your driver name. Useful for debugging network traces. |

### 2.3 Discovery Response

**Stones respond with unicast UDP back to your source address:**

```json
{
  "stone_id": "01936e8a-7b2c-7def-8123-456789abcdef",
  "stone_name": "stone-topaz-basin",
  "stone_endpoint": "http://192.168.1.100:7185",
  "moss_version": "0.1.0.312",
  "lantern_endpoint": "http://192.168.1.1:7186"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `stone_id` | `string?` | **Immutable GUID v7.** Cache key. Survives hostname changes. |
| `stone_name` | `string` | Human-readable hostname. May change (user renames Stone). |
| `stone_endpoint` | `string` | Full HTTP URL for Moss API. |
| `moss_version` | `string` | Format: `{semver}.{build_number}` (e.g., `0.1.0.312`). |
| `lantern_endpoint` | `string?` | Lantern registry URL, if known. May be `null`. |

### 2.4 Election Delay (Anti-Storm)

**Problem:** If 50 Stones respond simultaneously, you get a packet storm.

**Solution:** Each Stone calculates a deterministic delay using BLAKE3 hash:

```rust
fn calculate_election_delay(stone_id: &str, request_id: &str) -> Duration {
    let input = format!("election:{}:{}", stone_id, request_id);
    let hash = blake3::hash(input.as_bytes());
    
    // First byte (0-255) × 30ms = 0-7650ms spread
    let delay_ms = (hash.as_bytes()[0] as u64) * 30;
    Duration::from_millis(delay_ms)
}
```

**Driver behavior:**
- **Single Stone needed:** Accept first response, stop listening.
- **Topology scan:** Wait full 3 seconds, collect all responses, deduplicate by `stone_id`.

### 2.5 Chirp Protocol (Passive Discovery)

**Stones broadcast their state every 30 seconds.** Listen passively to maintain live topology.

```json
{
  "msg_id": "01936e8b-1234-7def-8123-456789abcdef",
  "type": "stone_chirp",
  "data": {
    "stone_id": "01936e8a-7b2c-7def-8123-456789abcdef",
    "stone_name": "stone-topaz-basin",
    "endpoint": "http://192.168.1.100:7185",
    "moss_version": "0.1.0.312",
    "services": [
      {
        "name": "mongodb",
        "offering": "mongodb",
        "category": "database",
        "status": "Running"
      }
    ]
  }
}
```

**Chirp Timing:**

| Event | Interval |
|-------|----------|
| Periodic chirp | 30 seconds |
| Service state change | Immediate (100ms debounce) |
| Keep-alive | 5 minutes |

**Envelope Format (`UdpAnnouncement`):**

| Field | Type | Description |
|-------|------|-------------|
| `msg_id` | `string?` | GUIDv7 for deduplication. Same message may arrive via multicast AND broadcast. |
| `type` | `string` | Discriminator: `stone_chirp`, `discovery_request`, `discovery_response`, etc. |
| `data` | `object` | Typed payload (varies by `type`). |

**Announcement Types:**

```rust
pub const DISCOVERY_REQUEST: &str = "discovery_request";
pub const DISCOVERY_RESPONSE: &str = "discovery_response";
pub const STONE_CHIRP: &str = "stone_chirp";
pub const STONE_GOODBYE: &str = "stone_goodbye";
pub const ELECTION_REQUEST: &str = "election_request";
pub const ELECTION_CANDIDATE: &str = "election_candidate";
pub const ELECTION_RESULT: &str = "election_result";
```

### 2.6 Complete Discovery Implementation

```python
import socket
import json
import time
from typing import List, Optional, Callable

class DiscoveryResponse:
    def __init__(self, data: dict):
        self.stone_id: Optional[str] = data.get('stone_id')
        self.stone_name: str = data['stone_name']
        self.stone_endpoint: str = data['stone_endpoint']
        self.moss_version: str = data['moss_version']
        self.lantern_endpoint: Optional[str] = data.get('lantern_endpoint')

def discover_stones(
    timeout_sec: float = 3.0,
    on_discovered: Optional[Callable[[DiscoveryResponse], None]] = None
) -> List[DiscoveryResponse]:
    """
    Discover all Stones on the local network.
    
    Uses multicast first, falls back to broadcast.
    Streams results via callback for progressive disclosure.
    
    Args:
        timeout_sec: Maximum wait time (default 3s)
        on_discovered: Optional callback invoked per Stone (progressive UI)
    
    Returns:
        List of unique DiscoveryResponse objects (deduplicated by stone_id)
    """
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.settimeout(0.1)  # Non-blocking reads for streaming
    
    # Generate unique request ID (UUIDv7 recommended, timestamp fallback)
    request_id = f"req-{int(time.time() * 1000)}"
    request = {
        "discover": "moss",
        "request_id": request_id,
        "requester": "python-driver"
    }
    payload = json.dumps(request).encode()
    
    # Send to both multicast and broadcast for maximum compatibility
    MULTICAST_GROUP = '239.255.42.99'
    BROADCAST_ADDR = '255.255.255.255'
    DISCOVERY_PORT = 7184
    
    sock.sendto(payload, (MULTICAST_GROUP, DISCOVERY_PORT))
    sock.sendto(payload, (BROADCAST_ADDR, DISCOVERY_PORT))
    
    stones = {}  # Deduplicate by stone_id
    deadline = time.time() + timeout_sec
    
    while time.time() < deadline:
        try:
            data, addr = sock.recvfrom(4096)
            response = json.loads(data.decode())
            
            # Skip non-discovery responses (chirps, elections, etc.)
            if 'stone_endpoint' not in response:
                continue
            
            stone = DiscoveryResponse(response)
            key = stone.stone_id or stone.stone_name
            
            if key not in stones:
                stones[key] = stone
                if on_discovered:
                    on_discovered(stone)
        except socket.timeout:
            continue
        except json.JSONDecodeError:
            continue
    
    sock.close()
    return list(stones.values())

# Progressive disclosure usage
def main():
    print("Discovering Stones...")
    stones = discover_stones(
        timeout_sec=3.0,
        on_discovered=lambda s: print(f"  Found: {s.stone_name} ({s.stone_endpoint})")
    )
    print(f"\nTotal: {len(stones)} Stone(s)")

if __name__ == "__main__":
    main()
```

---

## 3. Real-World Scenarios

### Scenario 1: App Startup — Find MongoDB

**Goal:** Application starts, needs MongoDB connection string.

```python
async def get_mongodb_connection() -> str:
    """
    Find MongoDB in the Garden. Falls back gracefully.
    
    Resolution order:
    1. Tended Stone (cached preference)
    2. UDP discovery (any Stone offering MongoDB)
    3. Lantern registry (cross-subnet fallback)
    """
    # Try tended Stone first
    tending = load_tending_state()
    if tending:
        try:
            services = await http_get(f"{tending.endpoint}/api/v1/stone/services")
            for svc in services['data']:
                if svc['offering'] == 'mongodb' and svc['status'] == 'Running':
                    return f"mongodb://{parse_host(tending.endpoint)}:27017"
        except ConnectionError:
            pass  # Stone offline, fall through

    # Discover any Stone with MongoDB
    stones = await discover_stones_async(timeout=3.0)
    for stone in stones:
        services = await http_get(f"{stone.stone_endpoint}/api/v1/stone/services")
        for svc in services['data']:
            if svc['offering'] == 'mongodb' and svc['status'] == 'Running':
                return f"mongodb://{parse_host(stone.stone_endpoint)}:{svc['ports']['native']}"
    
    raise ServiceNotFoundError("MongoDB not found in Garden")
```

### Scenario 2: Hardware Failure — Automatic Reconnect

**Goal:** Stone dies, app reconnects to replacement.

```
Timeline:
  T+0:   stone-alpha (MongoDB) crashes
  T+5:   App connection fails, enters retry loop
  T+30:  stone-beta comes online with MongoDB
  T+35:  App discovers stone-beta via chirp or rediscovery
  T+36:  App connects to stone-beta, resumes operation
```

**Implementation:**

```python
class ResilientConnection:
    def __init__(self, offering: str):
        self.offering = offering
        self.current_stone: Optional[str] = None
        self.connection = None
    
    async def ensure_connected(self):
        if self.connection and self.connection.is_alive():
            return self.connection
        
        # Rediscover on failure
        stones = await discover_stones_async(timeout=3.0)
        for stone in stones:
            services = await http_get(f"{stone.stone_endpoint}/api/v1/stone/services")
            for svc in services['data']:
                if svc['offering'] == self.offering and svc['status'] == 'Running':
                    self.current_stone = stone.stone_endpoint
                    self.connection = await self._connect(stone, svc)
                    return self.connection
        
        raise ServiceNotFoundError(f"{self.offering} unavailable")
```

### Scenario 3: Multi-Stone Topology Dashboard

**Goal:** Display all Stones and their services in real-time.

```python
import asyncio

class TopologyWatcher:
    def __init__(self):
        self.stones: dict[str, dict] = {}
        self.on_update: Optional[Callable] = None
    
    async def watch(self):
        """
        Maintain live topology via chirp listening + periodic discovery.
        """
        # Initial discovery
        for stone in await discover_stones_async():
            await self._update_stone(stone)
        
        # Listen for chirps (passive updates)
        asyncio.create_task(self._chirp_listener())
        
        # Periodic full discovery (catch new Stones, prune stale)
        while True:
            await asyncio.sleep(90)  # TTL is 90s
            await self._full_refresh()
    
    async def _chirp_listener(self):
        sock = create_udp_socket()
        while True:
            data = await sock.recvfrom(4096)
            msg = json.loads(data)
            if msg.get('type') == 'stone_chirp':
                await self._handle_chirp(msg['data'])
    
    async def _handle_chirp(self, chirp: dict):
        stone_id = chirp['stone_id']
        self.stones[stone_id] = {
            'name': chirp['stone_name'],
            'endpoint': chirp['endpoint'],
            'services': chirp['services'],
            'last_seen': time.time(),
        }
        if self.on_update:
            self.on_update(self.stones)
```

### Scenario 4: Cross-Subnet Discovery (Lantern)

**Goal:** Discover Stones on different subnets via Lantern registry.

```
Network:
  Subnet A: 192.168.1.0/24 (stone-alpha, stone-beta)
  Subnet B: 192.168.2.0/24 (stone-gamma)
  Lantern:  192.168.1.5:7186 (bridges subnets)
```

**Implementation:**

```python
async def discover_via_lantern(lantern_endpoint: str) -> List[Stone]:
    """
    Query Lantern for cross-subnet discovery.
    
    Lantern aggregates Stone registrations and proxies discovery.
    """
    response = await http_get(f"{lantern_endpoint}/api/v1/garden/stones")
    return [Stone.from_dict(s) for s in response['data']['stones']]
```

**Resolution priority:**
1. Local UDP discovery (same subnet)
2. Lantern HTTP query (cross-subnet)
3. Explicit `--at` flag (manual override)

---

## 4. Endpoint Resolution

### 4.1 Resolution Chain

Drivers MUST resolve endpoints in this order:

```
1. Explicit target       (user provided: --at stone-alpha)
2. Environment variable  (ZG_STONE=http://192.168.1.100:7185)
3. Tending state         (cached Stone preference, verified reachable)
4. Auto-discovery        (UDP broadcast, first responder or best match)
```

### 4.2 Target Format Normalization

Accept flexible input formats:

| User Input | Normalized Endpoint |
|------------|---------------------|
| `http://192.168.1.100:7185` | Use as-is |
| `https://stone.example.com` | Use as-is |
| `192.168.1.100:7185` | Prepend `http://` |
| `192.168.1.100` | Append `:7185`, prepend `http://` |
| `stone-alpha.local` | Append `:7185`, prepend `http://` |
| `stone-alpha` | Resolve via mDNS/UDP/Lantern |
| `01936e8a-7b2c-...` | Resolve by stone_id via discovery |

### 4.3 Implementation

```python
import re

def normalize_endpoint(target: str) -> str:
    """
    Normalize user-provided target to full HTTP URL.
    """
    # Already a URL
    if target.startswith('http://') or target.startswith('https://'):
        return target
    
    # IP:port format
    if re.match(r'^\d+\.\d+\.\d+\.\d+:\d+$', target):
        return f"http://{target}"
    
    # IP only (add default port)
    if re.match(r'^\d+\.\d+\.\d+\.\d+$', target):
        return f"http://{target}:7185"
    
    # hostname.local or hostname.domain
    if '.' in target:
        if ':' in target:
            return f"http://{target}"
        return f"http://{target}:7185"
    
    # Bare stone name or stone_id - requires resolution
    return None  # Caller must use discovery

async def resolve_endpoint(
    target: Optional[str] = None,
    cache: Optional[StoneCache] = None
) -> str:
    """
    Resolve target to HTTP endpoint using full resolution chain.
    """
    # Priority 1: Explicit target
    if target:
        normalized = normalize_endpoint(target)
        if normalized:
            return normalized
        # Bare name - resolve via discovery
        return await resolve_stone_name(target, cache)
    
    # Priority 2: Environment variable
    env_stone = os.environ.get('ZG_STONE')
    if env_stone:
        return normalize_endpoint(env_stone) or env_stone
    
    # Priority 3: Tending state
    tending = load_tending_state()
    if tending and await is_reachable(tending.endpoint, timeout=2.0):
        return tending.endpoint
    
    # Priority 4: Auto-discovery
    stones = await discover_stones_async(timeout=3.0)
    if stones:
        return stones[0].stone_endpoint
    
    raise NoStonesFoundError("No Stones discovered on network")
```

---

## 5. Tending (Stone Pinning)

### 5.1 Concept

**Tending** persists a Stone preference across sessions. Once you "tend" a Stone, your driver automatically targets it for all operations—even across restarts.

**Key behaviors:**
- No TTL expiration (tending never expires automatically)
- Validity checked at use time (is Stone reachable?)
- Falls back to discovery if tended Stone is offline
- User can override with explicit `--at` flag

### 5.2 Tending State File

**Location:** `~/.zen-garden/.tending`  
**Format:** JSON

```json
{
  "stone_name": "stone-topaz-basin",
  "endpoint": "http://192.168.1.100:7185",
  "last_seen": "2026-01-27T12:34:56.789Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `stone_name` | `string` | Display name of tended Stone |
| `endpoint` | `string` | HTTP endpoint URL |
| `last_seen` | `ISO 8601` | When tending was written (informational) |

### 5.3 Tending with Fallback

```python
async def execute_with_tending(
    operation: Callable[[str], Awaitable[T]],
    discovery_timeout: float = 3.0,
    on_fallback: Optional[Callable[[str], None]] = None
) -> tuple[T, str]:
    """
    Execute operation against tended Stone with automatic fallback.
    
    Algorithm:
    1. Try tended Stone immediately
    2. After 3 seconds, start parallel discovery
    3. If tended responds (even slowly), use it
    4. If tended fails AND discovery finds alternative, use alternative
    5. Update tending if new Stone elected
    
    Args:
        operation: Async function taking endpoint, returning result
        discovery_timeout: How long to wait for fallback discovery
        on_fallback: Callback when switching to different Stone
    
    Returns:
        Tuple of (result, endpoint_used)
    """
    tending = load_tending_state()
    
    if tending:
        try:
            # Race: tended Stone vs discovery
            result = await asyncio.wait_for(
                operation(tending.endpoint),
                timeout=5.0
            )
            return result, tending.endpoint
        except (ConnectionError, asyncio.TimeoutError):
            if on_fallback:
                on_fallback(tending.stone_name)
    
    # Fallback to discovery
    stones = await discover_stones_async(timeout=discovery_timeout)
    for stone in stones:
        try:
            result = await operation(stone.stone_endpoint)
            # Elect new tended Stone
            write_tending(stone.stone_name, stone.stone_endpoint)
            return result, stone.stone_endpoint
        except ConnectionError:
            continue
    
    raise NoStonesFoundError("All Stones unreachable")
```

### 5.4 Setting Tending

```python
def write_tending(stone_name: str, endpoint: str) -> None:
    """Persist tending state."""
    state = {
        "stone_name": stone_name,
        "endpoint": endpoint,
        "last_seen": datetime.utcnow().isoformat() + "Z"
    }
    
    path = Path.home() / ".zen-garden" / ".tending"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, indent=2))

def clear_tending() -> None:
    """Remove tending state."""
    path = Path.home() / ".zen-garden" / ".tending"
    if path.exists():
        path.unlink()
```

---

## 6. HTTP API Reference

### 6.1 Base URLs

```
Stone API:   http://{stone}:7185
Lantern:     http://{lantern}:7186
```

### 6.2 Response Format

**All responses use this envelope:**

```json
{
  "data": { /* payload */ },
  "suggestions": ["Optional hint 1", "Optional hint 2"]
}
```

**Error responses:**

```json
{
  "error": {
    "code": "SERVICE_NOT_FOUND",
    "message": "Service 'mongodb' not found on this stone",
    "details": null
  }
}
```

### 6.3 Core Endpoints

#### Health Check

```http
GET /health
```

```json
{
  "status": "healthy",
  "version": "0.1.0.312",
  "timestamp": "2026-01-27T12:34:56.789Z",
  "os": "linux",
  "architecture": "x86_64",
  "components": {
    "docker": { "status": "healthy", "version": "24.0.7" },
    "disk": { "status": "healthy", "used_percent": 45 },
    "memory": { "status": "healthy", "used_percent": 62 }
  }
}
```

#### Hardware Capabilities

```http
GET /api/v1/stone/capabilities
```

```json
{
  "data": {
    "stone_id": "01936e8a-7b2c-7def-8123-456789abcdef",
    "stone_name": "stone-topaz-basin",
    "detection_status": "complete",
    "hardware": {
      "cpu": { "model": "Intel Core i5-8250U", "cores": 4, "architecture": "x86_64" },
      "memory": { "total_mb": 8192 },
      "gpus": [
        { "vendor": "NVIDIA", "model": "GTX 1060", "vram_mb": 6144, "ai_runtimes": ["cuda", "cuda:12.2"] }
      ],
      "ai_capabilities": {
        "runtimes": ["cuda", "cuda:12.2"],
        "vendors": ["nvidia"],
        "total_vram_mb": 6144,
        "gpu_count": 1,
        "detection_complete": true
      }
    }
  }
}
```

### 6.4 Offerings API (Human Layer)

**List offerings:**

```http
GET /api/v1/stone/offerings
GET /api/v1/stone/offerings?state=available
GET /api/v1/stone/offerings?state=installed
```

**Search offerings:**

```http
GET /api/v1/stone/offerings/search?q=nosql%20database&limit=5
```

```json
{
  "data": {
    "query": "nosql database",
    "tokens": ["nosql", "database", "mongodb"],
    "results": [
      {
        "name": "mongodb",
        "category": "data",
        "description": "Document database for modern apps",
        "tags": ["nosql", "document-store"],
        "image": "mongo:7.0",
        "score": 95,
        "compatibility": "pass"
      }
    ],
    "total_offerings": 30
  }
}
```

**Install (plant) offering:**

```http
POST /api/v1/stone/offerings
Content-Type: application/json

{
  "name": "mongodb",
  "config": {
    "environment": { "MONGO_INITDB_ROOT_USERNAME": "admin" }
  }
}
```

Response (202 Accepted):

```json
{
  "data": {
    "name": "mongodb",
    "state": "installing",
    "job_id": "job_01936e8b-1234-7def-8123-456789abcdef"
  }
}
```

**Uninstall (take away) offering:**

```http
DELETE /api/v1/stone/offerings/{name}
```

### 6.5 Services API (Technical Layer)

**List services (container-level):**

```http
GET /api/v1/stone/services
```

```json
{
  "data": [
    {
      "name": "mongodb",
      "offering": "mongodb",
      "version": "7.0.4",
      "status": "Running",
      "health": "Healthy",
      "ports": { "native": 27017, "agnostic": null },
      "resources": {
        "cpu_percent": 2.5,
        "cpu_friendly": "2.5%",
        "memory_bytes": 536870912,
        "memory_friendly": "512 MB",
        "uptime_seconds": 172800,
        "uptime_friendly": "2d 0h"
      }
    }
  ]
}
```

**Service details:**

```http
GET /api/v1/stone/services/{name}
```

### 6.6 Garden Endpoints (Cross-Stone Orchestration)

**Topology (all Stones):**

```http
GET /api/v1/garden/topology
```

```json
{
  "data": {
    "stones": [
      {
        "stone_id": "01936e8a-7b2c-7def-8123-456789abcdef",
        "stone_name": "stone-topaz-basin",
        "endpoint": "http://192.168.1.100:7185",
        "moss_version": "0.1.0.312",
        "health": "thriving",
        "status": "online",
        "services": [
          { "name": "mongodb", "offering": "mongodb", "category": "database", "status": "Running" }
        ]
      }
    ]
  }
}
```

**Garden-wide updates:**

```http
GET /api/v1/garden/updates
POST /api/v1/garden/updates/execute
```

---

## 7. Service Discovery

### 7.1 Finding Services by Name

```python
async def find_service(offering: str) -> Optional[ServiceInfo]:
    """Find a running service by offering name."""
    
    # Option 1: Query tended Stone
    tending = load_tending_state()
    if tending:
        services = await http_get(f"{tending.endpoint}/api/v1/stone/services")
        for svc in services['data']:
            if svc['offering'] == offering and svc['status'] == 'Running':
                return ServiceInfo.from_dict(svc, tending.endpoint)

    # Option 2: Discover and query all Stones
    stones = await discover_stones_async()
    for stone in stones:
        services = await http_get(f"{stone.stone_endpoint}/api/v1/stone/services")
        for svc in services['data']:
            if svc['offering'] == offering and svc['status'] == 'Running':
                return ServiceInfo.from_dict(svc, stone.stone_endpoint)
    
    return None
```

### 7.2 Search Query Syntax

Use prefix syntax for advanced searches:

| Prefix | Meaning | Example |
|--------|---------|---------|
| (none) | Name search | `mongodb` |
| `c:`, `cat:`, `category:` | Category | `c:database` |
| `t:`, `tag:`, `tags:` | Tag | `t:nosql` |

```http
GET /api/v1/stone/services?q=mongodb          # Name search
GET /api/v1/stone/services?q=c:database       # Category search
GET /api/v1/stone/services?q=t:nosql          # Tag search
GET /api/v1/stone/services?fresh=true         # Force network scan
```

### 7.3 Default Ports

When port information is unavailable, use these defaults:

| Offering | Port |
|----------|------|
| mongodb | 27017 |
| redis | 6379 |
| postgres | 5432 |
| mysql / mariadb | 3306 |
| elasticsearch | 9200 |
| meilisearch | 7700 |
| qdrant | 6333 |
| minio | 9000 |
| rabbitmq | 5672 |
| nats | 4222 |
| prometheus | 9090 |
| grafana | 3000 |
| ollama | 11434 |
| (default) | 8080 |

---

## 8. Companion Integration

### 8.1 Companion Overview

Companions extend Stone capabilities with audio, visual, and display feedback.

| Companion | Port | Purpose |
|---------|------|---------|
| **Cricket** | 7187 | Audio feedback (4-channel mixer, 180 CC0 samples) |
| **Firefly** | TBD | LED control |
| **OLED** | TBD | Display status screens |

### 8.2 Listing Companions

```http
GET /api/v1/stone/companions
```

```json
{
  "data": {
    "Companions": [
      {
        "id": "cricket",
        "name": "Cricket Audio Companion",
        "version": "0.1.0",
        "port": 7187,
        "running": true,
        "pid": 12345,
        "commands": 6
      }
    ]
  }
}
```

### 8.3 Sending Commands

```http
POST /api/v1/stone/companions/{id}/command
Content-Type: application/json

{
  "args": ["play", "stone-online"]
}
```

Response:

```json
{
  "data": {
    "success": true,
    "output": "Playing: stone-online.mp3 on foreground channel"
  }
}
```

### 8.4 Command Forwarding Architecture

```
┌─────────┐       ┌─────────┐       ┌─────────────┐
│  YOUR   │──────►│  MOSS   │──────►│  Companion    │
│  DRIVER │ HTTP  │ (proxy) │  HTTP │  (Cricket)  │
│         │ 7185  │         │ 7187  │             │
└─────────┘       └─────────┘       └─────────────┘

Timeout: 5 seconds per command
Port ledger: {data_dir}/companion-ports.json
```

---

## 9. Connection Strings

The full URI grammar is specified in [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md). The discovery-side resolution algorithm is in [specs/discovery.md](../specs/discovery.md). This section is the *minimum viable* implementation for client libraries — enough to handle the dominant case (named offering with optional sub-path).

Driver authors implementing the full grammar (capability queries, kind-explicit forms, wish action, replica pinning) should use the shared test corpus at `docs/specs/zen-garden-uri-test-vectors.json` as the conformance contract.

### 9.1 Format (full)

```
zen-garden:[<target>][/<sub-path>][?<query>][#<fragment>]

<target> := <bare-name>            # cascade resolution
          | <kind>//<name>          # explicit kind
          | (empty, with cap= query)
```

### 9.2 Examples (dominant case: bare name + optional sub-path)

| Connection String | Resolved Native String |
|-------------------|------------------------|
| `zen-garden:mongodb` | `mongodb://stone-alpha.local:27017` |
| `zen-garden:mongodb/mydb` | `mongodb://stone-alpha.local:27017/mydb` |
| `zen-garden:redis` | `redis://stone-beta.local:6379` |
| `zen-garden:postgres/app` | `postgresql://stone-gamma.local:5432/app` |

Advanced forms (full URI-0003 grammar):

| Connection String | Resolution |
|-------------------|------------|
| `zen-garden:?cap=s3` | Capability-only — any S3-speaking endpoint |
| `zen-garden:offering//mongodb` | Explicit offering kind |
| `zen-garden:mongodb?action=wish` | Find-or-provision |
| `zen-garden:mongodb:staging` | Specific instance |

### 9.3 Resolution Algorithm — minimum viable (bare name + sub-path)

A starter parser that handles the dominant case. Use a real URI library (Python `urllib.parse`, Node.js `URL`, Rust `url` crate, C# `System.Uri`) for production implementations.

```python
def resolve_connection_string(conn_str: str) -> str:
    """
    Resolve a zen-garden: URI to native connection string.
    Handles the dominant case: bare offering name with optional sub-path.

    For full URI-0003 grammar (capability queries, explicit kinds,
    wish action, replica pinning), use a real URI parser and follow
    the discovery-layer algorithm in specs/discovery.md.
    """
    if not conn_str.startswith('zen-garden:'):
        return conn_str  # Pass-through native strings

    # Strip scheme. URL-form (zen-garden://) is accepted as a tolerant
    # alias and normalises to URN-form on output.
    remainder = conn_str[len('zen-garden:'):]
    if remainder.startswith('//'):
        remainder = remainder[2:]

    # Split off query/fragment for this minimum-viable parser
    # (full grammar handles ?cap=, ?action=, ?at=, #fragment)
    target_and_path, *_ = remainder.split('?', 1)
    target_and_path, *_ = target_and_path.split('#', 1)

    # Reject explicit-kind form for this starter parser
    if '//' in target_and_path:
        raise NotImplementedError(
            "Explicit-kind form 'zen-garden:<kind>//<name>' requires "
            "the full URI-0003 parser"
        )

    # Bare name + optional sub-path
    parts = target_and_path.split('/', 1)
    target = parts[0]
    sub_path = parts[1] if len(parts) > 1 else None

    # Optional :instance qualifier on the target
    if ':' in target:
        service_type, instance = target.split(':', 1)
    else:
        service_type, instance = target, None

    # Discover service via cascade (offering → stone → bank → ...)
    # and apply instance filter if present
    service = find_service(service_type, instance=instance)
    if not service:
        raise ServiceNotFoundError(f"Service '{target}' not found")

    # Build native connection string from the resolved endpoint
    host = parse_host(service.stone_endpoint)
    port = service.ports.get('native', DEFAULT_PORTS.get(service_type, 8080))

    if service_type == 'mongodb':
        base = f"mongodb://{host}:{port}"
        return f"{base}/{sub_path}" if sub_path else base
    elif service_type == 'redis':
        return f"redis://{host}:{port}"
    elif service_type == 'postgres':
        base = f"postgresql://{host}:{port}"
        return f"{base}/{sub_path}" if sub_path else base
    else:
        # Generic HTTP
        return f"http://{host}:{port}"
```

---

## 10. Caching Strategy

### 10.1 What to Cache

| Item | TTL | Key | Purpose |
|------|-----|-----|---------|
| Stone endpoints | 90s | `stone_id` | Avoid repeated discovery |
| Service lists | 30s | `endpoint` | Reduce API calls |
| Hardware capabilities | 5min | `stone_id` | Placement decisions |
| Topology (from chirps) | Continuous | `stone_id` | Live network view |

### 10.2 Cache Key Strategy

**CRITICAL:** Use `stone_id` (GUID) as primary key, not `stone_name`.

- `stone_name` may change (user renames Stone)
- `stone_id` is immutable (generated once on first boot)

```python
class StoneCache:
    def __init__(self):
        self._by_id: dict[str, CacheEntry] = {}
        self._by_name: dict[str, str] = {}  # name -> id mapping
    
    def get(self, key: str) -> Optional[CacheEntry]:
        """Lookup by stone_id or stone_name (case-insensitive)."""
        key_lower = key.lower()
        
        # Try as stone_id
        if key_lower in self._by_id:
            return self._by_id[key_lower]
        
        # Try as stone_name
        if key_lower in self._by_name:
            return self._by_id.get(self._by_name[key_lower])
        
        return None
    
    def upsert(self, stone_id: str, stone_name: str, data: dict, ttl: float):
        """Insert or update with dual indexing."""
        entry = CacheEntry(data=data, expires=time.time() + ttl)
        
        self._by_id[stone_id.lower()] = entry
        self._by_name[stone_name.lower()] = stone_id.lower()
```

### 10.3 Cache Invalidation

| Event | Action |
|-------|--------|
| Stone unreachable | Mark stale, don't delete (may return) |
| Explicit refresh | Clear and rediscover |
| Chirp received | Update entry, reset TTL |
| Discovery response | Upsert entry |

---

## 11. Error Handling

### 11.1 Error Codes

| Code | HTTP | Description |
|------|------|-------------|
| `SERVICE_NOT_FOUND` | 404 | Service doesn't exist on this Stone |
| `STONE_NOT_FOUND` | 404 | Stone not in garden topology |
| `OFFERING_NOT_FOUND` | 404 | Unknown offering template |
| `INVALID_REQUEST` | 400 | Malformed request body |
| `TEMPLATE_NOT_FOUND` | 400 | Manifest template missing |
| `COMPATIBILITY_FAILED` | 400 | Offering incompatible with Stone hardware |
| `DOCKER_ERROR` | 500 | Container operation failed |
| `DOCKER_UNAVAILABLE` | 503 | Docker daemon not running |
| `INTERNAL_ERROR` | 500 | Unexpected server error |

### 11.2 Retry Strategy

| Scenario | Retry? | Strategy |
|----------|--------|----------|
| Network timeout | Yes | Exponential backoff (1s, 2s, 4s) |
| HTTP 5xx | Yes | Exponential with jitter |
| HTTP 4xx | No | Client error, don't retry |
| Stone unreachable | Fallback | Try discovery for alternative Stone |
| Connection refused | Fallback | Stone may be offline |

### 11.3 Resilient Request Pattern

```python
async def resilient_request(
    endpoint: str,
    method: str,
    path: str,
    body: Optional[dict] = None,
    max_retries: int = 3,
    timeout: float = 30.0
) -> dict:
    """
    Make HTTP request with retry and exponential backoff.
    """
    last_error = None
    
    for attempt in range(max_retries):
        try:
            async with aiohttp.ClientSession() as session:
                url = f"{endpoint}{path}"
                async with session.request(
                    method, url,
                    json=body,
                    timeout=aiohttp.ClientTimeout(total=timeout)
                ) as response:
                    data = await response.json()
                    
                    if response.status >= 400:
                        error = data.get('error', {})
                        if response.status < 500:
                            # Client error - don't retry
                            raise ClientError(error.get('code'), error.get('message'))
                        raise ServerError(error.get('code'), error.get('message'))
                    
                    return data
        
        except (aiohttp.ClientError, asyncio.TimeoutError) as e:
            last_error = e
            if attempt < max_retries - 1:
                delay = (2 ** attempt) + random.uniform(0, 1)
                await asyncio.sleep(delay)
    
    raise ConnectionError(f"Failed after {max_retries} attempts: {last_error}")
```

---

## 12. Type Definitions

### 12.1 Discovery Types

```typescript
interface DiscoveryRequest {
  discover: "moss";
  request_id: string;
  requester: string;
}

interface DiscoveryResponse {
  stone_id?: string;
  stone_name: string;
  stone_endpoint: string;
  moss_version: string;
  lantern_endpoint?: string;
}

interface UdpAnnouncement {
  msg_id?: string;
  type: string;  // "stone_chirp", "discovery_request", etc.
  data: object;
}

interface StoneChirp {
  stone_id: string;
  stone_name: string;
  endpoint: string;
  moss_version: string;
  services: TopologyServiceEntry[];
}
```

### 12.2 Service Types

```typescript
type ServiceStatus = "Installing" | "Running" | "Stopped" | "Maintenance" | "Degraded" | "Unknown";
type ServiceHealth = "Healthy" | "Degraded" | "Offline";
type DetectionStatus = "scanning" | "partial" | "complete";

interface ServiceInfo {
  name: string;
  offering: string;
  version: string;
  status: ServiceStatus;
  health: ServiceHealth;
  ports: { native: number; agnostic?: number };
  resources?: ContainerResources;
  job_id?: string;  // Present when status is "Installing"
}

interface TopologyServiceEntry {
  name: string;
  offering: string;
  category: string;
  status: string;
}
```

### 12.3 Hardware Types

```typescript
interface HardwareCapabilities {
  stone_id?: string;
  stone_name: string;
  detection_status: DetectionStatus;
  hardware: HardwareInventory;
  runtime?: RuntimeInfo;
}

interface HardwareInventory {
  cpu: { model?: string; cores: number; architecture: string };
  memory: { total_mb: number };
  gpus: GpuInfo[];
  disk?: { total_gb: number; disk_type?: string };
  ai_capabilities?: AiCapabilitiesSummary;
}

interface GpuInfo {
  vendor: string;
  model: string;
  vram_mb?: number;
  capabilities: string[];  // "cuda", "rocm", "vulkan", "directml"
  ai_runtimes: string[];   // "cuda", "cuda:12.2", "directml"
}

interface AiCapabilitiesSummary {
  runtimes: string[];      // All runtimes (deduplicated)
  vendors: string[];       // ["nvidia", "amd", "intel"]
  total_vram_mb: number;
  gpu_count: number;
  detection_complete: boolean;
}
```

### 12.4 Topology Types

```typescript
type StoneStatus = "online" | "offline";

interface TopologyEntry {
  stone_id: string;
  stone_name: string;
  endpoint: string;
  moss_version: string;
  services: TopologyServiceEntry[];
  mac?: string;  // For Wake-on-LAN
  health: string;
  capabilities?: HardwareCapabilities;
  status: StoneStatus;
  discovered_at: string;  // ISO 8601
  last_seen: string;      // ISO 8601
}
```

### 12.5 Tending Types

```typescript
interface TendingState {
  stone_name: string;
  endpoint: string;
  last_seen: string;  // ISO 8601
}
```

---

## 13. Implementation Checklist

### 13.1 Minimum Viable Driver (MVP)

- [ ] **UDP discovery** — Send request to multicast/broadcast, parse responses
- [ ] **Endpoint normalization** — Accept all input formats
- [ ] **HTTP client** — With timeout handling (30s default)
- [ ] **Health check** — `GET /health`
- [ ] **Service list** — `GET /api/v1/stone/services`
- [ ] **API unwrapping** — Handle `{ data, suggestions }` envelope
- [ ] **Error parsing** — Handle `{ error: { code, message } }` format

### 13.2 Full-Featured Driver

- [ ] **Tending persistence** — Read/write `~/.zen-garden/.tending`
- [ ] **Stone name resolution** — mDNS, UDP, Lantern fallback
- [ ] **Case-insensitive matching** — For stone names
- [ ] **stone_id cache keying** — Not stone_name
- [ ] **Service search** — Query prefixes (`c:`, `t:`)
- [ ] **Capabilities caching** — 5-minute TTL
- [ ] **Chirp listener** — Passive topology via UDP 7184
- [ ] **Retry with backoff** — Exponential + jitter
- [ ] **Connection pooling** — Reuse HTTP connections
- [ ] **Connection string resolver** — `zen-garden:mongodb/mydb`

### 13.3 Timeout Reference

| Operation | Timeout |
|-----------|---------|
| UDP discovery | 3s (full), 2s (quick) |
| Health check | 2s |
| HTTP request (default) | 30s |
| HTTP connect | 5s |
| mDNS probe | 800ms |
| Companion command | 5s |

### 13.4 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZG_STONE` | — | Override target Stone |
| `ZG_QUIET` | — | Suppress verbose output |
| `ZG_DISCOVERY_TIMEOUT_SECS` | 3 | UDP discovery timeout |
| `ZG_CACHE_TTL_SECS` | 90 | Stone cache TTL |
| `ZG_HTTP_REQUEST_TIMEOUT_SECS` | 30 | HTTP request timeout |
| `DISCOVERY_MCAST_GROUP` | 239.255.42.99 | Multicast group |
| `DISCOVERY_PORT` | 7184 | UDP port |

---

## 14. Troubleshooting

### 14.1 No Stones Discovered

**Check 1: Is Moss running?**

```bash
# On the Stone
systemctl status garden-moss
journalctl -u garden-moss -n 50
```

**Check 2: Firewall blocking UDP 7184?**

```powershell
# Windows
New-NetFirewallRule -DisplayName "Zen Garden Discovery" `
    -Direction Inbound -Action Allow -Protocol UDP -LocalPort 7184
```

```bash
# Linux
sudo iptables -A INPUT -p udp --dport 7184 -j ACCEPT
```

**Check 3: Multicast blocked by network?**

Test multicast directly:

```bash
# Sender
echo "test" | nc -u 239.255.42.99 7184

# Receiver (different machine)
sudo tcpdump -i eth0 'host 239.255.42.99'
```

If multicast is blocked, directed broadcast fallback should work. Check if `DISCOVERY_ENABLE_BCAST_FALLBACK=true`.

**Check 4: Multi-homed system (WSL/Hyper-V)?**

Windows 11 with WSL may route broadcasts incorrectly. Verify Moss logs show physical NIC in "detected interfaces."

### 14.2 Tended Stone Unreachable

Tending doesn't expire, but your driver should:

1. Check health: `GET /health` with 2s timeout
2. If unreachable, log warning: "Stone is sleeping"
3. Fall back to discovery
4. Don't clear tending (Stone may return)

### 14.3 Service Not Found

1. Verify offering is installed: `GET /api/v1/stone/offerings?state=installed`
2. Check container status: `GET /api/v1/stone/services/{name}`
3. Container may be starting: check `status` field for `"Installing"`
4. Track installation: use `job_id` to poll progress

### 14.4 Slow Discovery

**Likely cause:** Multicast blocked, using broadcast fallback.

Check logs for "multicast send failed" warnings. Broadcast is slower but functional.

---

## Appendix A: Quick Reference

### Common Endpoints

```http
GET  /health                                   # Health status
GET  /api/v1/stone/capabilities                # Hardware inventory
GET  /api/v1/stone/services                    # List services
GET  /api/v1/stone/services?q=mongo            # Search services
GET  /api/v1/stone/offerings                   # List offerings
GET  /api/v1/stone/offerings/search?q=nosql    # Search offerings
POST /api/v1/stone/offerings                   # Install offering
GET  /api/v1/garden/topology                   # All Stones
GET  /api/v1/stone/companions                  # List Companions
POST /api/v1/stone/companions/{id}/command     # Send Companion command
```

### Port Summary

```
UDP  7184    Discovery (multicast + broadcast)
HTTP 7185    Moss API (per-Stone)
HTTP 7186    Lantern API (optional registry)
HTTP 7187-7199  Companions (Cricket, Firefly, OLED)
```

### Vitality Language (CLI Display)

Map technical health to human-friendly terms:

| Health | Vitality |
|--------|----------|
| healthy | thriving |
| degraded | needs attention |
| unhealthy | withering |
| (unreachable) | sleeping |

---

## Appendix B: Changelog

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-01-27 | Complete rewrite with real-world scenarios, multicast transport, Companion integration |
| 1.0 | 2026-01-23 | Initial specification |

---

## Appendix C: Related Documents

| Document | Purpose |
|----------|---------|
| [Components](components.md) | Internal development guide |
| [discovery-transport.md](../specs/discovery-transport.md) | Multicast transport details |
| [api-v1.md](../specs/api-v1.md) | Complete API specification |
| [companion-command-protocol.md](../specs/companion-command-protocol.md) | Companion integration |
| [connection-strings.md](connection-strings.md) | Connection string resolution |
| [glossary.md](../glossary.md) | Terminology reference |
