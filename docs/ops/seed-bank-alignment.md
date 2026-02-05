# Seed Bank Alignment Proposal (Greenfield)

**Objective**  
Realign seed bank storage layout and APIs to the canonical model while preserving the core goal: portable, moveable, auto‑discoverable storage for the garden that supports both infrastructure (nurturing/restore) and user/solution storage (S3).

This is a greenfield realignment. There is **no backcompat, shim, or migration** for legacy layouts.

---

**Progress (2026‑02‑05)**
- Implemented `garden/storage/{bucket}/{key}` as the S3 root (ObjectStore + S3 gateway).
- Moved S3 gateway to `/api/v1/storage/s3/*`.
- Removed `X-App-Name`; added optional `X-Seed-Bank` / `seed-bank` selector.
- Seed bank prepare now initializes `garden/memories` + `garden/storage`.
- Nurturing replication uses `garden/memories/{offering_id}/{harvest_id}.tar.gz`.
- Added canonical layout validation to reject non‑canonical seed banks.
- Implemented REST `/api/v1/storage/*` gateway (SDK surface) with auto‑routing.
- Added traversal/path validation for stone, REST, and S3 storage paths.
- Added alignment notes to core proposal docs.
- Added read‑only `/api/v1/memories/*` gateway for hydration access (auto‑routing).
- Added hydration metadata snapshots under `garden/memories/{offering_id}/offering.json`.
- Added audit logging for memories access (no auth yet).

Remaining gap: hydration orchestration flow (auto-install + restore from memories).

---

**Target End State**

**Canonical Layout**
- `garden/memories/{offering_id}/...` is the only location for nurturing backups.
- `garden/memories/{offering_id}/offering.json` captures hydration metadata.
- `garden/storage/{bucket}/{key}` is the root for all S3 operations.
- `.zen-garden/manifest.json` remains JSON and is the source of truth for seed bank identity and configuration.

**SDK vs S3 Split**
- SDKs use the REST storage API for app‑scoped logical paths (default `{app}/{bucket}/...`).
- S3 gateway is bucket‑rooted only and maps strictly to `garden/storage/{bucket}/{key}`.
- No server‑side app isolation. Multitenancy is by intent.

**Operational Invariants**
- Seed banks are auto‑discoverable and auto‑mountable.
- A seed bank is either valid (canonical layout) or rejected for use.
- Nurturing and S3 operations never touch legacy `apps/` paths.
- Storage resolution is automatic: if stone‑01 receives a write and the default seed bank is attached to stone‑02, stone‑01 routes the request to stone‑02 without user intervention.
- `garden/offerings/` is reserved for a directory of active, ready‑to‑use offerings (not backups).
- `garden/memories` is exposed read‑only via `/api/v1/memories/*` with audit logging (no auth yet).

**Connection Strings (Storage)**
- Default (unnamed seed bank): `zen-garden:storage//{path}`
- Named seed bank: `zen-garden:storage//flower-meadow:{path}`

---

**Current Gaps (Remaining)**

| Area | Target | Current | Impact |
|---|---|---|---|
| Hydration orchestration | Auto‑install + restore from memories | Not implemented | External orchestrator must wire steps manually |
| Docs cleanup | Canonical paths across all docs | Updated across guides/specs/proposals | Low drift |

---

**Missing / Incomplete Code Paths (Seed‑Bank‑Relevant)**
1. Hydration orchestration workflow (auto‑install + restore from memories).
2. Documented operational gaps in nurturing (retention config, routing strategy, restore commands, health metrics).  
   Evidence: `docs/TODO-NURTURING-GAPS.md`.

---

**Alignment Plan**

**Phase 1 — Canonical Paths (Core)**
1. Add canonical seed bank path constants in `src/common/src/constants/paths.rs`.
2. Update `ObjectStore` to support a configurable root and use `{mount}/garden/storage`.
3. Update S3 gateway (garden scope):
   - Move S3 gateway to `/api/v1/storage/s3/*` (garden surface).
   - Remove `X-App-Name` requirement.
   - Map `s3://{bucket}/{key}` to `{seed-bank}/garden/storage/{bucket}/{key}`.
   - Enforce traversal protection and deny access outside `garden/storage`.
4. Update nurturing replication to write under `garden/memories/{offering_id}/{harvest_id}/`.
5. Update seed bank preparation to create `garden/memories` and `garden/storage`.
6. Add seed bank layout validation in `src/moss/src/api/v1/storage.rs` (and/or registry scan) to enforce canonical directories.

**Phase 2 — Strict Canonical Enforcement**
1. Reject non‑canonical seed banks during scan/mount.
2. Fail fast on legacy paths and return explicit “re‑prepare seed bank” errors.
3. Remove all legacy `apps/` references in seed bank code paths.

**Phase 3 — Routing and Remote Access**
1. Implement automatic routing using storage announcements (Storage Beacon + cache):
   - Default seed bank selection resolves to the stone that currently hosts it.
   - Requests received by any stone route to the owning stone automatically.
2. Implement named seed bank selection with `StorageCache` lookup.
3. Add proxy routing for remote seed banks (stone‑to‑stone HTTP hop).
4. Extend REST and S3 APIs with an explicit seed bank selector (header or query param) for named banks.

**Phase 4 — Namespace Enforcement + Audit**
1. Reject path traversal and absolute paths.
2. Enforce:
   - External requests can only access `garden/storage/*`.
   - `garden/memories/*` is exposed read‑only via `/api/v1/memories/*`.
3. Audit all memories access (requesting stone metadata where available).

**Phase 5 — Operational Completeness**
1. Implement retention configuration and routing strategy in moss config.
2. Implement missing Rake restore and scheduling commands.
3. Add seed bank health checks and capacity alerts.
4. Update docs and operator guides to the canonical layout and API semantics.

---

**Acceptance Criteria**
1. A prepared seed bank always contains `garden/memories` and `garden/storage`.
2. Nurturing replication stores backups under `garden/memories` only.
3. S3 bucket `a1` maps to `{seed-bank}/garden/storage/a1/...`.
4. `X-App-Name` is not required or used by the S3 gateway.
5. `/api/v1/memories/*` exposes read‑only access to backups with audit logging.
6. A seed bank with legacy `apps/` layout is rejected and provides a clear error.
7. A request received by stone‑01 for the default seed bank routes to stone‑02 if the bank is attached there (no name required).
8. `/api/v1/storage/*` is the canonical garden storage surface; `/api/v1/stone/storage/*` is local management only.

---

**Concrete File Touchpoints (Non‑Exhaustive)**
- Paths and constants: `src/common/src/constants/paths.rs`
- Seed bank prepare: `src/moss/src/api/v1/storage.rs`
- Object store I/O: `src/moss/src/infra/storage/objects.rs`
- S3 gateway: `src/moss/src/api/v1/s3_gateway.rs`
- Nurturing replication: `src/moss/src/infra/nurturing_store.rs`
- Routing cache: `src/moss/src/domain/storage_cache.rs`
- Probes/tests: `src/probe/src/ssh.rs`, `src/probe/src/tests/nurturing.rs`
- Docs: `docs/specs/STORAGE-0001-seed-bank-onboarding.md`, `docs/guides/seed-banks.md`, `docs/proposals/zen-garden-spec-cultivation.md`, `docs/proposals/zen-garden-spec-storage-api.md`

---

**Open Questions**
1. Canonical index format for `garden/memories`: per‑offering index vs `garden/index.json`.
2. `latest` symlink support vs versioned directories only.

---

**API Scope Investigation (Stone / Garden / Storage)**

**Stone‑scoped** (`/api/v1/stone/*`) — local‑only operations  
Evidence: `src/moss/src/bootstrap/router.rs`, `src/moss/src/api/v1/*`  
- Storage: `/api/v1/stone/storage/*` (candidates, prepare, bank CRUD, object ops, S3 gateway)  
- Nurturing: `/api/v1/stone/nurturing/*`  
- Services, offerings, companions, presence, nourishment, etc.

**Garden‑scoped** (`/api/v1/garden/*`) — aggregated/cluster view  
Evidence: `src/moss/src/api/v1/garden.rs`, `src/moss/src/api/v1/services.rs`  
- Topology, stone info, service discovery, placement recommendations, garden‑level nourishment

**Storage‑scoped** (`/api/v1/storage/*`) — documented but not implemented as such  
Evidence: `docs/specs/api-v1.md`, `docs/proposals/zen-garden-spec-storage-api.md`  
- Spec describes `/api/v1/storage/{path}` as the S3/REST storage surface.
- Code currently implements local storage under `/api/v1/stone/storage/*` plus the S3 gateway at `/api/v1/storage/s3/*`.

**Implication for Realignment**
- `/api/v1/storage/*` becomes the canonical garden storage surface (SDK + S3).  
- `/api/v1/stone/storage/*` is retained for local storage management only.  
- `/api/v1/stone/storage/s3/*` is removed.  
- Automatic routing must work across stones (stone‑01 → stone‑02) for both default and named seed banks.
