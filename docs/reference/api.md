---
audience: [api-client, operator, maintainer]
doc_type: reference
status: current
last_verified: 2026-03-25
canonical: true
note: "Complete HTTP API documentation for Moss daemon v1."
---

# Zen Garden API Reference

**Version:** v1  
**Base Path:** `/api/v1`  
**Protocol:** HTTP/REST with JSON payloads  
**Date:** March 2026

## Overview

The Zen Garden API v1 provides **two layers** for managing containerized services across distributed stones:

1. **Offerings API** (`/api/v1/stone/offerings`) — Human-friendly, simplified responses for beginners and automation (90% use case)
2. **Services API** (`/api/v1/stone/services`) — Technical, detailed container-level operations for operators (10% use case)

Both layers access the same underlying resources but provide different views optimized for their audiences. See [API-V1-DUAL-LAYER-DESIGN.md](API-V1-DUAL-LAYER-DESIGN.md) for complete architecture details.

### API Selection Guide

**Use Offerings API when:**
- Building quick start tutorials or simple automation
- Creating beginner-friendly UIs
- You don't need container internals (IDs, volume paths, etc.)

**Use Services API when:**
- Debugging production issues or building operator dashboards
- Need technical details (port bindings, resource usage, health checks)
- Implementing advanced orchestration or monitoring

### Authentication

**Default:** None (open garden mode)  
**With Pond:** mTLS via koi-certmesh (ECDSA P-256 certificates). When pond is active, Moss binds HTTPS on port 7183 using certmesh-issued certificates.

### Response Format

All successful responses follow this structure:

```json
{
  "status": "success",
  "message": "Human-readable status message",
  "data": { /* endpoint-specific payload */ },
  "suggestions": ["Optional guidance messages"]
}
```

Error responses use standard HTTP status codes with:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable error description",
    "details": { /* optional context */ }
  }
}
```

### Quiet Mode

All endpoints respect the `X-Quiet: true` header, which suppresses suggestion generation. Useful for scripting and automation.

---

## Endpoint Index

### Offerings API (Human Layer)

**Catalog Operations:**
- `GET /api/v1/stone/offerings` - List offerings (available + installed)
- `GET /api/v1/stone/offerings?state=available` - Filter available offerings
- `GET /api/v1/stone/offerings?state=installed` - Filter planted offerings
- `GET /api/v1/stone/offerings/:name` - Get offering details
- `GET /api/v1/stone/offerings/:name/manifest` - Get offering YAML template
- `GET /api/v1/stone/offerings/search?q={query}` - Search with taxonomy
- `GET /api/v1/stone/offerings/inspect?image={ref}` - Inspect Docker image

**Lifecycle Operations:**
- `POST /api/v1/stone/offerings` - Plant offering (simplified install)
- `DELETE /api/v1/stone/offerings/:name` - Take away offering (uninstall)
- `POST /api/v1/stone/offerings/heal` - Heal garden (discover orphans)
- `POST /api/v1/stone/offerings/refresh` - Refresh catalog from disk

**Adoption & Borrowing:**
- `GET /api/v1/stone/offerings/adoptable` - List containers available for adoption
- `GET /api/v1/stone/offerings/adopted` - List adopted containers
- `GET /api/v1/stone/offerings/borrowed` - List borrowed services
- `POST /api/v1/stone/offerings/:offering/adopt` - Adopt container
- `DELETE /api/v1/stone/offerings/:offering/adopt` - Unadopt container
- `POST /api/v1/stone/offerings/borrow` - Borrow service from topology
- `DELETE /api/v1/stone/offerings/borrow/:name` - Return borrowed service

**Capabilities:**
- `GET /api/v1/stone/offerings/:name/capabilities` - List offering capabilities
- `POST /api/v1/stone/offerings/:name/capabilities` - Add capability
- `POST /api/v1/stone/offerings/:name/capabilities/refresh` - Refresh capabilities
- `DELETE /api/v1/stone/offerings/:name/capabilities/:capability` - Remove capability

### Services API (Technical Layer)

**Manifest Operations:**
- `GET /api/v1/stone/services/manifests` - List all service manifests
- `GET /api/v1/stone/services/:name/manifest` - Get specific manifest YAML

**Runtime Operations:**
- `GET /api/v1/stone/services` - List services (container-level details)
- `GET /api/v1/stone/services/:name` - Get service details (full technical view)
- `GET /api/v1/stone/services/:name/logs` - Stream logs (SSE)
- `POST /api/v1/stone/services` - Install (full Docker control)
- `POST /api/v1/stone/services/:name/restart` - Restart service
- `POST /api/v1/stone/services/:name/upgrade` - Upgrade service
- `POST /api/v1/stone/services/:name/cordon` - Mark non-schedulable
- `POST /api/v1/stone/services/:name/destroy` - Destroy service
- `POST /api/v1/stone/services/:name/reassign` - Reassign to another stone
- `DELETE /api/v1/stone/services/:name` - Uninstall service
- `GET /api/v1/stone/services/:name/env` - Read env vars + manageable list
- `PATCH /api/v1/stone/services/:name/env` - Set/delete env vars (allowlist)
- `GET /api/v1/stone/services/:name/capabilities` - Discover service capabilities
- `POST /api/v1/stone/services/reconcile` - Reconcile inventory
- `POST /api/v1/stone/services/refresh` - Refresh manifests
- `POST /api/v1/stone/services/refresh-capabilities` - Refresh all capabilities

### Stone Operations
- `GET /health` - Health check with component status
- `GET /api/v1/stone/capabilities` - Hardware and software capabilities
- `GET /api/v1/stone/metrics` - Prometheus metrics exposition
- `POST /api/v1/stone/upgrade` - Upgrade stone software

### Administrative API
- `POST /api/v1/admin/moss/shutdown` - Shutdown Moss daemon
- `POST /api/v1/admin/moss/take-root` - Escalate to root (Linux)
- `POST /api/v1/admin/stone/shutdown` - Shutdown stone
- `POST /api/v1/admin/stone/reboot` - Reboot stone
- `POST /api/v1/admin/stone/{name}/wake` - Wake-on-LAN

### Logs & Maintenance
- `GET /api/v1/stone/logs` - Recent daemon logs
- `GET /api/v1/stone/logs/stream` - Live log stream (SSE)
- `GET /api/v1/stone/maintenance/history` - Sweep reports
- `POST /api/v1/stone/maintenance/sweep` - Trigger immediate sweep

### Updates
- `GET /api/v1/stone/updates` - Pending updates
- `POST /api/v1/stone/updates/execute` - Execute updates
- `GET /api/v1/stone/updates/stream/:job_id` - SSE update stream

### Pond (Security / Trust)
- `POST /api/v1/pond/init` - Place keystone (create CA)
- `GET /api/v1/pond/status` - Pond status and membership
- `POST /api/v1/pond/join` - Join pond with TOTP code
- `POST /api/v1/pond/invite` - Open enrollment
- `POST /api/v1/pond/unlock` - Unlock CA after restart
- `DELETE /api/v1/pond` - Drain pond
- `DELETE /api/v1/pond/stones/:name` - Revoke stone
- `POST /api/v1/pond/promote` - Promote to standby CA
- `PUT /api/v1/pond/name` - Rename pond
- `GET /api/v1/pond/ca.pem` - Download CA certificate

### Events & Jobs
- `GET /api/v1/stone/presence/stream` - Stream all domain events (SSE) - unified presence stream
- `GET /api/v1/jobs` - List background jobs
- `GET /api/v1/jobs/:job_id` - Get job status and result

### Console
- `GET /api/v1/console/mode` - Get console mode
- `POST /api/v1/console/mode` - Set console mode

### Garden Operations (Orchestrated)
- `GET /api/v1/garden/services` - Find services across garden
- `GET /api/v1/garden/updates` - Garden-wide updates
- `POST /api/v1/garden/updates/execute` - Execute garden updates
- `GET /api/v1/garden/topology` - Aggregate topology
- `GET /api/v1/garden/tools` - List garden-wide tools
- `GET /api/v1/garden/tools/stream` - Tool changes (SSE)

**Total:** 77 endpoints (index above; see detailed sections below for request/response examples)

---

## Services API

Manage containerized services (offerings) on a stone.

### POST /api/v1/stone/services

**Create and start a service from an offering template.**

**Request:**
```json
{
  "offering": "mongodb",
  "name": "my-mongo",  // optional, defaults to offering name
  "ports": [],         // optional override
  "environment": {}    // optional override
}
```

**Response (201 Created):**
```json
{
  "status": "success",
  "message": "Service 'my-mongo' created from mongodb",
  "data": {
    "service_name": "my-mongo",
    "offering": "mongodb",
    "status": "running",
    "container_id": "abc123...",
    "ports": [
      {"container": 27017, "host": 27017, "protocol": "tcp"}
    ],
    "environment": {
      "MONGO_INITDB_ROOT_USERNAME": "admin",
      "MONGO_INITDB_ROOT_PASSWORD": "generated-password"
    }
  },
  "suggestions": [
    "Service is now accessible at localhost:27017",
    "Use 'garden-rake observe' to monitor service health"
  ]
}
```

**Errors:**
- `400` - Invalid offering name or missing template
- `409` - Service name already exists
- `500` - Docker daemon error or container start failure

**Edge Cases:**
- Port conflicts: Returns error with available port suggestions
- Image pull failures: Falls back to architecture-compatible image if available
- Resource constraints: Returns error if disk/memory thresholds exceeded
- Name collisions: Validates against existing zen-offering-* containers

**CLI Examples:**
```bash
garden-rake offer mongodb

# With quiet mode
garden-rake offer redis quietly
```

---

### GET /api/v1/stone/services

**List all services on the stone.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "services": [
      {
        "name": "mongodb",
        "offering": "mongodb",
        "status": "running",
        "container_id": "abc123...",
        "uptime_seconds": 3600,
        "ports": [{"container": 27017, "host": 27017}]
      }
    ]
  }
}
```

**Status Values:**
- `running` - Container is active
- `stopped` - Container exists but not running (rest mode)
- `restarting` - Container in restart loop
- `error` - Container in error state

**CLI Examples:**
```bash
garden-rake observe
```

---

### GET /api/v1/stone/services/:service

**Get detailed information about a specific service.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "name": "mongodb",
    "offering": "mongodb",
    "status": "running",
    "container_id": "abc123...",
    "image": "mongo:7.0",
    "created": "2026-01-17T10:30:00Z",
    "started": "2026-01-17T10:30:05Z",
    "ports": [
      {"container": 27017, "host": 27017, "protocol": "tcp"}
    ],
    "environment": {
      "MONGO_INITDB_ROOT_USERNAME": "admin"
    },
    "volumes": [
      {"name": "zen-vol-mongodb-data", "mount": "/data/db"}
    ]
  }
}
```

**Errors:**
- `404` - Service not found

**CLI Examples:**
```bash
garden-rake touch mongodb
```

---

### POST /api/v1/stone/services/:service/rest

**Stop a running service (rest mode).**

Stops the container but preserves volumes and configuration for quick restart.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' stopped",
  "data": {
    "service_name": "mongodb",
    "status": "stopped"
  },
  "suggestions": [
    "Service volumes preserved",
    "Use 'garden-rake wake mongodb' to restart"
  ]
}
```

**Errors:**
- `404` - Service not found
- `409` - Service already stopped

**Edge Cases:**
- Graceful shutdown timeout: 10 seconds before force-stop
- Active connections: Logs warning but proceeds with shutdown
- Volume cleanup: Volumes preserved unless service is removed

**CLI Examples:**
```bash
garden-rake rest mongodb
```

---

### POST /api/v1/stone/services/:service/wake

**Start a stopped service (wake from rest).**

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' started",
  "data": {
    "service_name": "mongodb",
    "status": "running"
  },
  "suggestions": [
    "Service restored from rest mode",
    "All volumes and configuration preserved"
  ]
}
```

**Errors:**
- `404` - Service not found
- `409` - Service already running
- `500` - Container start failure

**CLI Examples:**
```bash
garden-rake wake mongodb
```

---

### POST /api/v1/stone/services/:service/upgrade

**Upgrade service to latest image version.**

Pulls latest image, stops container, recreates with same config, preserves volumes.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' upgraded",
  "data": {
    "service_name": "mongodb",
    "old_image": "mongo:7.0",
    "new_image": "mongo:7.0.5",
    "status": "running"
  },
  "suggestions": [
    "Service upgraded with zero data loss",
    "Verify application compatibility with new version"
  ]
}
```

**Errors:**
- `404` - Service not found
- `500` - Image pull failure or recreation error

**Edge Cases:**
- Image unchanged: Returns success with note "already up to date"
- Downtime: Brief interruption during container recreation
- Volume compatibility: Upgrade may fail if new version incompatible with existing data
- Rollback: Manual process - no automatic rollback on failure

**CLI Examples:**
```bash
garden-rake nourish mongodb
garden-rake nourish --all  # upgrade all services
```

---

### DELETE /api/v1/stone/services/:service

**Remove service and its containers (volumes preserved).**

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' removed",
  "data": {
    "service_name": "mongodb",
    "volumes_preserved": true
  },
  "suggestions": [
    "Service removed but volumes preserved",
    "Volumes: zen-vol-mongodb-data",
    "Use 'docker volume rm' to remove volumes if needed"
  ]
}
```

**Errors:**
- `404` - Service not found
- `500` - Container removal failure

**Edge Cases:**
- Volume preservation: Volumes kept by design for data safety
- Registry cleanup: Service removed from Moss registry immediately
- Container force-remove: Uses force flag if graceful stop fails
- Orphaned volumes: Listed in suggestions for manual cleanup

**CLI Examples:**
```bash
garden-rake release mongodb
```

---

### POST /api/v1/stone/services/:service/destroy

**Destroy a service, including its volumes and configuration.**

Unlike `DELETE` which preserves volumes, destroy performs a full cleanup.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' destroyed",
  "data": {
    "service_name": "mongodb",
    "volumes_removed": ["zen-vol-mongodb-data"],
    "config_removed": true
  }
}
```

**Errors:**
- `404` - Service not found
- `500` - Cleanup failure

---

### POST /api/v1/stone/services/:service/reassign

**Reassign a service to another stone in the garden.**

Migrates the service definition (and optionally data) to a target stone.

**Request:**
```json
{
  "target_stone": "stone-02"
}
```

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' reassigned to stone-02",
  "data": {
    "service_name": "mongodb",
    "source_stone": "stone-01",
    "target_stone": "stone-02"
  }
}
```

**Errors:**
- `404` - Service or target stone not found
- `409` - Target stone already has this service
- `503` - Target stone unreachable

---

### POST /api/v1/stone/services/:service/cordon

**Mark a service as non-schedulable.**

Prevents the service from being included in scheduling decisions while remaining running.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Service 'mongodb' cordoned",
  "data": {
    "service_name": "mongodb",
    "cordoned": true
  }
}
```

**Errors:**
- `404` - Service not found

---

### GET /api/v1/stone/services/:service/env

**Read environment variables for a service.**

Returns current environment variables and the list of manageable (user-configurable) variables.

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "environment": {
      "MONGO_INITDB_ROOT_USERNAME": "admin",
      "MONGO_INITDB_ROOT_PASSWORD": "***"
    },
    "manageable": ["MONGO_INITDB_ROOT_USERNAME", "MONGO_INITDB_ROOT_PASSWORD"]
  }
}
```

**Errors:**
- `404` - Service not found

---

### PATCH /api/v1/stone/services/:service/env

**Set or delete environment variables for a service.**

Only variables in the manageable allowlist can be modified. Service is restarted automatically after changes.

**Request:**
```json
{
  "set": {
    "MONGO_INITDB_ROOT_PASSWORD": "new-password"
  },
  "delete": ["OLD_UNUSED_VAR"]
}
```

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Environment updated for 'mongodb'",
  "data": {
    "set": ["MONGO_INITDB_ROOT_PASSWORD"],
    "deleted": ["OLD_UNUSED_VAR"],
    "restarted": true
  }
}
```

**Errors:**
- `400` - Variable not in manageable allowlist
- `404` - Service not found

---

### GET /api/v1/stone/services/:service/capabilities

**Discover capabilities exposed by a service.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "service": "ollama",
    "capabilities": [
      {"name": "chat", "type": "inference"},
      {"name": "embedding", "type": "inference"}
    ]
  }
}
```

**Errors:**
- `404` - Service not found

---

### POST /api/v1/stone/services/refresh-capabilities

**Refresh capabilities for all services.**

Re-discovers capabilities by probing all running services.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Capabilities refreshed",
  "data": {
    "services_scanned": 5,
    "capabilities_found": 12
  }
}
```

---

## Garden API

Observe multi-stone topology and service distribution.

### GET /api/v1/garden

**Get garden overview with all discovered stones.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "stones": [
      {
        "name": "stone-01",
        "endpoint": "http://192.168.1.100:7185",
        "status": "healthy",
        "services": [
          {
            "name": "mongodb",
            "offering": "mongodb",
            "status": "running"
          }
        ],
        "capabilities": {
          "arch": "x86_64",
          "platform": "linux",
          "docker_version": "24.0.7"
        }
      }
    ]
  }
}
```

**CLI Examples:**
```bash
garden-rake observe
garden-rake observe --offering mongodb  # filter by offering
```

---

### GET /api/v1/garden/stones/:stone_name

**Get detailed information about a specific stone.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "name": "stone-01",
    "endpoint": "http://192.168.1.100:7185",
    "status": "healthy",
    "services": [...],
    "capabilities": {
      "arch": "x86_64",
      "platform": "linux",
      "cpu_cores": 8,
      "memory_gb": 16.0,
      "docker_version": "24.0.7"
    },
    "resources": {
      "disk": {
        "total_gb": 500.0,
        "available_gb": 350.0,
        "used_percent": 30.0
      },
      "memory": {
        "total_gb": 16.0,
        "available_gb": 8.0,
        "used_percent": 50.0
      }
    }
  }
}
```

**Errors:**
- `404` - Stone not found in garden topology

---

### GET /api/v1/stone

**Get local stone information (self-introspection).**

Returns the same data as `/garden/stones/:stone_name` but always for the local stone where the request is made.

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "name": "stone-01",
    "endpoint": "http://localhost:7185",
    "services": [...],
    "capabilities": {...},
    "resources": {...}
  }
}
```

---

## Pond Security API

**Status:** Implemented — backed by koi-certmesh (ECDSA P-256 CA, TOTP enrollment, mTLS)

Pond endpoints manage the certificate authority lifecycle, stone enrollment, and trust operations. All handlers delegate to the embedded koi-certmesh subsystem.

### Error Format

All error responses use a standard envelope:

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable description",
    "details": {}
  }
}
```

**Shared Error Codes:**

| HTTP | Code | Meaning |
|------|------|---------|
| 401 | `INVALID_AUTH` | Wrong passphrase or TOTP code |
| 403 | `ENROLLMENT_CLOSED` | No active enrollment window |
| 403 | `APPROVAL_DENIED` | Operator denied enrollment |
| 403 | `REVOKED` | Stone already revoked |
| 404 | `NOT_FOUND` | Resource not found |
| 409 | `POND_NOT_INITIALIZED` | CA not created yet |
| 409 | `ALREADY_ENROLLED` | Stone hostname already enrolled |
| 423 | `POND_LOCKED` | CA key encrypted (needs unlock after restart) |
| 429 | `RATE_LIMITED` | Too many failed auth attempts |
| 500 | `CERTMESH_ERROR` | Internal certmesh failure |
| 503 | `CERTMESH_UNAVAILABLE` | Certmesh subsystem not loaded |

---

### POST /api/v1/pond/init

**Initialize pond security — create CA and place keystone.**

Creates a new ECDSA P-256 certificate authority, encrypts the private key with the given passphrase, and designates this stone as the cornerstone.

**Request:**
```json
{
  "passphrase": "secure-passphrase",
  "profile": "just-me"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `passphrase` | Yes | Encrypts the CA private key |
| `profile` | No | Trust profile: `just-me` (default), `my-team`, `my-organization` |
| `name` | No | Pond display name (`pond-{adj}-{noun}` format). Auto-generated if omitted. |

**Response (200 OK):**
```json
{
  "data": {
    "cornerstone": "stone-coral-prairie",
    "name": "pond-moonlit-basin",
    "keystone_path": "/var/lib/zen-garden/certmesh/ca",
    "certificate_expires": "30 days",
    "status": "active",
    "totp_uri": "otpauth://totp/certmesh:stone-coral-prairie?secret=BASE32&issuer=certmesh&algorithm=SHA1&digits=6&period=30",
    "ca_fingerprint": "AB:CD:EF:01:23:45:67:89..."
  }
}
```

**Errors:** `400 INVALID_PROFILE`, `400 INVALID_POND_NAME`, `500 CA_CREATION_FAILED`, `503 CERTMESH_UNAVAILABLE`

**CLI Examples:**
```bash
garden-rake place keystone --passphrase "my-secure-pass"
```

---

### GET /api/v1/pond/status

**Get pond security status and membership.**

Returns CA state, enrolled stones, trust profile, and enrollment window status.

**Response (200 OK) — not initialized:**
```json
{
  "data": {
    "active": false,
    "locked": false,
    "cornerstone": null,
    "name": null,
    "stones": [],
    "profile": "JustMe",
    "ca_fingerprint": null,
    "enrollment_state": "Closed"
  }
}
```

**Response (200 OK) — active:**
```json
{
  "data": {
    "active": true,
    "locked": false,
    "cornerstone": "stone-coral-prairie",
    "name": "pond-moonlit-basin",
    "stones": [
      {
        "name": "stone-coral-prairie",
        "role": "primary",
        "status": "active",
        "certificate_expires": "2026-03-16T12:00:00Z",
        "joined_at": null
      }
    ],
    "profile": "MyTeam",
    "ca_fingerprint": "AB:CD:EF:01:23:45:67:89...",
    "enrollment_state": "Open"
  }
}
```

**CLI Examples:**
```bash
garden-rake pond status
```

---

### POST /api/v1/pond/invite

**Open enrollment window and generate TOTP URI for stone admission.**

Rotates the TOTP secret and opens enrollment for the specified TTL. The returned URI can be shared with stones that need to join.

**Request:**
```json
{
  "passphrase": "secure-passphrase",
  "ttl_minutes": 30
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `passphrase` | Yes | CA passphrase to authorize the operation |
| `ttl_minutes` | No | Enrollment window duration (default: 30) |

**Response (200 OK):**
```json
{
  "data": {
    "totp_uri": "otpauth://totp/certmesh:stone-coral-prairie?secret=BASE32&issuer=certmesh&algorithm=SHA1&digits=6&period=30",
    "expires_at": "2026-02-14T12:57:00+00:00",
    "ttl_seconds": 1800,
    "inviter_stone": "stone-coral-prairie",
    "enrollment_state": "open"
  }
}
```

**Errors:** `401 INVALID_AUTH`, `409 POND_NOT_INITIALIZED`, `423 POND_LOCKED`

**CLI Examples:**
```bash
garden-rake invite --passphrase "my-secure-pass"
```

---

### POST /api/v1/pond/join

**Join pond using a TOTP code.**

Validates the TOTP code against the cornerstone's enrollment secret. On success, issues a certificate for the joining stone.

**Request:**
```json
{
  "code": "123456",
  "hostname": "stone-02",
  "sans": ["192.168.1.50"]
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `code` | Yes | 6-digit TOTP code from the invite URI |
| `hostname` | No | Override hostname (auto-detected if omitted) |
| `sans` | No | Additional Subject Alternative Names |

**Response (200 OK):**
```json
{
  "data": {
    "stone_name": "stone-02",
    "cornerstone": "stone-coral-prairie",
    "certificate_expires": "30 days",
    "status": "active",
    "ca_fingerprint": "AB:CD:EF:01:23:45:67:89..."
  }
}
```

**Errors:**
- `401 INVALID_AUTH` — wrong or expired TOTP code
- `403 ENROLLMENT_CLOSED` — no active enrollment window
- `409 ALREADY_ENROLLED` — stone hostname already in roster

**CLI Examples:**
```bash
garden-rake place stone --code 123456
```

---

### POST /api/v1/pond/unlock

**Unlock the CA after a Moss restart.**

After a Moss restart, the CA private key is locked (encrypted at rest). This endpoint decrypts it so pond operations can resume.

**Request:**
```json
{
  "passphrase": "secure-passphrase"
}
```

**Response (200 OK):**
```json
{
  "data": {
    "unlocked": true
  }
}
```

**Errors:** `401 INVALID_AUTH`, `409 POND_NOT_INITIALIZED`, `429 RATE_LIMITED`

**CLI Examples:**
```bash
garden-rake pond unlock --passphrase "my-secure-pass"
```

---

### POST /api/v1/pond/promote

**Promote this stone to standby CA (receive CA key material).**

Copies the CA private key material to this stone so it can act as a backup certificate authority.

**Request:**
```json
{
  "passphrase": "secure-passphrase"
}
```

**Response (200 OK):**
```json
{
  "data": {
    "promoted": true,
    "ca_fingerprint": "AB:CD:EF:01:23:45:67:89..."
  }
}
```

**Errors:** `401 INVALID_AUTH`, `409 POND_NOT_INITIALIZED`, `423 POND_LOCKED`

**CLI Examples:**
```bash
garden-rake pond promote --passphrase "my-secure-pass"
```

---

### GET /api/v1/pond/ca.pem

**Download the CA public certificate.**

Returns the PEM-encoded CA certificate for manual trust installation.

**Response (200 OK):**
```
Content-Type: application/x-pem-file

-----BEGIN CERTIFICATE-----
MIIBxTCCAW...
-----END CERTIFICATE-----
```

**Errors:** `404 POND_NOT_INITIALIZED`, `500 CA_READ_ERROR`

**CLI Examples:**
```bash
curl http://stone:7185/api/v1/pond/ca.pem -o ca.pem
```

---

### PUT /api/v1/pond/name

**Rename the pond.**

Changes the pond's display name. Accepts a specific name in `pond-{adjective}-{noun}` format, or omit/empty to auto-generate a new one. Pond names are decorative — renaming has no effect on certificates or enrollment.

**Request:**
```json
{
  "name": "pond-glacial-heron"
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | No | New name in `pond-{adj}-{noun}` format. Omit or send empty string to auto-generate. |

**Response (200 OK):**
```json
{
  "data": {
    "name": "pond-glacial-heron"
  }
}
```

**Errors:** `400 INVALID_POND_NAME`, `409 POND_NOT_INITIALIZED`

**CLI Examples:**
```bash
garden-rake pond rename pond-glacial-heron       # Set specific name
garden-rake pond rename                           # Auto-generate new name
```

---

### DELETE /api/v1/pond

**Drain the pond — destroy the CA and all certificates.**

Removes the certificate authority, all issued certificates, and reverts the stone to unauthenticated mode.

**Response (200 OK):**
```json
{
  "data": {
    "destroyed": true
  }
}
```

**CLI Examples:**
```bash
garden-rake lift keystone
```

---

### DELETE /api/v1/pond/stones/:stone_name

**Revoke a stone's certificate (untrust).**

Revokes the named stone's certificate and removes it from the pond roster.

**Response (200 OK):**
```json
{
  "data": {
    "revoked": true,
    "stone_name": "stone-02"
  }
}
```

**Errors:**
- `404 NOT_FOUND` — stone not in pond roster
- `403 REVOKED` — stone already revoked
- `409 POND_NOT_INITIALIZED` — CA not created
- `423 POND_LOCKED` — CA locked after restart

**CLI Examples:**
```bash
garden-rake lift stone stone-02
```

---

## Health & Capabilities

### GET /health

**Health check endpoint for load balancers and monitoring.**

**Response (200 OK or 503):**
```json
{
  "status": "healthy",
  "timestamp": "2026-01-17T10:30:00Z",
  "components": {
    "docker": {
      "status": "healthy",
      "details": {
        "available": true
      }
    },
    "disk": {
      "status": "healthy",
      "details": {
        "free_gb": "350.0",
        "total_gb": "500.0",
        "usage_percent": "30.00"
      }
    },
    "memory": {
      "status": "healthy",
      "details": {
        "available_gb": "8.0",
        "total_gb": "16.0",
        "usage_percent": "50.00"
      }
    }
  },
  "docker_available": true,
  "disk_space_ok": true,
  "memory_ok": true,
  "uptime_seconds": 86400
}
```

**Health Status Values:**
- `healthy` - All components operational, <80% resource usage
- `degraded` - Functional but 80-95% resource usage
- `unhealthy` - Critical issues, >95% resources or component failure

**HTTP Status Codes:**
- `200` - Healthy or degraded
- `503` - Unhealthy (should not receive traffic)

---

### GET /api/v1/stone/capabilities

**Get stone hardware and software capabilities.**

**Response (200 OK):**
```json
{
  "stone_name": "stone-01",
  "arch": "x86_64",
  "platform": "linux",
  "os": "Ubuntu 22.04.3 LTS",
  "kernel": "5.15.0-91-generic",
  "cpu_cores": 8,
  "memory_gb": 16.0,
  "docker_version": "24.0.7",
  "docker_api_version": "1.43",
  "offerings_count": 47
}
```

**Used For:**
- Compatibility decisions (architecture matching)
- Service placement recommendations
- Resource-aware scheduling
- Garden topology display

---

### GET /api/v1/stone/metrics

**Prometheus-compatible metrics endpoint.**

Returns stone resource metrics in Prometheus exposition format for monitoring and alerting.

**Response (200 OK - text/plain):**
```
# HELP zen_stone_disk_free_bytes Free disk space in bytes
# TYPE zen_stone_disk_free_bytes gauge
zen_stone_disk_free_bytes{stone="stone-01"} 376000000000

# HELP zen_stone_memory_available_bytes Available memory in bytes
# TYPE zen_stone_memory_available_bytes gauge
zen_stone_memory_available_bytes{stone="stone-01"} 8589934592

# HELP zen_stone_services_total Total number of services
# TYPE zen_stone_services_total gauge
zen_stone_services_total{stone="stone-01"} 3
```

**Metrics Exposed:**
- `zen_stone_disk_free_bytes` - Available disk space
- `zen_stone_disk_total_bytes` - Total disk space
- `zen_stone_memory_available_bytes` - Available RAM
- `zen_stone_memory_total_bytes` - Total RAM
- `zen_stone_services_total` - Service count
- `zen_stone_services_running` - Running services
- `zen_stone_uptime_seconds` - Moss daemon uptime

**Integration:**
- Prometheus scrape target
- Grafana dashboards
- AlertManager rules
- Custom monitoring tools

---

## Offerings Catalog

Validated offering templates with architecture compatibility analysis.

### GET /api/v1/stone/offerings

**List validated offerings with compatibility decisions.**

Returns all offering templates analyzed for this stone's architecture with automatic fallback recommendations.

**Response (200 OK):**
```json
{
  "offerings": [
    {
      "name": "mongodb",
      "category": "databases",
      "description": "MongoDB NoSQL database",
      "tags": ["nosql", "database", "document-store"],
      "image": "mongo:7.0",
      "ports": [
        {"container": 27017, "host": 27017, "protocol": "tcp"}
      ],
      "environment": {
        "MONGO_INITDB_ROOT_USERNAME": "admin",
        "MONGO_INITDB_ROOT_PASSWORD": "${GENERATED}"
      },
      "volumes": [
        {"name": "${SERVICE}-data", "mount": "/data/db"}
      ],
      "compatibility": {
        "decision": "pass",
        "reason": null,
        "original_image": null,
        "fallback_image": null,
        "suggestion": null
      }
    },
    {
      "name": "postgres",
      "category": "databases",
      "description": "PostgreSQL relational database",
      "tags": ["sql", "database", "relational"],
      "image": "postgres:16",
      "compatibility": {
        "decision": "fallback",
        "reason": "Architecture mismatch: amd64 image on arm64 host",
        "original_image": "postgres:16",
        "fallback_image": "postgres:16-alpine",
        "suggestion": "Will use ARM-compatible Alpine image"
      }
    }
  ]
}
```

**Compatibility Decisions:**
- `pass` - Image compatible with stone architecture
- `fallback` - Architecture-compatible fallback available
- `block` - No compatible image available for this architecture

**CLI Examples:**
```bash
garden-rake explore
```

---

### GET /api/v1/stone/offerings/:name

**Get detailed offering info with compatibility analysis.**

Returns single offering template with full compatibility analysis for this specific stone.

**Response (200 OK):**
```json
{
  "name": "mongodb",
  "category": "databases",
  "description": "MongoDB NoSQL database",
  "tags": ["nosql", "database"],
  "image": "mongo:7.0",
  "ports": [...],
  "environment": {...},
  "volumes": [...],
  "compatibility": {
    "decision": "pass",
    "reason": null,
    "original_image": null,
    "fallback_image": null,
    "suggestion": null
  }
}
```

**Errors:**
- `404` - Offering template not found

**CLI Examples:**
```bash
garden-rake offer mongodb info
```

---

### POST /api/v1/stone/offerings/refresh

**Force rebuild of offerings index cache.**

Triggers immediate recompilation of all offering templates with compatibility decisions. Normally done automatically on template changes.

**Response (200 OK):**
```json
{
  "message": "Offerings index refreshed",
  "count": 47,
  "generated_at": "2026-01-17T10:30:00Z"
}
```

**Use Cases:**
- After manual template modifications
- Testing compatibility fallback logic
- Debugging template issues
- Development workflow

---

## Real-Time Streams (SSE)

Server-Sent Events for logs and system events.

### GET /api/v1/stone/services/:service/logs

**Stream service container logs in real-time.**

Returns Server-Sent Events stream of container stdout/stderr.

**Query Parameters:**
- `timestamps=true` - Include ISO8601 timestamps
- `tail=100` - Number of historical lines to include (default: all)

**Response (200 OK - text/event-stream):**
```
data: 2026-01-17T10:30:00Z Starting MongoDB...
data: 2026-01-17T10:30:01Z Listening on port 27017
data: 2026-01-17T10:30:02Z Waiting for connections
```

**Errors:**
- `404` - Service not found
- `500` - Docker logging driver not compatible

**Edge Cases:**
- Container restart: Stream continues with new container logs
- Service removed: Stream closes with error event
- Network timeout: Client should reconnect

**CLI Examples:**
```bash
garden-rake watch offering mongodb logs
garden-rake watch offering mongodb logs --timestamps
```

---

### GET /api/v1/stone/presence/stream

**Stream all domain events in real-time (unified presence stream).**

Server-Sent Events stream of all stone activity: service lifecycle, storage events, stone health, and job progress. This is the single source of truth for real-time stone state, used by Companions (Cricket, Firefly) and the portrait page.

**Query Parameters:**
- `companion=<name>` - Optional Companion identification (e.g., `cricket`, `firefly`)
- `version=<ver>` - Optional Companion version
- `categories=<list>` - Optional filter (e.g., `service,storage,job`)

**Response (200 OK - text/event-stream):**
```
event: presence.snapshot
data: {"stone": {"name": "stone-01", "health": "thriving"}, "services": [...], "timestamp": "..."}

event: service.started
data: {"service": "mongodb", "offering_id": "...", "timestamp": "2026-01-17T10:30:05Z"}

event: job.started
data: {"job_id": "abc123", "offering": "mongodb", "operation": "install", "timestamp": "..."}

event: job.progress
data: {"job_id": "abc123", "offering": "mongodb", "message": "Pulling image...", "level": "info"}

event: storage.detected
data: {"name": "seed-usb-01", "device": "/dev/sdb1", "capacity_gb": 500}

event: presence.heartbeat
data: {"stone_health": "thriving", "service_count": 3, "timestamp": "..."}
```

**Event Categories:**

| Category | Events |
|----------|--------|
| **service** | `service.started`, `service.stopped`, `service.updated`, `service.health.changed`, `service.renamed` |
| **storage** | `storage.detected`, `storage.removed`, `storage.sync.started`, `storage.sync.completed` |
| **stone** | `stone.tended`, `stone.health.changed`, `stone.load.updated` |
| **job** | `job.started`, `job.progress`, `job.completed`, `job.failed` |

**CLI Examples:**
```bash
curl -N http://localhost:7185/api/v1/stone/presence/stream
garden-rake presence watch
```

**Note:** The former `/api/v1/events` endpoint has been consolidated into this unified stream.
All job progress events now flow through the presence stream alongside other domain events.

---

## Async Jobs (Reserved)

Job tracking infrastructure for future long-running operations.

### GET /api/v1/jobs

**List all async jobs.**

Returns list of background jobs and their statuses.

**Response (200 OK):**
```json
{
  "jobs": []
}
```

**Note:** Currently unused - reserved for future async operations like multi-stone upgrades or backup/restore.

---

### GET /api/v1/jobs/:job_id

**Get async job status.**

Check status of specific background operation.

**Response (200 OK):**
```json
{
  "job_id": "uuid-here",
  "status": "completed",
  "result": {...}
}
```

**Status Values:**
- `pending` - Job queued
- `running` - Job in progress
- `completed` - Job finished successfully
- `failed` - Job encountered error

**Errors:**
- `404` - Job not found

**Note:** Infrastructure in place but no jobs currently use this system.

---

## Discovery

Multi-stone network discovery via mDNS.

### GET /api/v1/stone/peer-stones

**Query discovered stones in the garden.**

Returns list of stones discovered via mDNS/Avahi, simpler format than `/api/v1/garden`.

**Response (200 OK):**
```json
{
  "stones": [
    {
      "name": "stone-01",
      "endpoint": "http://192.168.1.100:7185"
    },
    {
      "name": "stone-02",
      "endpoint": "http://192.168.1.101:7185"
    }
  ]
}
```

**Use Cases:**
- Quick stone discovery
- Network topology visualization
- Load balancing stone selection

---

## System Administration

### POST /api/v1/stone/services/reconcile

**Reconcile Moss registry with existing containers.**

Adopts orphaned zen-offering-* containers and optionally removes invalid ones.

**Request:**
```json
{
  "drop_invalid": false
}
```

**Response (200 OK):**
```json
{
  "adopted": ["mongodb", "redis"],
  "dropped_invalid": [],
  "left_unregistered": [],
  "message": "Reconciliation complete: 2 adopted, 0 dropped"
}
```

**Use Cases:**
- After Moss restart or upgrade
- Recovering from registry corruption
- Cleaning up manual container operations
- Migration from external container management

**CLI Examples:**
```bash
garden-rake reconcile
garden-rake reconcile --drop-invalid
```

---

### POST /api/v1/stone/refresh

**Refresh Moss or Rake binary on stone.**

Upload and replace garden-moss or garden-rake binary. Moss automatically restarts after update.

**Request (multipart/form-data):**
- `component` - "moss" or "rake"
- `binary` - Binary file upload

**Response (200 OK):**
```json
{
  "message": "Component refreshed successfully",
  "component": "moss",
  "version": "0.1.0",
  "restart_required": true
}
```

**Security Notes:**
- Development/testing use only
- No authentication (Phase 1-3)
- Validates binary architecture before installation
- Automatic Moss restart after upload

**CLI Examples:**
```bash
garden-rake refresh moss --from ./target/release/garden-moss
garden-rake refresh rake --from ./dist/linux-x64/garden-rake
```

---

### GET /api/v1/stone/services/:name/sources

**Get template image sources and fallbacks.**

Returns list of available image sources for an offering template, including architecture-specific fallbacks.

**Response (200 OK):**
```json
{
  "template": "postgres",
  "sources": [
    {
      "image": "postgres:16",
      "arch": "amd64",
      "compatible": false
    },
    {
      "image": "postgres:16-alpine",
      "arch": "arm64",
      "compatible": true
    }
  ]
}
```

**Use Cases:**
- Debugging compatibility decisions
- Understanding architecture fallback strategy
- Template development and testing

---

### PUT /api/v1/stone/services/:name/compatibility

**Override compatibility decision for template.**

Manually set compatibility fallback image for a template (development/testing).

**Request:**
```json
{
  "fallback_image": "postgres:16-alpine"
}
```

**Response (200 OK):**
```json
{
  "message": "Compatibility override applied",
  "template": "postgres",
  "fallback_image": "postgres:16-alpine"
}
```

**Warning:** Manual overrides bypass automatic compatibility detection.

---

## Administrative Endpoints

### POST /api/v1/admin/moss/shutdown

**Gracefully shutdown Moss daemon.**

Stops all services and shuts down the Moss HTTP server.

**Response (200 OK):**
```json
{
  "message": "Shutting down Moss daemon",
  "services_stopped": 3
}
```

**Behavior:**
- Stops all running services
- Closes HTTP server
- Exits process with code 0

**Security:** No authentication on HTTP. When pond is active, use HTTPS (:7183) for authenticated access.

---

### POST /api/v1/admin/moss/take-root

**Escalate Moss daemon to root privileges (Linux only).**

Re-executes Moss under root to enable privileged operations (storage mounting, firmware updates).

**Response (200 OK):**
```json
{
  "message": "Escalating to root",
  "pid": 12345
}
```

**Errors:**
- `403` - Already running as root
- `500` - Escalation failed (sudo not available)

---

### POST /api/v1/admin/stone/shutdown

**Shutdown the stone (power off).**

Gracefully shuts down the Moss daemon, then issues a system power-off command.

**Response (200 OK):**
```json
{
  "message": "Stone shutting down"
}
```

**Errors:**
- `500` - Shutdown command failed

---

### POST /api/v1/admin/stone/reboot

**Reboot the stone.**

Gracefully shuts down the Moss daemon, then issues a system reboot command.

**Response (200 OK):**
```json
{
  "message": "Stone rebooting"
}
```

**Errors:**
- `500` - Reboot command failed

---

### POST /api/v1/admin/stone/{name}/wake

**Send Wake-on-LAN packet to a stone.**

Sends a WoL magic packet to wake a powered-off stone on the local network.

**Request:**
```json
{
  "mac": "AA:BB:CC:DD:EE:FF"
}
```

**Response (200 OK):**
```json
{
  "message": "Wake-on-LAN packet sent",
  "target": "stone-02"
}
```

**Errors:**
- `400` - Invalid MAC address
- `404` - Stone not found in topology

---

## Adoption & Borrowing

### GET /api/v1/stone/offerings/adoptable

**List containers available for adoption.**

Returns containers running on the stone that match the `zen-offering-*` naming convention but are not yet tracked by Moss.

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "adoptable": [
      {
        "container_name": "zen-offering-redis",
        "image": "redis:7",
        "status": "running",
        "created": "2026-01-10T08:00:00Z"
      }
    ]
  }
}
```

---

### GET /api/v1/stone/offerings/adopted

**List adopted containers.**

Returns containers that were adopted into Moss management (not originally planted through the API).

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "adopted": [
      {
        "name": "redis",
        "container_name": "zen-offering-redis",
        "adopted_at": "2026-01-10T09:00:00Z"
      }
    ]
  }
}
```

---

### GET /api/v1/stone/offerings/borrowed

**List borrowed services.**

Returns services borrowed from other stones in the garden topology.

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "borrowed": [
      {
        "name": "mongodb",
        "source_stone": "stone-02",
        "endpoint": "http://192.168.1.101:27017"
      }
    ]
  }
}
```

---

### POST /api/v1/stone/offerings/:offering/adopt

**Adopt an existing container into Moss management.**

Takes ownership of a `zen-offering-*` container that is running but not tracked.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Container adopted as 'redis'",
  "data": {
    "name": "redis",
    "container_name": "zen-offering-redis",
    "status": "running"
  }
}
```

**Errors:**
- `404` - Container not found
- `409` - Already managed by Moss

---

### DELETE /api/v1/stone/offerings/:offering/adopt

**Unadopt a container (release from Moss management).**

Removes the container from Moss tracking without stopping or deleting it.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Container 'redis' released from management"
}
```

**Errors:**
- `404` - Offering not found or not adopted

---

### POST /api/v1/stone/offerings/borrow

**Borrow a service from another stone in the topology.**

Creates a proxy reference to a service running on a remote stone.

**Request:**
```json
{
  "name": "mongodb",
  "source_stone": "stone-02"
}
```

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Borrowed 'mongodb' from stone-02",
  "data": {
    "name": "mongodb",
    "source_stone": "stone-02",
    "endpoint": "http://192.168.1.101:27017"
  }
}
```

**Errors:**
- `404` - Service or stone not found
- `409` - Already borrowed or locally installed

---

### DELETE /api/v1/stone/offerings/borrow/:name

**Return a borrowed service.**

Removes the proxy reference to the remote service.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Returned borrowed service 'mongodb'"
}
```

**Errors:**
- `404` - Borrowed service not found

---

## Offering Capabilities

### GET /api/v1/stone/offerings/:name/capabilities

**List capabilities exposed by an offering.**

Returns discovered capabilities (HTTP endpoints, protocols, tools) for the offering.

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "offering": "ollama",
    "capabilities": [
      {
        "name": "chat",
        "type": "inference",
        "endpoint": "/api/chat"
      }
    ]
  }
}
```

**Errors:**
- `404` - Offering not found

---

### POST /api/v1/stone/offerings/:name/capabilities

**Add a capability to an offering.**

Manually register a capability for an offering.

**Request:**
```json
{
  "name": "embedding",
  "type": "inference",
  "endpoint": "/api/embeddings"
}
```

**Response (201 Created):**
```json
{
  "status": "success",
  "message": "Capability 'embedding' added to ollama"
}
```

**Errors:**
- `404` - Offering not found
- `409` - Capability already exists

---

### POST /api/v1/stone/offerings/:name/capabilities/refresh

**Refresh capabilities for an offering.**

Re-discovers capabilities by probing the running service.

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Capabilities refreshed for ollama",
  "data": {
    "discovered": 4
  }
}
```

**Errors:**
- `404` - Offering not found
- `503` - Service not running

---

### DELETE /api/v1/stone/offerings/:name/capabilities/:capability

**Remove a capability from an offering.**

**Response (200 OK):**
```json
{
  "status": "success",
  "message": "Capability 'embedding' removed from ollama"
}
```

**Errors:**
- `404` - Offering or capability not found

---

## Logs & Maintenance

### GET /api/v1/stone/logs

**Retrieve recent Moss daemon log lines.**

**Query Parameters:**
- `lines=100` - Number of recent lines (default: 100)
- `level=warn` - Minimum log level filter (`trace`, `debug`, `info`, `warn`, `error`)

**Response (200 OK):**
```json
{
  "data": {
    "lines": [
      "2026-03-25T10:30:00Z INFO  moss::api: Listening on 0.0.0.0:7185",
      "2026-03-25T10:30:01Z WARN  moss::infra::docker: Slow image pull for mongo:7.0"
    ],
    "count": 2
  }
}
```

---

### GET /api/v1/stone/logs/stream

**Live log stream from the Moss daemon (SSE).**

**Response (200 OK - text/event-stream):**
```
event: log
data: {"timestamp": "2026-03-25T10:30:00Z", "level": "info", "message": "Service started", "target": "moss::domain"}

event: log
data: {"timestamp": "2026-03-25T10:30:01Z", "level": "warn", "message": "High memory usage", "target": "moss::infra"}
```

---

### GET /api/v1/stone/maintenance/history

**Retrieve recent caretaking sweep reports.**

Returns the last N sweep reports showing what maintenance actions were performed.

**Response (200 OK):**
```json
{
  "data": {
    "reports": [
      {
        "timestamp": "2026-03-25T06:00:00Z",
        "actions": ["pruned 3 dangling images", "rotated logs"],
        "duration_ms": 1200
      }
    ]
  }
}
```

---

### POST /api/v1/stone/maintenance/sweep

**Trigger an immediate maintenance sweep.**

Runs the caretaking sweep cycle on demand (normally runs on a schedule).

**Response (200 OK):**
```json
{
  "data": {
    "started": true,
    "job_id": "sweep-01234567"
  }
}
```

---

## Updates

### GET /api/v1/stone/updates

**List pending updates for this stone.**

Returns available updates for offerings, firmware, and system packages.

**Response (200 OK):**
```json
{
  "data": {
    "offerings": [
      {"name": "mongodb", "current": "7.0", "available": "7.0.5"}
    ],
    "firmware": [],
    "total": 1
  }
}
```

---

### POST /api/v1/stone/updates/execute

**Execute pending updates.**

**Request:**
```json
{"scope": "all"}
```

| Scope | Description |
|-------|-------------|
| `all` | All pending updates |
| `offerings` | Software/container updates only |
| `firmware` | Firmware updates only |

**Response (200 OK):**
```json
{
  "data": {
    "job_id": "update-01234567",
    "scope": "all",
    "updates_queued": 1
  }
}
```

---

### GET /api/v1/stone/updates/stream/:job_id

**SSE stream for update progress.**

**Response (200 OK - text/event-stream):**
```
event: progress
data: {"job_id": "update-01234567", "offering": "mongodb", "phase": "pulling", "percent": 45}

event: complete
data: {"job_id": "update-01234567", "success": true, "updated": ["mongodb"]}
```

---

### GET /api/v1/garden/updates

**Aggregate pending updates across all garden stones.**

**Response (200 OK):**
```json
{
  "data": {
    "stones": [
      {"name": "stone-01", "pending": 1},
      {"name": "stone-02", "pending": 0}
    ],
    "total": 1
  }
}
```

---

### POST /api/v1/garden/updates/execute

**Dispatch update execution to affected stones.**

**Request:**
```json
{"scope": "all"}
```

**Response (200 OK):**
```json
{
  "data": {
    "dispatched": ["stone-01"],
    "skipped": ["stone-02"]
  }
}
```

---

## Console

### GET /api/v1/console/mode

**Get the current console display mode.**

**Response (200 OK):**
```json
{
  "data": {
    "mode": "observe"
  }
}
```

---

### POST /api/v1/console/mode

**Set the console display mode.**

**Request:**
```json
{
  "mode": "observe"
}
```

**Response (200 OK):**
```json
{
  "data": {
    "mode": "observe",
    "previous": "idle"
  }
}
```

---

## Garden Operations (Orchestrated)

All garden endpoints hit the tended Moss instance, which aggregates data from all stones in the garden.

### GET /api/v1/garden/services

**Find services across all stones in the garden.**

**Query Parameters:**
- `q` - Search query text

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "services": [
      {
        "name": "mongodb",
        "stone": "stone-01",
        "status": "running",
        "endpoint": "http://192.168.1.100:27017"
      }
    ]
  }
}
```

---

### GET /api/v1/garden/tools

**List garden-wide tools aggregated from all stones.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "tools": [
      {
        "name": "chat",
        "offering": "ollama",
        "stone": "stone-01",
        "type": "inference"
      }
    ]
  }
}
```

---

### GET /api/v1/garden/tools/stream

**SSE stream for tool availability changes across the garden.**

**Response (200 OK - text/event-stream):**
```
event: tool.added
data: {"name": "chat", "offering": "ollama", "stone": "stone-01"}

event: tool.removed
data: {"name": "embedding", "offering": "ollama", "stone": "stone-02"}
```

---

### GET /api/v1/garden/observe

**Aggregate topology view of all stones and their services.**

**Response (200 OK):**
```json
{
  "status": "success",
  "data": {
    "stones": [
      {
        "name": "stone-01",
        "health": "thriving",
        "services": ["mongodb", "redis"],
        "endpoint": "http://192.168.1.100:7185"
      }
    ]
  }
}
```

---

## Rate Limits & Throttling

**Current:** None  
**Future:** Per-client rate limiting with 429 responses

---

## Versioning Strategy

- **Current:** v1 only
- **Breaking Changes:** Require major version bump (v2)
- **Backward Compatibility:** v1 endpoints maintained for minimum 6 months after v2 release
- **Deprecation:** 90-day notice via response headers and documentation

---

## Error Codes Reference

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `OFFERING_NOT_FOUND` | 404 | Offering template doesn't exist |
| `SERVICE_NOT_FOUND` | 404 | Service not registered in Moss |
| `SERVICE_EXISTS` | 409 | Service name already in use |
| `PORT_CONFLICT` | 409 | Requested port already allocated |
| `DOCKER_ERROR` | 500 | Docker daemon communication failure |
| `IMAGE_PULL_FAILED` | 500 | Cannot pull container image |
| `CONTAINER_START_FAILED` | 500 | Container failed to start |
| `RESOURCE_EXHAUSTED` | 503 | Insufficient disk/memory resources |
| `POND_NOT_INITIALIZED` | 409 | Pond CA not created yet |
| `POND_LOCKED` | 423 | CA key encrypted, needs unlock after restart |
| `INVALID_AUTH` | 401 | Wrong passphrase or TOTP code |
| `ENROLLMENT_CLOSED` | 403 | No active enrollment window |
| `ALREADY_ENROLLED` | 409 | Stone hostname already in pond roster |
| `RATE_LIMITED` | 429 | Too many failed auth attempts |

---

## Change Log

### v1 (Current)

**Phase 1-2 (Complete):**
- Services CRUD endpoints
- Lifecycle operations (rest/wake/upgrade)
- Garden topology discovery
- Offerings catalog with compatibility
- Health and capabilities introspection

**Phase 3 (Complete — Pond Security):**
- 9 pond endpoints: init, status, join, invite, unlock, remove, untrust, promote, ca.pem
- CA-based mTLS via koi-certmesh (ECDSA P-256)
- TOTP enrollment (6-digit, 30-second period, configurable TTL)
- Trust profiles: just-me, my-team, my-organization
- CA unlock after restart, promote for standby CA
- mDNS TXT advertises pond and https_port when active

**Phase 3.5 (Complete — Extended Surface):**
- Administrative API: take-root, stone shutdown/reboot, Wake-on-LAN
- Adoption & borrowing: adoptable, adopted, borrowed, adopt, unadopt, borrow, return
- Offering capabilities: list, add, refresh, remove
- Service lifecycle: destroy, reassign, cordon, env read/write, capabilities, refresh-capabilities
- Logs & maintenance: daemon logs (file + SSE), sweep history, on-demand sweep
- Updates: local + garden-wide pending, execute, SSE progress stream
- Console mode get/set
- Garden operations: tools list, tools SSE stream

**Phase 4 (Planned):**
- HTTPS listener on :7183 with route splitting (public vs authenticated)
- Web dashboard integration
- Automated integration tests

---

## Client Libraries

### Official

**Rake CLI:** Built-in client with zen syntax

### Community

None yet - contributions welcome!

**Reference Implementation:** See `src/rake/src/main.rs` for HTTP client patterns

---

## Support & Feedback

- **Issues:** GitHub Issues
- **Documentation:** `/docs` directory
- **Architecture Decisions:** `/docs/decisions/*.md`

---

**Last Updated:** March 25, 2026  
**API Version:** v1  
**Implementation Status:** Phase 3.5 Complete (Pond Security + Extended Surface)
