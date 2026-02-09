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
| POST | `/api/v1/stone/services/:service/nourish` | Update |
| GET | `/api/v1/stone/services/:service/logs` | Stream logs (SSE) |

### Companions
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/companions` | List companions |
| GET | `/api/v1/stone/companions/:id` | Get companion details |
| POST | `/api/v1/stone/companions/:id/command` | Forward command |
| POST | `/api/v1/stone/companions/:id/up` | Start companion |
| POST | `/api/v1/stone/companions/:id/down` | Stop companion |
| POST | `/api/v1/stone/companions/refresh` | Rescan directory |

### Storage / Seed Banks
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/storage` | Storage overview |
| GET | `/api/v1/stone/storage/candidates` | Eligible devices |
| POST | `/api/v1/stone/storage/prepare` | Prepare seed bank |
| GET | `/api/v1/stone/storage/bank` | List seed banks |
| GET | `/api/v1/stone/storage/bank/:id` | Bank details |
| DELETE | `/api/v1/stone/storage/bank/:id` | Remove bank |
| POST | `/api/v1/stone/storage/bank/:id/release` | Unmount |
| GET/PUT/DELETE | `/api/v1/stone/storage/bank/:id/*path` | Object ops |

### S3 Gateway
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/storage/s3` | List buckets |
| GET | `/api/v1/stone/storage/s3/:bucket` | List objects |
| PUT/GET/HEAD/DELETE | `/api/v1/stone/storage/s3/:bucket/*key` | Object ops |

### System
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/capabilities` | Hardware capabilities |
| GET | `/api/v1/stone/metrics` | Prometheus metrics |
| GET | `/health` | Health check |

### Nourishment (Local)
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/nourishment` | Pending updates |
| POST | `/api/v1/stone/nourishment/execute` | Execute updates |
| GET | `/api/v1/stone/nourishment/stream/:job_id` | SSE stream |

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

---

## Garden Endpoints (Orchestrated)

**All garden endpoints hit tended Moss, which aggregates from all stones.**

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/garden/services?q={query}` | Find services across garden |
| GET | `/api/v1/garden/nourishment` | Aggregate updates |
| POST | `/api/v1/garden/nourishment/execute` | Dispatch to affected stones |
| GET | `/api/v1/garden/observe` | Aggregate topology |

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

**Nourishment execute**:
```json
{"scope": "all"}        // All updates
{"scope": "offerings"}  // Software only
{"scope": "firmware"}   // Firmware only
```
