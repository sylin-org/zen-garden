---
audience: [developer, operator, maintainer]
doc_type: spec
status: current
last_verified: 2026-02-06
canonical: true
note: "Formal ADR documenting dual-layer API architecture."
---

# Zen Garden API v1: Dual-Layer Architecture

**Status:** Design Document  
**Date:** 2026-01-17  
**Purpose:** Complete API structure with progressive disclosure through dual layers

---

## Executive Summary

Zen Garden provides **two API layers** for the same underlying resources:

1. **Offerings API** — Human-friendly, safe, simplified (90% of users)
2. **Services API** — Technical, detailed, full control (10% power users)

Both layers access the same data but present different views optimized for different audiences.

---

## Complete API v1 Structure

### Offerings API (Human Layer)

**Target:** Beginners, scripters, CI/CD, simple automation  
**Philosophy:** Hide Docker complexity, provide safety rails, optimize for common case

#### Catalog Operations

```http
GET  /api/v1/offerings                     # List all offerings (available + installed)
GET  /api/v1/offerings?state=available     # Filter: available to install
GET  /api/v1/offerings?state=installed     # Filter: planted offerings
GET  /api/v1/offerings/{name}              # Offering details + compatibility (FQN accepted; instance ignored)
GET  /api/v1/offerings/{name}/manifest     # Raw YAML definition (FQN accepted; instance ignored)
```

**GET /api/v1/offerings response:**
```json
{
  "offerings": [
    {
      "name": "mongodb",
      "state": "available",
      "category": "databases",
      "description": "MongoDB NoSQL database",
      "tags": ["nosql", "document-store"],
      "compatibility": {"decision": "pass", "reason": null}
    },
    {
      "name": "postgres",
      "state": "installed",
      "category": "databases",
      "health": "healthy",
      "uptime": "2 days"
    }
  ]
}
```

#### Lifecycle Operations

```http
POST   /api/v1/offerings                   # Plant offering (simplified install)
DELETE /api/v1/offerings/{name}            # Take away offering (uninstall)
POST   /api/v1/offerings:heal              # Heal garden (discover orphans)
POST   /api/v1/offerings:refresh           # Refresh catalog from disk
```

**POST /api/v1/offerings (plant):**
```json
// Request
{
  "name": "mongodb",
  "config": {
    "environment": {"MONGO_INITDB_ROOT_USERNAME": "admin"}
  }
}

// Response (202 Accepted)
{
  "name": "mongodb",
  "state": "installing",
  "job_id": "job_abc123"
}
```

---

### Services API (Technical Layer)

**Target:** Operators, troubleshooting, DevOps, advanced automation  

---

## Companion Management API

**Target:** Companion control and command routing  
**Philosophy:** Extend Stone capabilities with pluggable services

### Companion Operations

```http
GET  /api/v1/stone/companions                # List all registered Companions
GET  /api/v1/stone/companions/{id}           # Get Companion details and manifest
POST /api/v1/stone/companions/{id}/command   # Forward command to Companion (5s timeout)
POST /api/v1/stone/companions/{id}/up        # Start Companion process
POST /api/v1/stone/companions/{id}/down      # Stop Companion process
POST /api/v1/stone/companions/refresh        # Rescan Companion directory
```

**GET /api/v1/stone/companions response:**
```json
{
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
```

**POST /api/v1/stone/companions/cricket/command:**
```json
// Request
{
  "args": ["play", "stone-online"]
}

// Response (200 OK)
{
  "success": true,
  "output": "Playing: stone-online.mp3 on foreground channel"
}

// Response (500 Internal Server Error) - timeout/connection failure
{
  "success": false,
  "output": "Failed to connect to Companion on port 7187"
}
```

**Architecture:**
- Companions bind HTTP servers on assigned ports (7187-7199)
- Moss maintains port ledger in `{data_dir}/companion-ports.json`
- Commands routed: Rake → Moss → Companion (localhost)
- Companions receive presence events via SSE subscription to Moss

**Reference:**
- [Companion-COMMAND-PROTOCOL.md](Companion-COMMAND-PROTOCOL.md)
- [Companion-SERVICE-REGISTRY.md](Companion-SERVICE-REGISTRY.md)
- [HEY-TELL-SYNTAX.md](hey-tell-syntax.md)

---

### Services API (Technical Layer)

**Target:** Operators, troubleshooting, DevOps, advanced automation  
**Philosophy:** Expose container reality, provide full control, enable debugging

#### Manifest Operations

```http
GET /api/v1/services/manifests             # List all service manifests
GET /api/v1/services/{name}/manifest       # Get specific manifest YAML (FQN accepted; instance ignored)
```

**GET /api/v1/services/manifests response:**
```json
{
  "manifests": [
    {
      "name": "mongodb",
      "category": "databases",
      "image": "mongo:7.0",
      "ports": [{"container": 27017, "protocol": "tcp"}],
      "volumes": [{"mount": "/data/db"}],
      "environment_defaults": {
        "MONGO_INITDB_ROOT_USERNAME": "admin",
        "MONGO_INITDB_ROOT_PASSWORD": "${GENERATED}"
      },
      "compatibility_rules": {
        "architectures": ["amd64", "arm64"],
        "fallback_image": "mongo:7.0-alpine"
      }
    }
  ]
}
```

#### Runtime Operations

```http
GET    /api/v1/services                    # List services (container-level details)
GET    /api/v1/services/{name}             # Service details (full technical view, name = FQN)
GET    /api/v1/services/{name}/logs        # Stream logs (SSE)
POST   /api/v1/services                    # Install (full Docker control)
POST   /api/v1/services/{name}:restart     # Restart service
POST   /api/v1/services/{name}:cordon      # Mark unavailable
DELETE /api/v1/services/{name}             # Uninstall
POST   /api/v1/services:reconcile          # Reconcile inventory
POST   /api/v1/services:refresh            # Refresh manifests
```

**GET /api/v1/services/{name} response (detailed):**
```json
{
  "name": "mongodb",
  "container_id": "a1b2c3d4e5f6",
  "state": "running",
  "image": "mongo:7.0",
  "image_id": "sha256:abc123...",
  "created_at": "2026-01-15T10:30:00Z",
  "ports": [
    {"host": 27017, "container": 27017, "protocol": "tcp", "host_ip": "0.0.0.0"}
  ],
  "volumes": [
    {"host": "/var/lib/zen-garden/mongodb", "container": "/data/db", "mode": "rw", "size_mb": 2048}
  ],
  "environment": {"MONGO_INITDB_ROOT_USERNAME": "admin"},
  "networks": [{"name": "zen-garden-default", "ip": "172.18.0.5"}],
  "health_check": {
    "status": "healthy",
    "last_check": "2026-01-17T10:30:00Z",
    "consecutive_failures": 0
  },
  "resource_usage": {
    "cpu_percent": 5.2,
    "memory_mb": 256,
    "memory_limit_mb": 1024
  },
  "restart_policy": "unless-stopped",
  "uptime_seconds": 172800
}
```

**POST /api/v1/services (install with full control):**
```json
{
  "name": "mongodb",
  "image": "mongo:7.0",
  "ports": [{"host": 27017, "container": 27017}],
  "volumes": [{"host": "/custom/path", "container": "/data/db"}],
  "environment": {"MONGO_INITDB_ROOT_USERNAME": "admin"},
  "restart_policy": "unless-stopped",
  "memory_limit_mb": 1024,
  "health_check": {
    "command": ["mongo", "--eval", "db.adminCommand('ping')"],
    "interval_seconds": 30
  }
}
```

---

### Capabilities API (Offerings)

**Target:** Capability discovery and management for offering instances  
**Path param** `name` accepts FQN (URL-encode `:` as `%3A`)

```http
GET    /api/v1/stone/offerings/{name}/capabilities
POST   /api/v1/stone/offerings/{name}/capabilities
DELETE /api/v1/stone/offerings/{name}/capabilities/{capability}
POST   /api/v1/stone/offerings/{name}/capabilities/refresh
POST   /api/v1/stone/offerings/{name}/capabilities/mirror
```

**Mirror request:**
```json
{
  "from": "stone-01",
  "to": "stone-02",
  "dry_run": false
}
```

---

### Stone Operations (Universal)

```http
GET  /health                               # Health check (Prometheus standard)
GET  /capabilities                         # Stone hardware capabilities
GET  /metrics                              # Prometheus metrics
POST /api/v1/stone:upgrade                 # Upgrade stone software
POST /api/v1/stone:shutdown                # Shutdown Moss daemon
```

---

### Resolution API

**Target:** Service discovery and connection string resolution  
**Philosophy:** Unified resolution for protocols and offerings

```http
GET /api/v1/resolve                        # Resolve connection string components
```

**Query Parameters:**

| Parameter  | Required | Description |
|------------|----------|-------------|
| `offering` | No       | Offering name (mongodb, redis, minio) |
| `protocol` | No       | Protocol (s3, mongodb, redis, storage) |
| `instance` | No       | Instance name for multi-instance offerings |

**Resolution Logic:**
- If `protocol` specified → find offerings supporting that protocol
- If `offering` specified → resolve that specific offering
- If both → find specific offering with protocol validation

**GET /api/v1/resolve?protocol=s3 response:**
```json
{
  "resolved": {
    "protocol": "s3",
    "endpoint": "http://10.0.1.10:9000",
    "offering": "minio",
    "instance": null,
    "stone": "stone-01",
    "source": "offering"
  },
  "alternatives": [
    {
      "endpoint": "http://10.0.1.10:7185/api/v1/storage",
      "offering": null,
      "stone": "stone-01",
      "source": "seed-bank"
    }
  ]
}
```

**GET /api/v1/resolve?offering=mongodb&instance=staging response:**
```json
{
  "resolved": {
    "protocol": "mongodb",
    "endpoint": "mongodb://10.0.1.10:27018",
    "offering": "mongodb",
    "instance": "staging",
    "stone": "stone-01",
    "source": "offering"
  },
  "alternatives": []
}
```

---

### Storage API (Seed Bank Gateway)

**Target:** S3-compatible storage access via seed banks  
**Philosophy:** Infrastructure-as-capability, protocol-based access

```http
PUT    /api/v1/storage/s3/{bucket}/{key}   # Put object
GET    /api/v1/storage/s3/{bucket}/{key}   # Get object
HEAD   /api/v1/storage/s3/{bucket}/{key}   # Head object (metadata)
DELETE /api/v1/storage/s3/{bucket}/{key}   # Delete object
GET    /api/v1/storage/s3                  # List buckets
GET    /api/v1/storage/s3/{bucket}         # List objects (with prefix, pagination)
```

**Headers:**
- `X-Seed-Bank` (optional): Select a specific seed bank by name

**PUT /api/v1/storage/s3/configs/app.json:**
```http
PUT /api/v1/storage/s3/configs/app.json
Content-Type: application/json

{"key": "value"}

Response:
200 OK
ETag: "abc123..."
```

**GET /api/v1/storage/s3/configs?prefix=data/:**
```xml
<ListBucketResult>...</ListBucketResult>
```

---

### Storage API (REST)

**Target:** SDK-friendly storage access via seed banks  
**Philosophy:** JSON responses + raw bytes, non-S3 surface

```http
GET    /api/v1/storage                    # List buckets (JSON)
PUT    /api/v1/storage/{bucket}/{key}     # Put object (JSON response)
GET    /api/v1/storage/{bucket}/{key}     # Get object (raw bytes)
HEAD   /api/v1/storage/{bucket}/{key}     # Head object (metadata)
DELETE /api/v1/storage/{bucket}/{key}     # Delete object
GET    /api/v1/storage/{bucket}/?list=true&prefix=...&delimiter=/&marker=...&max-keys=...
```

**Headers:**
- `X-Seed-Bank` (optional): Select a specific seed bank by name

---

### Memories API (Hydration)

**Target:** Read-only access to nurturing backups for external orchestrators  
**Philosophy:** Backups are portable, discoverable, and auditable

```http
GET /api/v1/memories                                # List all remote snapshots (index)
GET /api/v1/memories/{offering_id}                  # List snapshots for offering
GET /api/v1/memories/{offering_id}/manifest         # Hydration metadata (offering.json)
GET /api/v1/memories/{offering_id}/{harvest_id}     # Download snapshot tar.gz
```

**Headers:**
- `X-Seed-Bank` (optional): Select a specific seed bank by name

---

### Seed Bank Management

**Target:** Storage infrastructure management  
**Philosophy:** Adopt local/network storage as S3-capable seed banks

```http
GET    /api/v1/stone/storage                      # Overview (seed banks + candidates)
GET    /api/v1/stone/storage/health               # Storage readiness (mounted + canonical + writable)
GET    /api/v1/stone/storage/candidates           # List eligible devices
POST   /api/v1/stone/storage/prepare              # Prepare a device as seed bank
POST   /api/v1/stone/storage/release-all          # Safely unmount all seed banks
GET    /api/v1/stone/storage/bank                 # List seed banks
GET    /api/v1/stone/storage/bank/{id}            # Get seed bank details
DELETE /api/v1/stone/storage/bank/{id}            # Remove seed bank (does not delete data)
PATCH  /api/v1/stone/storage/bank/{id}/visibility # Change visibility (open/closed)
PATCH  /api/v1/stone/storage/bank/{id}/rename     # Rename seed bank (pool rules apply)
POST   /api/v1/stone/storage/bank/{id}/release    # Safely unmount seed bank
```

**Object operations (stone-local, by seed bank id):**
```http
GET    /api/v1/stone/storage/bank/{id}/*path       # Get object
PUT    /api/v1/stone/storage/bank/{id}/*path       # Put object
DELETE /api/v1/stone/storage/bank/{id}/*path       # Delete object
HEAD   /api/v1/stone/storage/bank/{id}/*path       # Object metadata
```

---

### Events & Jobs (Universal)

```http
GET /api/v1/stone/presence/stream          # Stream all domain events (SSE) - unified endpoint
GET /api/v1/jobs                           # List recent jobs
GET /api/v1/jobs/{id}                      # Job status/result
```

**Note:** All events (offerings, storage, stone, jobs) flow through the unified presence stream.
Job progress events (`job.started`, `job.progress`, `job.completed`, `job.failed`) are included
alongside service and storage events, giving Companions like Cricket and Firefly full visibility
into stone activity.

---

## Key Differences: Offerings vs Services

| Aspect | Offerings API | Services API |
|--------|---------------|--------------|
| **State names** | `available`, `installing`, `installed` | `creating`, `running`, `exited` |
| **Health** | `healthy`, `degraded` | Detailed health check results |
| **Configuration** | Simplified (no raw flags) | Full Docker configuration |
| **Responses** | Human-readable summaries | Container IDs, technical details |
| **Operations** | Plant, take away, heal | Install, uninstall, reconcile |
| **Errors** | Friendly messages | Technical error codes |

---

## API Selection Guide

**Use Offerings API when:**
- Building quick start tutorials
- Writing simple automation scripts
- Creating beginner-friendly UIs
- You don't need container internals

**Use Services API when:**
- Debugging production issues
- Building operator dashboards
- Need port bindings, volume paths
- Implementing advanced orchestration
- Writing monitoring integrations

---

## CLI Mapping

**Offerings API (Zen commands):**
```bash
garden-rake explore              # GET /api/v1/offerings?state=available
garden-rake offer mongodb        # POST /api/v1/offerings
garden-rake observe              # GET /api/v1/offerings?state=installed
```

**Services API (Technical operations):**
```bash
garden-rake service logs mongodb     # GET /api/v1/services/mongodb/logs
garden-rake service inspect mongodb  # GET /api/v1/services/mongodb
garden-rake service reconcile        # POST /api/v1/services:reconcile
```

---

## Implementation Notes

### Shared Backend

Both APIs access the same:
- Registry state (in-memory + persistent)
- Docker API calls
- Template/manifest files
- Compatibility engine

**Difference is presentation layer only.**

### Response Transformers

```rust
// Offerings layer - simplified view
pub fn to_offering_view(service: &Service) -> OfferingView {
    OfferingView {
        name: service.name.clone(),
        state: simplify_state(&service.state),
        health: simplify_health(&service.health),
        uptime: humanize_duration(service.uptime_seconds),
        // Container internals hidden
    }
}

// Services layer - full technical view
pub fn to_service_view(service: &Service, container_info: &ContainerInfo) -> ServiceView {
    ServiceView {
        name: service.name.clone(),
        container_id: container_info.id.clone(),
        state: container_info.state.clone(),
        ports: container_info.ports.clone(),
        volumes: container_info.mounts.clone(),
        resource_usage: get_resource_usage(container_info),
        // All details exposed
    }
}
```

### Error Handling

**Offerings API:**
```json
{
  "error": "OFFERING_NOT_FOUND",
  "message": "MongoDB is not available in the catalog",
  "suggestion": "Run 'garden-rake explore database' to see available options"
}
```

**Services API:**
```json
{
  "error": "CONTAINER_NOT_FOUND",
  "message": "No container found with name 'mongodb'",
  "details": {
    "searched_names": ["zen-offering-mongodb", "mongodb"],
    "available_containers": ["zen-offering-postgres"]
  }
}
```

---

## Documentation Strategy

### Quick Start (Offerings API Only)
```markdown
## Getting Started

1. Explore available offerings:
   ```bash
   garden-rake explore
   ```

2. Plant MongoDB:
   ```bash
   garden-rake offer mongodb
   ```

3. Check status:
   ```bash
   garden-rake observe
   ```
```

### Troubleshooting Guide (Introduces Services API)
```markdown
## Debugging Services

If an offering isn't working as expected, use the services API for detailed inspection:

```bash
# Get container-level details
curl http://localhost:7185/api/v1/services/mongodb

# Stream logs
curl http://localhost:7185/api/v1/services/mongodb/logs
```

This exposes:
- Container ID and image SHA
- Port bindings and volume mounts
- Resource usage and limits
- Health check results
```

---

## Migration Path

**Phase 1:** Implement both APIs  
**Phase 2:** Document Offerings API first (quick start)  
**Phase 3:** Document Services API (operations manual)  
**Phase 4:** Build CLI commands for both layers

**No deprecation needed** — both APIs coexist permanently, serving different needs.
