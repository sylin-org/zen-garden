---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-02-16
---

# ORCH-0001: Replant Ceremony — Offering State Transfer Between Stones

**Date**: 2026-02-16
**Status**: Proposed
**Extends**: ORCH-0001 (Offering Orchestration), Phase 4 — Pull-Based Sync
**Depends on**: STORAGE-0006 (Seed Bank Replication)

## Context

Zen Garden's offering orchestration system (ORCH-0001 Phases 1–3) assigns roles to offerings across stones: Primary, Dormant, Joining, Degraded. When a Dormant replica needs to catch up with the Primary, or when a new stone joins the garden and needs a copy of an offering, **state must transfer between stones**.

The codebase already has:

- **Harvest system** (`src/moss/src/infra/harvest.rs`): Captures an offering's state (Docker image commit + volume archives) to local disk. Used today for pre-nourishment rollback.
- **Ceremony framework** (`src/moss/src/domain/ceremony/`): Three-phase, journaled, crash-recoverable orchestration for long-running operations. Already defines `CeremonyType::Replant` and `CeremonyType::Store` — both unimplemented.
- **Job system** (`src/moss/src/app_state.rs`): In-memory job tracking with SSE event streaming for progress.
- **Seed-bank system** (`src/moss/src/infra/storage/`): USB-mounted portable storage with retention-managed snapshot indices (default: 5 snapshots per offering).
- **Capability mirroring** (`src/moss/src/api/v1/offering_capabilities.rs`): `POST .../capabilities/mirror` already copies capability data between stones for Adopted offerings.

**What's missing**: No mechanism to transfer the harvest artifacts (volume archives, committed images) between stones over HTTP. The harvest system writes to local disk only.

## Decision

Implement `CeremonyType::Replant` as a three-phase ceremony that transfers a Managed offering's full state between stones. The ceremony follows the existing Nourish pattern (Collect → Transfer → Plant) and reuses harvest infrastructure for state capture.

### Terminology

| Term | Definition | Implementation |
|------|-----------|----------------|
| **Harvest** | Capture an offering's state to local disk (volumes + container image) | `create_harvest()` — already exists |
| **Collect** | Harvest + stage artifacts for remote retrieval | New: API endpoints on source stone |
| **Plant** | Apply harvested state on a target stone (extract volumes, recreate container, health-check) | New: extends `restore_harvest()` with container lifecycle |
| **Replant** | Full flow: collect on source → transfer → plant on target | `CeremonyType::Replant` — ceremony wrapper |
| **Store** | Collect to a seed-bank (portable offline storage) | `CeremonyType::Store` — harvest + write to seed-bank mount |

### Source Resolution — Fallback Chain

When `garden-rake replant <offering>` is invoked without an explicit source, the system resolves the source automatically:

```
1. Seed-bank with latest snapshot?
   ├─ Yes → Plant from seed-bank (local I/O, fastest)
   └─ No  → continue

2. Live primary in topology?
   ├─ Yes → Collect from primary → Transfer → Plant
   └─ No  → continue

3. Seed-bank with ANY snapshot (even stale)?
   ├─ Yes → Plant from seed-bank (warn: data may be outdated)
   └─ No  → Fail: "No source available for <offering>"
```

"Latest snapshot" = newest entry in the `RemoteSnapshotIndex` (sorted by `created_at`, position 0). No age threshold — the retention system guarantees the N most recent are available (default N=5 per offering).

For stale seed-bank fallback (step 3), Rake shows:
```
⚠ No live primary for "mongodb". Planting from seed-bank "garden-data"
  (harvest age: 2 days, 14 hours). Data may be outdated.
  Continue? [Y/n]
```

For orchestration-triggered (automatic) replants, skip the prompt and log the warning.

### CLI Grammar

```
garden-rake replant <offering[:instance]> [from stone <name> | from seed-bank <name>] [to <stone>]
```

| Clause | Required | Default |
|--------|----------|---------|
| `offering[:instance]` | Yes | — |
| `from stone <name>` | No | Primary from topology |
| `from seed-bank <name>` | No | Auto-detect (fallback chain above) |
| `to <stone>` | No | Tended stone |

**Examples:**

```bash
garden-rake replant mongodb                                             # from primary, to tended stone
garden-rake replant mongodb from stone stone-amber-ridge                # explicit source stone
garden-rake replant mongodb from seed-bank garden-data                  # from seed-bank
garden-rake replant mongodb:dev from seed-bank offsite to stone-coral   # full explicit
```

### Ceremony Phases

The Replant ceremony has three phases, following the existing ceremony pattern in `src/moss/src/domain/ceremony/`:

#### Phase 1: Collect (on source stone)

**For Managed offerings (containers):**

1. **Identify local mounts**: Call `docker.get_container_volumes(offering)` → list of `(host_path, container_path)` bind mounts. Offerings using the Storage API for files are unaffected — their data lives on seed-bank mounts, not container volumes.

2. **Commit container snapshot**: Call `docker.commit_container(container_name, repo, tag, pause=true)` → captures container filesystem state as a local Docker image. The `pause=true` flag ensures data consistency.

3. **Archive volumes**: For each bind mount, `archive::create_archive(host_path, archive_path)` → `.tar.gz` with BLAKE3 checksum.

4. **Record manifest**: Save `HarvestManifest` with offering name, original image, committed image tag, volume archive list (paths, sizes, checksums), source stone ID, timestamp.

This is exactly what `create_harvest(docker, store, offering, source_stone, commit_image=true)` already does.

**For Adopted offerings (capabilities):**

No harvest needed. Capability mirroring via `mirror_offering_capabilities_v1` handles this path (already implemented).

**For Borrowed offerings:**

Not replicable. Skip — `replicable: false` prevents orchestration from attempting sync.

**Collect is a Job.** The target stone triggers it via API. The source stone runs it asynchronously, emitting `JobEvent::Started`, `JobEvent::Progress`, `JobEvent::Completed`/`Failed` through the event bus. The target stone polls for completion.

#### Phase 2: Transfer (target stone pulls from source)

1. **Poll job status** on source stone until collect completes.
2. **Download harvest manifest** from source → learn volume list, sizes, checksums.
3. **Stream volume archives** from source → write to local harvest store. Use `tokio::io::copy` with streaming response body — never buffer full archives in memory (volumes can be gigabytes).
4. **Verify checksums** on all downloaded archives using `archive::verify_checksum()`.
5. **Ensure Docker image available**: For registry-sourced images (common case), `docker.pull_image(original_image)`. For committed snapshots, `docker save` on source → stream → `docker load` on target.

**Seed-bank path**: When the source is a seed-bank, skip steps 1–3. Read harvest manifest and volume archives directly from `{seed_bank_mount}/garden/harvests/{offering}/`. No network transfer needed.

#### Phase 3: Plant (on target stone)

1. **Stop local container**: `docker.stop_service(offering)` — if the offering is already running locally (Joining/Dormant state).
2. **Remove local container**: `docker.remove_service(offering)` — clean slate.
3. **Restore volumes**: `restore_harvest(docker, store, harvest_id)` — extracts archived volumes to host paths, verifies checksums.
4. **Recreate container**: `docker.install_service(offering, image, ports, env, volumes)` — using the same configuration as the source (ports, env vars, volume mounts from the harvest manifest or the offering's compiled template).
5. **Start and health-check**: `docker.start_service(offering)` → poll health for up to 120s (every 3s), matching the existing "Water" phase in nourishment ceremonies (`src/moss/src/domain/ceremony/phases/water.rs`).
6. **Rollback on failure**: If health checks fail and `auto_rollback` is enabled, stop the new container and restore previous state (if any existed).
7. **Transition role**: On success, call `transition_role(state, offering_id, fqn, OfferingRole::Dormant)` (or `Primary` if this is a promotion). Emit `OfferingEvent::RoleChanged`.

### New API Endpoints (Source Stone)

Three new endpoints on the source stone to support remote collection and artifact retrieval:

#### `POST /api/v1/stone/offerings/:name/harvest`

Triggers an asynchronous harvest (collect). Returns immediately with job tracking info.

**Request:** (no body required — harvests the current state)

**Response:**
```json
{
  "data": {
    "job_id": "01JMBN...",
    "harvest_id": "mongodb-20260216T143000-a1b2c3",
    "offering": "mongodb",
    "status": "pending"
  }
}
```

**Implementation:** Spawns `create_harvest()` in a background task, tracked via the job system. Emits job events through the event bus.

**Concurrency guard:** Reuse `ceremony_registry.has_active_for_offering()` — only one harvest per offering at a time.

#### `GET /api/v1/stone/offerings/:name/harvest/:id/status`

Polls harvest job status.

**Response:**
```json
{
  "data": {
    "harvest_id": "mongodb-20260216T143000-a1b2c3",
    "status": "completed",
    "manifest": {
      "offering": "mongodb",
      "original_image": "mongo:7",
      "committed_image": "zen-harvest/mongodb:20260216T143000",
      "volumes": [
        { "name": "data", "container_path": "/data/db", "size_bytes": 524288000, "checksum": "blake3:abc123..." }
      ],
      "total_size_bytes": 524288000,
      "created_at": "2026-02-16T14:30:00Z"
    }
  }
}
```

When `status` is not `"completed"`, `manifest` is `null`.

#### `GET /api/v1/stone/offerings/:name/harvest/:id/artifact/:artifact_name`

Streams a single artifact (volume archive or committed image) as binary.

**Parameters:**
- `:artifact_name` — volume archive name (e.g., `data.tar.gz`) or `image.tar` for the committed Docker image.

**Response:** `200 OK` with `Content-Type: application/octet-stream`, `Content-Length`, streamed body.

**Implementation:** `tokio::fs::File` → `ReaderStream` → axum `Body::from_stream()`. No buffering.

### Integration with Orchestration Task

The orchestration task (`src/moss/src/tasks/offering_orchestration.rs`) drives sync automatically:

#### Joining State

When an offering is in Joining state (`assign_initial_role` detected a primary elsewhere):

```rust
OfferingRole::Joining => {
    // Trigger a Replant ceremony from the primary
    // This is a one-time full sync
    dispatch_joining_sync(state, offering_id, fqn, orch).await?;
    // On completion: transition_role(Dormant)
}
```

#### Dormant State

Dormant offerings already watch the primary heartbeat. Add periodic sync check:

```rust
OfferingRole::Dormant => {
    dispatch_dormant(state, offering_id, fqn, orch).await?;  // existing heartbeat

    // Periodic sync check (every SYNC_CHECK_INTERVAL_SECS = 60s)
    if should_sync_check(orch) {
        dispatch_dormant_sync(state, offering_id, fqn, orch).await?;
    }
}
```

For Dormant sync, **do NOT do a full replant every 60s**. Instead:
- Check the primary's cursor (via `GET /api/v1/stone/offerings/:name/cursor`)
- If cursor matches local → skip (already in sync)
- If cursor differs → trigger a Replant ceremony (full re-sync)

**Cursor definition for Managed offerings**: The `HarvestManifest.created_at` timestamp of the last successful harvest on the primary. The cursor endpoint returns this value. A Dormant replica compares its `sync_cursor` (set after last successful plant) against the primary's.

### Sync Fields on OrchestrationState

Extend `OrchestrationState` in `src/common/src/types.rs` (backward-compatible via `#[serde(default)]`):

```rust
pub struct OrchestrationState {
    pub role: OfferingRole,
    pub primary_stone_id: Option<String>,
    pub pinned: bool,
    pub pin_timestamp: Option<String>,

    /// ISO 8601 timestamp of the last successfully planted harvest.
    /// Compared against primary's cursor to detect sync drift.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_cursor: Option<String>,

    /// Sync method used: "harvest" (Managed), "capabilities" (Adopted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_method: Option<String>,

    /// ISO 8601 timestamp of last sync check (throttle polling).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_check: Option<String>,
}
```

### Cursor Endpoint

```
GET /api/v1/stone/offerings/:name/cursor
```

**Response:**
```json
{
  "data": {
    "cursor": "2026-02-16T14:30:00Z",
    "method": "harvest",
    "harvest_id": "mongodb-20260216T143000-a1b2c3"
  }
}
```

**Implementation:** Reads the offering's `OrchestrationState.sync_cursor` (for Primary, this is updated each time a harvest is created). For Adopted offerings, returns `capability_revision` as the cursor.

### Seed-Bank Path

When the source is a seed-bank:

1. **Store (collect to seed-bank)**: `CeremonyType::Store` — runs `create_harvest()` then copies artifacts to `{seed_bank_mount}/garden/harvests/{offering}/{harvest_id}/`.
2. **Plant from seed-bank**: The Replant ceremony's Transfer phase reads directly from the seed-bank mount instead of HTTP. `HarvestStore::new(seed_bank_mount.join("garden/harvests"))` provides the same interface.

The `NurturingScheduler` already replicates to seed-banks with retention management (`add_with_retention()`, default 5 slots per offering). This ADR extends that to also store harvest artifacts alongside the existing remote snapshots.

### Adopted Offering Sync (Capabilities)

For Adopted offerings, sync is simpler — no harvest/transfer/plant cycle:

1. Dormant replica calls `mirror_offering_capabilities_v1` with `from=primary_stone, to=self`
2. Capabilities are mirrored via existing HTTP fan-out
3. `sync_cursor` set to `capability_revision` from the primary
4. Joining → Dormant transition on successful mirror

No new API endpoints needed. The mirror endpoint already exists at `POST /api/v1/stone/offerings/:name/capabilities/mirror`.

## Implementation Plan

### Step 1: OrchestrationState sync fields
- **File**: `src/common/src/types.rs`
- **Change**: Add `sync_cursor`, `sync_method`, `last_sync_check` to `OrchestrationState`
- **Test**: Deserialize existing JSON without sync fields (backward compat)

### Step 2: Cursor endpoint
- **Files**: New handler in `src/moss/src/api/v1/offerings.rs` (or new file), route in `src/moss/src/bootstrap/router.rs`
- **Change**: `GET /api/v1/stone/offerings/:name/cursor` → returns sync cursor from offering state
- **Test**: Unit test handler with mock state; integration test with running offering

### Step 3: Harvest API endpoints (source stone)
- **Files**: New handler file `src/moss/src/api/v1/harvest.rs`, routes in router
- **Change**: Three endpoints — `POST .../harvest`, `GET .../harvest/:id/status`, `GET .../harvest/:id/artifact/:name`
- **Test**: Unit test harvest trigger; integration test full collect → download cycle
- **Key concern**: Streaming artifact download must not buffer in memory. Use `tokio::fs::File` → `ReaderStream`.

### Step 4: Adopted sync (capabilities mirror in orchestration task)
- **File**: `src/moss/src/tasks/offering_orchestration.rs`
- **Change**: In Joining dispatch, call capability mirror for Adopted offerings. On success, transition to Dormant.
- **Test**: Integration test — two stones, adopted offering, verify capabilities mirrored

### Step 5: Replant ceremony — Plant phase
- **Files**: New `src/moss/src/domain/ceremony/phases/plant.rs`, update `src/moss/src/domain/ceremony/mod.rs`
- **Change**: Stop container → restore harvest → recreate container → health-check → role transition
- **Test**: Unit test with mock Docker; integration test full plant cycle

### Step 6: Replant ceremony — Full flow
- **Files**: New `src/moss/src/domain/ceremony/replant.rs`
- **Change**: Wire Collect → Transfer → Plant into `CeremonyType::Replant` handler
- **Test**: Integration test — two stones, managed offering, verify full replant

### Step 7: Wire into orchestration task
- **File**: `src/moss/src/tasks/offering_orchestration.rs`
- **Change**: Joining dispatch triggers Replant ceremony. Dormant dispatch adds periodic sync check.
- **Test**: Integration test — deploy on stone-01, deploy on stone-02, verify Joining → sync → Dormant

### Step 8: Rake `replant` command
- **Files**: New `src/rake/src/commands/replant.rs`, update `src/rake/src/main.rs`
- **Change**: CLI grammar with source resolution fallback chain
- **Test**: Unit test argument parsing; integration test with tended stone

### Step 9: Seed-bank integration
- **Files**: `src/moss/src/infra/nurturing_store.rs`, ceremony Store handler
- **Change**: Store harvest to seed-bank; plant from seed-bank
- **Test**: Integration test with mounted seed-bank

## Existing Code to Reuse

| Component | Location | Reuse |
|-----------|----------|-------|
| `create_harvest()` | `src/moss/src/infra/harvest.rs` | Phase 1 (Collect) — use directly |
| `restore_harvest()` | `src/moss/src/infra/harvest.rs` | Phase 3 (Plant) — volume restoration |
| `HarvestStore` | `src/moss/src/infra/harvest_store.rs` | Manifest + archive storage on disk |
| `HarvestManifest` | `src/moss/src/domain/harvest.rs` | Harvest metadata, checksums, sizes |
| `DockerManager` | `src/moss/src/docker.rs` | `commit_container`, `get_container_volumes`, `stop_service`, `start_service`, `install_service`, `pull_image`, `get_container_recreate_config` |
| `archive::create_archive` / `extract_archive` | `src/common/src/infra/archive.rs` | Compression, checksumming |
| Ceremony framework | `src/moss/src/domain/ceremony/` | `Ceremony`, `CeremonyState`, `Phase`, `CeremonyRegistry`, `CeremonyJournal` |
| Phase 3 Water | `src/moss/src/domain/ceremony/phases/water.rs` | Health-check polling + rollback pattern |
| Job events | `src/moss/src/api/v1/events.rs` | `emit_job_started`, `emit_job_progress`, `emit_job_completed`, `emit_job_failed` |
| `mirror_offering_capabilities_v1` | `src/moss/src/api/v1/offering_capabilities.rs` | Adopted sync — use directly |
| `resolve_stone_endpoint()` | `src/moss/src/api/v1/offering_capabilities.rs` | Cross-stone HTTP base URL from topology |
| `NurturingScheduler` | `src/moss/src/tasks/nurturing_scheduler.rs` | Seed-bank replication pattern |
| `RemoteSnapshotIndex` | `src/moss/src/domain/nurturing.rs` | Retention-managed snapshot list (5 slots) |
| `transition_role()` | `src/moss/src/tasks/offering_orchestration.rs` | Role transition with event emission |
| `parse_offering_fqn()` | `src/common/src/offerings.rs` | FQN parsing (`name:instance`) |

## Consequences

**Positive:**
- Reuses existing harvest, ceremony, and job infrastructure — no parallel systems
- Pull-based: replicas pull from primaries, consistent with garden philosophy
- Seed-bank fallback provides offline resilience (primary gone, data recoverable)
- Streaming artifact transfer handles arbitrarily large volumes
- Ceremony journal provides crash-recovery for interrupted replants
- Same mechanism serves both automated orchestration sync and manual `garden-rake replant`

**Negative:**
- Full volume re-transfer on every sync (no incremental/delta yet). Acceptable for initial implementation; optimize later.
- Container downtime during Plant phase (stop → restore → start). Acceptable for Joining (not serving traffic). For Dormant re-sync, minimize by only syncing when cursor differs.
- Committed Docker images can be large (hundreds of MB). Registry-sourced images should `docker pull` instead of transferring committed snapshots when possible.

**Future optimization:**
- Incremental volume sync (rsync-like delta or changed-file tracking)
- Parallel volume archive streaming (multiple volumes simultaneously)
- Cursor-based partial sync (only changed capabilities for Adopted)

## References

- Execution prompt: `docs/proposals/offering-orchestration/ORCH-EXECUTION-PROMPT.md` (Phase 4)
- Spec: `docs/proposals/offering-orchestration/ORCH-0001-offering-orchestration.md`
- Harvest implementation: `src/moss/src/infra/harvest.rs`
- Ceremony framework: `src/moss/src/domain/ceremony/`
- Nourishment ceremony: `src/moss/src/domain/ceremony/nourish.rs`
- Docker operations: `src/moss/src/docker.rs`
- Capability mirror: `src/moss/src/api/v1/offering_capabilities.rs`
- Nurturing scheduler: `src/moss/src/tasks/nurturing_scheduler.rs`
- Orchestration task: `src/moss/src/tasks/offering_orchestration.rs`
- Job events: `src/moss/src/api/v1/events.rs`
