# Zen Garden - API Reference

Quick reference for all REST endpoints. For rules and patterns, see `.agentic/`.

---

## Stone Endpoints (Local Operations)

### Offerings
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/offerings` | List all offerings |
| GET | `/api/v1/stone/offerings/search?q={query}` | Search with taxonomy |
| GET | `/api/v1/stone/offerings/:name` | Get offering details |
| POST | `/api/v1/stone/offerings` | Plant (install) offering |
| DELETE | `/api/v1/stone/offerings/:name` | Remove offering |
| GET | `/api/v1/stone/offerings/inspect?image={ref}` | Inspect Docker image (OFFER-0006) |
| POST | `/api/v1/stone/offerings/refresh` | Refresh catalog |
| POST | `/api/v1/stone/offerings/heal` | Adopt orphaned containers |

### Services
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/services` | List local services |
| POST | `/api/v1/stone/services` | Create service |
| GET | `/api/v1/stone/services/:service` | Get service details |
| DELETE | `/api/v1/stone/services/:service` | Delete service |
| POST | `/api/v1/stone/services/:service/restart` | Restart |
| POST | `/api/v1/stone/services/:service/rest` | Stop |
| POST | `/api/v1/stone/services/:service/wake` | Start |
| POST | `/api/v1/stone/services/:service/upgrade` | Upgrade |
| GET | `/api/v1/stone/services/:service/logs` | Stream logs (SSE) |
| GET | `/api/v1/stone/services/:service/env` | Read env vars + manageable list |
| PATCH | `/api/v1/stone/services/:service/env` | Set/delete env vars (allowlist) |

### Companions
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/companions` | List companions |
| GET | `/api/v1/stone/companions/:id` | Get companion details |
| POST | `/api/v1/stone/companions/:id/command` | Forward command |
| POST | `/api/v1/stone/companions/:id/up` | Start companion |
| POST | `/api/v1/stone/companions/:id/down` | Stop companion |
| POST | `/api/v1/stone/companions/refresh` | Rescan directory |

### Storage (STORAGE-0009)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/storage` | Storage overview |
| GET | `/api/v1/stone/storage/health` | Health status |
| GET | `/api/v1/stone/storage/candidates` | Eligible devices |
| POST | `/api/v1/stone/storage/add` | Add storage (device or directory) |
| GET | `/api/v1/stone/storage/banks` | List local storages |
| GET | `/api/v1/stone/storage/banks/{name}` | Storage details |
| DELETE | `/api/v1/stone/storage/banks/{name}` | Remove storage |
| POST | `/api/v1/stone/storage/banks/{name}/release` | Unmount |
| POST | `/api/v1/stone/storage/banks/{name}/pin` | Claim Primary |
| POST | `/api/v1/stone/storage/banks/{name}/unpin` | Release Primary |
| PATCH | `/api/v1/stone/storage/banks/{name}/visibility` | Set visibility |
| PATCH | `/api/v1/stone/storage/banks/{name}/rename` | Rename |
| PATCH | `/api/v1/stone/storage/banks/{name}/roles` | Set roles (seed-bank, etc.) |
| GET | `/api/v1/stone/storage/banks/{name}/changes` | Replication changelog |
| GET | `/api/v1/stone/storage/stream` | SSE replication stream |

### Banks (ARCH-0026)

First-class bank endpoints. Pin/unpin on the legacy `/storage/banks/` paths redirect 301 to these.

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/banks` | List banks with a local volume |
| GET | `/api/v1/stone/banks/{moniker}` | Bank details |
| POST | `/api/v1/stone/banks/{moniker}/pin` | Claim Primary |
| POST | `/api/v1/stone/banks/{moniker}/unpin` | Release Primary |

### S3 Gateway (STORAGE-0016)

**Per-storage port listeners** — each bank has a dedicated S3 port (default range 23400–23499).
Standard S3 clients connect to `http://{stone}:{s3-port}/{bucket}/{key}` with no path prefix.

Discover ports: `GET /api/v1/stone/storage/s3/ports` or via Garden SSE `connection.port`.

| Method | Endpoint (per-port listener, root `/`) | Purpose |
|--------|----------------------------------------|---------|
| GET | `/` | List buckets |
| PUT | `/{bucket}` | Create bucket |
| GET | `/{bucket}?[list-type=2]` | List objects (V1 or V2) |
| PUT | `/{bucket}/{key}` | Put object (with `x-amz-meta-*` support) |
| PUT | `/{bucket}/{key}` + `x-amz-copy-source` header | Copy object |
| GET | `/{bucket}/{key}` | Get object (Range reads → 206) |
| HEAD | `/{bucket}/{key}` | Object metadata |
| DELETE | `/{bucket}/{key}` | Delete object |
| POST | `/{bucket}/{key}?uploads` | Initiate multipart upload |
| PUT | `/{bucket}/{key}?partNumber=N&uploadId=X` | Upload part |
| POST | `/{bucket}/{key}?uploadId=X` | Complete multipart upload |
| DELETE | `/{bucket}/{key}?uploadId=X` | Abort multipart upload |

**Presigned URLs** (Moss-native HMAC, not SigV4):
| Method | Endpoint (port 7185) | Purpose |
|--------|----------------------|---------|
| POST | `/api/v1/storage/s3/presign` | Generate time-limited access token |

**Port catalog** (port 7185):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/storage/s3/ports` | S3 port per bank (diagnostic) |

**Legacy compatibility path** (port 7185, same unified namespace):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/storage/s3` | List buckets |
| GET | `/api/v1/storage/s3/{bucket}` | List objects |
| PUT/GET/HEAD/DELETE | `/api/v1/storage/s3/{bucket}/*key` | Object ops |

All conditional headers honored: `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`.
`NoSuchBucket` (404) returned for unknown buckets; `ServiceUnavailable` (503) when storage device removed.

### System
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/capabilities` | Hardware capabilities |
| GET | `/api/v1/stone/metrics` | Prometheus metrics |
| GET | `/health` | Health check |

### Updates (Local)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/updates` | Pending updates |
| POST | `/api/v1/stone/updates/execute` | Execute updates |
| GET | `/api/v1/stone/updates/stream/:job_id` | SSE stream |

### Logs (Daemon)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/logs?lines=100&level=warn` | Recent log lines from file |
| GET | `/api/v1/stone/logs/stream` | Live log stream (SSE) |

### Maintenance (Caretaking)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/maintenance/history` | Last N sweep reports |
| POST | `/api/v1/stone/maintenance/sweep` | Trigger immediate sweep |

### Pond (Security / Trust)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| POST | `/api/v1/pond/init` | Place keystone (create CA) |
| GET | `/api/v1/pond/status` | Pond status and membership |
| POST | `/api/v1/pond/join` | Join pond with TOTP code |
| POST | `/api/v1/pond/invite` | Open enrollment, rotate auth |
| POST | `/api/v1/pond/unlock` | Unlock CA after restart |
| DELETE | `/api/v1/pond` | Drain pond (destroy CA) |
| DELETE | `/api/v1/pond/stones/:name` | Untrust / revoke a stone |
| POST | `/api/v1/pond/promote` | Promote to standby CA |
| PUT | `/api/v1/pond/name` | Rename pond (decorative) |
| GET | `/api/v1/pond/ca.pem` | Download CA public certificate |

---

## Garden Endpoints (Orchestrated)

**All garden endpoints hit tended Moss, which aggregates from all stones.**

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/garden/services?q={query}` | Find services across garden |
| GET | `/api/v1/garden/updates` | Aggregate updates |
| POST | `/api/v1/garden/updates/execute` | Dispatch to affected stones |
| GET | `/api/v1/garden/topology` | Aggregate topology |

### Garden Banks (ARCH-0026)

| Method | Endpoint | Purpose |
|--------|----------|----------|
| GET | `/api/v1/garden/banks` | All banks in the garden |
| GET | `/api/v1/garden/banks/{moniker}` | Bank details + volume locations |
| GET | `/api/v1/garden/banks/{moniker}/volumes` | Individual volumes, their stones, roles |

### Garden Storage (STORAGE-0009)

| Method | Endpoint | Purpose |
|--------|----------|----------|
| GET | `/api/v1/garden/storage` | List all storages across garden |
| GET | `/api/v1/garden/storage/{name}` | Discover replicas |
| GET | `/api/v1/garden/storage/{name}/fs` | Directory listing (`?path=&depth=N`) |
| GET | `/api/v1/garden/storage/{name}/fs/*path` | Read user file |
| PUT | `/api/v1/garden/storage/{name}/fs/*path` | Write user file |
| DELETE | `/api/v1/garden/storage/{name}/fs/*path` | Delete user file/dir |
| HEAD | `/api/v1/garden/storage/{name}/fs/*path` | File metadata |
| GET | `/api/v1/garden/storage/{name}/objects/*path` | Read S3 object |
| PUT | `/api/v1/garden/storage/{name}/objects/*path` | Write S3 object |
| DELETE | `/api/v1/garden/storage/{name}/objects/*path` | Delete S3 object |
| HEAD | `/api/v1/garden/storage/{name}/objects/*path` | S3 object metadata |
| GET | `/api/v1/garden/storage/{name}/snapshots` | List offerings with snapshots |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}` | List offering snapshots |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}/manifest` | Offering manifest |
| GET | `/api/v1/garden/storage/{name}/snapshots/{offering}/{harvest}` | Download snapshot |

### WebDAV (STORAGE-0009 Phase 3)

| Method | Endpoint | Purpose |
|--------|----------|----------|
| ANY | `/dav/{name}/{*path}` | RFC 4918 WebDAV (PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE, LOCK) |

**Platform mapping:**

| Platform | Connection |
|----------|------------|
| macOS | Finder > Connect to Server > `http://stone-name.local:7185/dav/personal/` |
| Linux | File manager > Connect to Server, or `davfs2` mount |
| Windows | Map Network Drive, or Cloud Filter (future Phase 4) |

---

## Ollama Orchestrator Endpoints

**Proxy port** (`:21434`) — Ollama-compatible + extension endpoints.

### Extension API (`/v1/`)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/v1/models` | Model inventory with placement, VRAM, fitness |
| GET | `/v1/stones` | Stone inventory with GPU, VRAM, loaded models |
| GET | `/v1/recommendations` | All capability recommendations (grouped) |
| GET | `/v1/recommendations?capability={cap}` | Single capability recommendations |

| PUT | `/v1/recommendations/{capability}/pin` | Pin a model for a capability |
| DELETE | `/v1/recommendations/{capability}/pin` | Unpin a capability |

**Capabilities**: `quick`, `chat`, `completion` (alias for chat), `synthesis`, `vision`, `ocr`, `tools`, `thinking`, `embedding`

**Recommended model monikers**: Use `"model": "recommended:{capability}"` in any inference request. The proxy resolves to the top-ranked model, rewrites the body, and adds `X-Zen-Resolved-Model` response header. See [ORCH-0011](../../docs/decisions/ORCH-0011-recommended-model-monikers.md).

All other paths are proxied to Ollama with smart routing.

### Dashboard API (`:7190`)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/` | Dashboard HTML |
| GET | `/api/status` | Full snapshot (stones, models, benchmark, metrics) |
| GET | `/api/events` | SSE event stream |
| GET | `/api/settings` | Current orchestrator settings |
| POST | `/api/settings` | Update settings |
| GET | `/api/jobs` | Active/recent orchestrator jobs |
| POST | `/api/metrics/reset` | Reset all metrics |
| POST | `/api/metrics/model-counters/reset` | Reset per-model request counters |
| POST | `/api/management/pull` | Pull model to stones |
| POST | `/api/management/delete` | Delete model from stones |
| GET | `/api/management/feasibility` | Check if model fits stone VRAM |
| POST | `/api/benchmark/start` | Start fitness benchmark |
| POST | `/api/benchmark/cancel` | Cancel running benchmark |
| GET | `/api/benchmark/results` | Benchmark run results |
| GET | `/api/benchmark/export` | Export GPU fitness matrix |
| GET | `/health` | Health check |

---

## Query Parameters

**Search** (`/offerings/search`, `/garden/services`):
- `q` - Query text
- `prefer` - Hardware preferences (comma-separated)
- `limit` - Max results (default: 5)

**Listing** (`/storage/bank/:id/*path`):
- `depth=1` - Immediate children (default)
- `depth=3` - 3 levels deep
- `depth=all` - Full recursive

**Updates execute**:
```json
{"scope": "all"}        // All updates
{"scope": "offerings"}  // Software only
{"scope": "firmware"}   // Firmware only
```
