---
audience: [developer, contributor, operator]
doc_type: spec
status: current
last_verified: 2026-02-12
---

# Koi mDNS Hardening Specification

This spec defines how Zen Garden hardens and extends Koi-backed mDNS discovery and registration.

---

## Overview

Zen Garden uses Koi as a local mDNS proxy on Windows (and optionally on other platforms). This spec standardizes resilience, event hygiene, and optional integration points so discovery remains stable under daemon restarts, noisy networks, and transient failures.

## Goals

- Maintain mDNS registration continuity when Koi restarts or drops connections.
- Ensure discovery continues through SSE disruptions using safe fallbacks.
- Reduce duplicate or noisy events before they reach higher layers.
- Provide operators with observable health signals and tunable behavior.

## Non-goals

- Replacing native mDNS on platforms that already use `mdns-sd`.
- Persisting discovery state across restarts.
- Enforcing security policy for Koi capabilities outside mDNS.

## Architecture

Koi integration is split into two flows that share a single client:

- **Registration flow**: register, heartbeat, reconcile, and unregister.
- **Discovery flow**: SSE stream, event parsing, dedupe cache, fallback browse.

```
Zen Garden
  KoiClient
    Registration manager
    Discovery manager
        SSE stream -> event parser -> dedupe cache -> broadcast
        Fallback browse -> event parser -> dedupe cache -> broadcast
Koi daemon (HTTP + SSE)
```

## Protocol / Behavior

### Endpoints

The Koi client uses these endpoints:

- `GET /healthz`
- `POST /v1/mdns/services`
- `DELETE /v1/mdns/services/{id}`
- `PUT /v1/mdns/services/{id}/heartbeat`
- `GET /v1/mdns/events?type={type}&idle_for={seconds}`
- `GET /v1/mdns/browse?type={type}&idle_for={seconds}`

### Registration flow

1. **Health probe**: the client probes `/healthz` before registration.
2. **Register**: on success, the client registers using `POST /v1/mdns/services` with `lease_secs` and a pinned `ip`.
3. **Heartbeat**: the client sends heartbeats at a fixed interval (default: 60s).
4. **Reconcile**: if heartbeat returns 404, the client re-registers immediately.
5. **Backoff**: connection errors trigger exponential backoff with a ceiling.
6. **Best-effort unregister**: on shutdown, the client calls `DELETE /v1/mdns/services/{id}`.

### Discovery flow

1. **SSE primary**: the client connects to `GET /v1/mdns/events` with `idle_for=0`.
2. **Event parsing**: only `resolved` events are accepted; `removed` is ignored by default.
3. **Dedupe cache**: events are deduped by `stone_id` or `(stone_name, endpoint)` with a TTL.
4. **Fallback browse**: if no resolved events arrive within the inactivity window, the client performs a short browse using `GET /v1/mdns/browse` and feeds results through the same parser and dedupe cache.
5. **Reconnect**: on SSE disconnect, reconnect with exponential backoff and reset backoff on clean closes.

### Event hygiene

- IP filtering only accepts LAN-routable IPv4 ranges (RFC 1918) and rejects loopback/link-local and Docker bridge.
- TXT records are required to include `stone_name` before discovery events propagate.
- Duplicate `resolved` events inside the dedupe TTL do not re-broadcast.

### Observability

The Koi integration emits structured logs with:

- current base URL
- reconnect count and last reconnect reason
- last resolved event timestamp
- last browse fallback timestamp
- current registration ID and heartbeat status

## Configuration

Configuration is provided via environment variables. Defaults match current behavior unless stated otherwise.

| Variable | Default | Purpose |
|----------|---------|---------|
| `KOI_HOST` | `localhost` | Koi hostname for HTTP/SSE. |
| `KOI_PORT` | `5641` | Koi HTTP port. |
| `KOI_MDNS_LEASE_SECS` | `120` | Registration lease length in seconds. |
| `KOI_MDNS_HEARTBEAT_SECS` | `60` | Heartbeat interval in seconds. |
| `KOI_MDNS_BACKOFF_MAX_SECS` | `30` | Max reconnect backoff for SSE and heartbeats. |
| `KOI_MDNS_IDLE_FOR_SECS` | `0` | SSE idle timeout (0 = infinite). |
| `KOI_MDNS_INACTIVITY_SECS` | `120` | No-resolved-event window before fallback browse. |
| `KOI_MDNS_BROWSE_IDLE_SECS` | `5` | Idle timeout for fallback browse stream. |
| `KOI_MDNS_DEDUPE_TTL_SECS` | `45` | Suppress duplicate resolved events within this window. |

## Examples

### Register and maintain a service

```json
POST /v1/mdns/services
{
  "name": "stone-01",
  "type": "_moss._tcp",
  "port": 7185,
  "ip": "192.168.1.10",
  "lease_secs": 120,
  "txt": {
    "stone_id": "abc123",
    "stone_name": "stone-01",
    "version": "0.5.0",
    "health": "ok"
  }
}
```

### Fallback browse when SSE is quiet

```
GET /v1/mdns/browse?type=_moss._tcp&idle_for=5
```

### SSE stream with resolved events

```
GET /v1/mdns/events?type=_moss._tcp&idle_for=0
```

```
data: {"event":"resolved","service":{"name":"stone-02","type":"_moss._tcp","host":"stone-02.local.","ip":"192.168.1.11","port":7185,"txt":{"stone_name":"stone-02"}}}
```

## References

- [Discovery spec](specs/discovery.md)
- [Koi technical spec](../../koi/TECHNICAL.md)
