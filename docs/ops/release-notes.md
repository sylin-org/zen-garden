---
audience: operator
doc_type: reference
status: current
last_verified: 2026-02-16
---

# Release Notes

**Release history, breaking changes, known issues, and deprecation timeline.**

---

## Current Release

### Version 0.1.0 (V1 API) — January 2026

#### Companion SDK and Physical Presence

**CompanionState Persistence:**
- Companions persist on/off state across restarts
- `CompanionState` module in companion-sdk handles file-based persistence
- Moss passes `--state-dir` to Companions on spawn

**Firefly LED Companion:**
- Ambient "firefly" animation on Waveshare RP2040-Matrix 5x5 RGB LED
- Zen garden-inspired visual language:
  - Warm white pixels fade in/out like fireflies
  - Load-based tempo (idle = slow/sparse, busy = fast/dense)
  - Green firefly for seed-bank (storage) presence
  - Blue firefly for running services
  - Activity bonus from installed offerings (+0.05 per offering)
- Override layer for notifications (tended, health changes, service events)
- Brightness persistence across restarts
- Display cleared on SIGTERM/shutdown for clean exit
- SSE subscription to Moss presence events

**Cricket Audio Companion:**
- SSE event handling for automatic sound playback
- On/off state persistence across restarts
- Event-to-sound mapping for health changes, service events

#### Dual-Layer API Architecture

**Offerings API (Human Layer — 90% use case):**
- `GET /api/v1/offerings` — List offerings with state filtering
- `GET /api/v1/offerings/:name` — Simplified offering details
- `GET /api/v1/offerings/:name/manifest` — YAML template
- `POST /api/v1/offerings:heal` — Heal garden (reconcile all services)
- `POST /api/v1/offerings:refresh` — Refresh catalog
- `DELETE /api/v1/offerings/:name` — Take away offering (forwards to services)

**Services API (Technical Layer — 10% power users):**
- `GET /api/v1/services/manifests` — List all manifests
- `GET /api/v1/services/:name/manifest` — Get manifest YAML
- `GET /api/v1/services` — List with container details
- `GET /api/v1/services/:name` — Full technical view
- `GET /api/v1/services/:name/logs` — SSE log streaming
- `POST /api/v1/services` — Install with full control
- `DELETE /api/v1/services/:name` — Uninstall
- `POST /api/v1/services/:name:restart` — Restart operation
- `POST /api/v1/services:reconcile` — Reconcile inventory
- `POST /api/v1/services:refresh` — Refresh manifests

**Stone Operations:**
- `POST /api/v1/stone:upgrade` — Upgrade stone software
- `POST /api/v1/stone:shutdown` — Shutdown daemon

**Universal Endpoints (no v1 namespace):**
- `GET /health` — Health check
- `GET /capabilities` — Stone capabilities
- `GET /metrics` — Prometheus metrics

**Events & Jobs:**
- `GET /api/v1/stone/presence/stream` — Unified SSE event stream (services, storage, stone, jobs)
- `GET /api/v1/jobs` — List jobs
- `GET /api/v1/jobs/:id` — Job status

The former `/api/v1/events` endpoint is consolidated into `/api/v1/stone/presence/stream`. All event types (services, storage, stone health, and job progress) flow through the unified presence stream.

#### CLI (garden-rake)

**Zen commands (Offerings API):**
- `explore` → `GET /api/v1/offerings`
- `offer <name>` → `GET /api/v1/offerings/:name`
- `refresh` → `POST /api/v1/offerings:refresh`

**Technical commands (Services API):**
- `observe` → `GET /api/v1/services`
- `templates` → `GET /api/v1/services/manifests`
- `template <name>` → `GET /api/v1/services/:name/manifest`

#### Design Decisions

**Custom action format:** Single colon (`:heal`, `:refresh`, `:reconcile`) aligns with Kubernetes/GCP standards.

**Progressive disclosure:** Same backend, different presentation layers:
- **Offerings API:** Simplified responses, hide container IDs, human-friendly health
- **Services API:** Full container details, technical metrics, debugging info

#### Not Yet Implemented

**Placeholders returning `NOT_IMPLEMENTED`:**

1. **`POST /api/v1/offerings` (plant offering)** — Requires full installation logic with environment generation. Currently forwards to services API.
2. **`POST /api/v1/services/:name:cordon` (cordon service)** — Requires ServiceStatus enum extension (add Cordoned state).

**Pond Security (Implemented):**
- 9 endpoints: init, status, join, invite, unlock, remove, untrust, promote, ca.pem
- CA-based mTLS via koi-certmesh (ECDSA P-256)
- TOTP enrollment (6-digit, 30-second period, configurable TTL)
- Trust profiles: just-me, my-team, my-organization

**Future features:**
- Garden topology endpoints (`/api/v1/garden/*`) — present in code but not documented
- HTTPS listener on :7187 (port defined, binding planned)

#### Known Limitations

1. **Native protocol only:** Agnostic Data API sidecars not yet implemented
2. **Single Stone focus:** Garden-wide operations (`--all` flag) work but have limited testing
3. **Manual discovery:** mDNS announcements work on Linux; Windows requires UDP broadcast discovery
4. **No RBAC:** Pond security (mTLS) implemented via certmesh; per-user access control not planned
5. **Basic health monitoring:** Advanced health checks (restart loops, resource thresholds) planned for Phase 2

---

## Release History

### V1 API (0.1.0) — January 19, 2026

Production-ready dual-layer API.

- 23 v1 API endpoints (Offerings + Services + Stone operations)
- Universal health/capabilities/metrics endpoints
- SSE events and job tracking
- CLI updated to v1 endpoints
- Custom action format (`:heal`, `:refresh`, `:reconcile`)
- Progressive disclosure pattern (human vs technical layers)

### Initial Development (0.0.x) — December 2025 – January 2026

Core functionality and architecture.

- Rust workspace (moss, rake, common crates)
- Shared types: ServiceInfo, StoneInfo, HealthStatus
- HTTP API Foundation (Axum server, reqwest client)
- Service Registry (in-memory tracking, status management)
- Docker Compose Integration (template loading, atomic updates)
- UDP Broadcast Discovery (Windows-compatible discovery)
- mDNS Announcements (Linux mDNS integration)
- Garden-Wide Operations (Moss coordinator pattern)
- Docker build pipeline and GitHub Actions CI (Linux + Windows)

---

## Breaking Changes

### V1 API Migration (0.1.0)

All API clients must update endpoint paths.

#### Endpoint Changes

**Offerings API:**

| Old Endpoint | New Endpoint | Notes |
|--------------|--------------|-------|
| `GET /api/offerings` | `GET /api/v1/offerings` | Added v1 namespace |
| `GET /api/offerings/{name}` | `GET /api/v1/offerings/{name}` | Added v1 namespace |
| `POST /api/offerings/refresh` | `POST /api/v1/offerings:refresh` | Custom action format (single colon) |

**Services API:**

| Old Endpoint | New Endpoint | Notes |
|--------------|--------------|-------|
| `GET /api/templates` | `GET /api/v1/services/manifests` | Renamed templates → manifests |
| `GET /api/templates/{name}` | `GET /api/v1/services/{name}/manifest` | Nested under services, singular manifest |
| `GET /api/services` | `GET /api/v1/services` | Added v1 namespace |

#### Response Format Changes

**Manifest listing:**

```json
// Old response
{
  "templates": [...]
}

// New response
{
  "manifests": [...]
}
```

Update JSON key parsing from `templates` to `manifests`.

#### Custom Action Format

Old format: `POST /api/offerings/refresh` (REST sub-resource)
New format: `POST /api/v1/offerings:refresh` (custom action with single colon)

Rationale: Aligns with industry standards (Kubernetes `:exec`, GCP `:start`). See [API-0001](../decisions/API-0001-dual-layer-api.md).

#### Backwards Compatibility

Old moss versions (pre-v1) return 404 for v1 endpoints. CLI shows upgrade message when detecting 404 on v1 endpoints.

**Migration path:**
1. Upgrade Moss daemon to 0.1.0
2. Update CLI to 0.1.0
3. Update third-party tools to v1 endpoints

---

## Known Issues

### Cross-Platform Discovery (Windows)

**Issue:** mDNS browse not available on Windows without third-party daemon (Bonjour).

**Workaround:** UDP broadcast discovery on port 3004 (Windows-native).

**Resolution:** By design — UDP broadcast preferred for Windows, mDNS for Linux/macOS.

---

### Service Template Validation

**Issue:** Template validation does not catch all Docker Compose syntax errors until runtime.

**Impact:** Invalid templates accepted during `refresh`, fail during `offer` installation.

**Workaround:** Pre-test templates with `docker compose config --file <template>`.

**Planned fix:** Add Docker Compose syntax validation during template refresh.

---

### Resource Monitor Thresholds

**Issue:** Stone capacity thresholds (Mini/Standard/Large) hardcoded in daemon.

**Impact:** Cannot customize RAM limits or container count warnings.

**Workaround:** Edit `/etc/zen-garden/moss.toml` (requires daemon restart).

**Planned fix:** Add `[capacity]` section to config file with customizable thresholds.

---

### Health Check Restart Loop Detection

**Issue:** Restart loop detection (>3 restarts in 10 min) marks service as Degraded but does not auto-remediate.

**Impact:** Operators must manually investigate and restart service.

**Workaround:** Use `garden-rake observe` to identify degraded services, then `garden-rake remove <name>` and `garden-rake offer <name>` to reinstall.

**Planned fix:** Add `:heal` action to automatically restart degraded services.

---

### Port Conflict Resolution Logging

**Issue:** Port conflict resolution (27017 → 27018) logs warning but does not update mDNS TXT record immediately.

**Impact:** mDNS announcements show incorrect port until next health check cycle (30s delay).

**Workaround:** Wait 30 seconds for mDNS update, or query `/api/v1/services/:name` for actual port.

**Planned fix:** Trigger mDNS re-announcement immediately after port conflict resolution.

---

## Deprecation Timeline

### Deprecated in 0.1.0

**Old API endpoints (pre-v1):**

| Deprecated Endpoint | Replacement | Removal Target |
|---------------------|-------------|----------------|
| `GET /api/offerings` | `GET /api/v1/offerings` | 0.2.0 (2027 Q1) |
| `GET /api/offerings/{name}` | `GET /api/v1/offerings/{name}` | 0.2.0 (2027 Q1) |
| `POST /api/offerings/refresh` | `POST /api/v1/offerings:refresh` | 0.2.0 (2027 Q1) |
| `GET /api/templates` | `GET /api/v1/services/manifests` | 0.2.0 (2027 Q1) |
| `GET /api/templates/{name}` | `GET /api/v1/services/{name}/manifest` | 0.2.0 (2027 Q1) |
| `GET /api/services` | `GET /api/v1/services` | 0.2.0 (2027 Q1) |

Old endpoints still function (forward to v1 handlers) and return deprecation warnings in response headers:

```http
X-Deprecated: true
X-Replacement: /api/v1/offerings
X-Removal-Date: 2027-01-15
```

Migration deadline: January 15, 2027 (12 months).

### Planned Deprecations

No additional deprecations planned at this time.

When new deprecations occur:
1. Announce deprecation 12 months before removal
2. Add deprecation headers to old endpoints
3. Update documentation with migration guides
4. Provide CLI warnings for deprecated command usage

---

## Upcoming Releases

- **0.2.0 (Phase 2):** Production hardening
  - Health monitoring (background task)
  - Resource monitoring (capacity warnings with auto-remediation)
  - Port conflict resolution (real-time mDNS updates)
  - Atomic compose updates with rollback
  - Enhanced CLI (`--all` parallel execution, `--json` output, progress indicators)

- **0.3.0 (Phase 3):** Advanced features
  - Lantern UI integration (dashboard, topology visualization)
  - HTTPS listener on :7187 with route splitting (public vs authenticated)
  - Cursor-based polling optimization (delta updates)
  - Lifecycle event broadcasting (moss_online, moss_offline)
  - Client bindings (Python, JavaScript, .NET)
  - Prometheus metrics (extended telemetry)

**Feature requests:** Submit proposals to `docs/proposals/` with ADR format.

**Bug reports:** File issues with reproduction steps, environment details (OS, Moss/Rake versions), and logs.
