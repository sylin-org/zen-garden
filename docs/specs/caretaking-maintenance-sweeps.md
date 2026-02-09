---
audience: [developer, contributor, ai]
doc_type: spec
status: current
created: 2026-02-09
---

# Caretaking: Maintenance Sweep Pipeline

Automated maintenance system that keeps stones lean and responsive. Each domain contributes a sweeper function. An orchestrator runs them sequentially, times each one, aggregates results, persists reports to disk, and exposes them via API.

---

## Architecture

```
domain/maintenance.rs        Types, orchestrator, sweepers (business logic)
infra/maintenance_store.rs   File persistence (save/load/prune)
api/v1/maintenance.rs        HTTP endpoints (history, trigger)
tasks/coordinator.rs         Background scheduling (hourly)
```

Domain owns the sweep logic. Infra owns persistence. API is a thin pass-through. Coordinator schedules.

---

## Contract

### SweepStatus

Each sweeper self-assesses with one of four statuses:

| Status | Severity | Meaning |
|--------|----------|---------|
| `healthy` | 0 | Domain is clean, nothing to do |
| `degraded` | 1 | Cleaned something, or encountered non-fatal errors |
| `unhealthy` | 2 | Domain has problems that need attention |
| `failed` | 3 | Sweeper itself failed to run |

The overall sweep run status is the **worst** (highest severity) across all reports.

### SweepReport

Each sweeper returns:

```json
{
  "domain": "staging",
  "status": "healthy",
  "duration_ms": 12,
  "notes": ["Staging directory clean"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `domain` | string | Sweeper identifier (e.g., `"staging"`, `"docker"`) |
| `status` | SweepStatus | Self-assessment result |
| `duration_ms` | u64 | Wall-clock time for this sweeper (set by orchestrator) |
| `notes` | string[] | Human-readable summary of what happened |

### SweepRun

A complete sweep run, persisted as one JSON file:

```json
{
  "timestamp": "2026-02-09T14:30:00Z",
  "duration_ms": 1250,
  "overall_status": "healthy",
  "reports": [ ... ]
}
```

---

## Sweepers

### `sweep_staging` — Deployment staging cleanup

- Scans `{staging_dir}/*.staged`
- Deletes files older than **24 hours**
- Reports count and bytes freed
- Status: `healthy` if clean, `degraded` if errors during deletion, `failed` if directory unreadable

### `sweep_docker` — Docker image hygiene

- Skips if Docker is unavailable (`subsystems.docker.ready == false`)
- Prunes **dangling** images via bollard `prune_images` API
- Reports count pruned and bytes reclaimed
- Status: `healthy` if clean or pruned, `degraded` if prune API fails

### `sweep_binaries` — Stale backup cleanup

- Scans the directory containing the running binary for `*.backup` files
- Deletes backups older than **7 days**
- Reports count and bytes freed
- Status: always `healthy` (best-effort cleanup)

### `sweep_task_history` — Orphaned task entries

- Loads task registry from `{config_dir}/tasks/registry.json`
- A task is **orphaned** when its offering no longer exists in the registry AND its `last_run` is older than 30 days (or it never ran)
- Removes orphaned entries and persists the cleaned registry
- Status: `healthy` if clean, `degraded` if save fails after cleanup

### `sweep_logs` — Rotated log file cleanup

- Scans `{data_dir}/logs/` for files matching `garden-moss.log.*`
- Deletes rotated log files older than **7 days**
- Reports count and bytes freed
- Status: always `healthy` (best-effort cleanup)

---

## Persistence

One JSON file per sweep run in `{data_dir}/maintenance/`:

```
{data_dir}/maintenance/
  sweep-20260209T143000Z.json
  sweep-20260209T153000Z.json
  ...
```

- **Filename format**: `sweep-{timestamp}.json` (ISO 8601, UTC)
- **Write-once**: No read-modify-write; each run creates a new file
- **Retention**: Pruned to the **last 20 files** after each write
- **No in-memory cache**: Files are read from disk on API request

---

## Scheduling

A background task in `tasks/coordinator.rs`:

| Parameter | Value |
|-----------|-------|
| Boot delay | 5 minutes |
| Interval | 1 hour |
| Blocking | No (runs in `tokio::spawn`) |

The first sweep runs 5 minutes after daemon startup. Subsequent sweeps run every hour. The scheduler calls `run_sweep()` and persists results via `save_sweep_run()`.

---

## API Endpoints

### `GET /api/v1/stone/maintenance/history`

Returns the last N sweep runs (newest first).

**Response** (200):
```json
{
  "data": [
    {
      "timestamp": "2026-02-09T15:30:00Z",
      "duration_ms": 1250,
      "overall_status": "healthy",
      "reports": [
        { "domain": "staging", "status": "healthy", "duration_ms": 5, "notes": ["Staging directory clean"] },
        { "domain": "docker", "status": "healthy", "duration_ms": 1200, "notes": ["Pruned 3 dangling image(s) (150.00 MB reclaimed)"] },
        { "domain": "binaries", "status": "healthy", "duration_ms": 2, "notes": ["No stale backups"] },
        { "domain": "task_history", "status": "healthy", "duration_ms": 1, "notes": ["No orphaned tasks"] },
        { "domain": "logs", "status": "healthy", "duration_ms": 3, "notes": ["No stale log files"] }
      ]
    }
  ]
}
```

### `POST /api/v1/stone/maintenance/sweep`

Trigger an immediate sweep. Returns the fresh report. The result is also persisted to disk.

**Response** (200): Same shape as a single `SweepRun` wrapped in `ApiResponse`.

---

## Adding a New Sweeper

1. Write the async function in `src/moss/src/domain/maintenance.rs`:

```rust
async fn sweep_foo(ctx: &SweepContext<'_>) -> SweepReport {
    // Check state, clean what you can, self-assess
    SweepReport {
        domain: "foo".into(),
        status: SweepStatus::Healthy,
        duration_ms: 0, // overwritten by orchestrator
        notes: vec!["All good".into()],
    }
}
```

2. Add it to the `run_sweep()` function's reports vector:

```rust
let reports = vec![
    run_one_sweeper(sweep_staging, &ctx).await,
    run_one_sweeper(sweep_docker, &ctx).await,
    run_one_sweeper(sweep_binaries, &ctx).await,
    run_one_sweeper(sweep_task_history, &ctx).await,
    run_one_sweeper(sweep_logs, &ctx).await,
    run_one_sweeper(sweep_foo, &ctx).await,  // <-- add here
];
```

3. Done. Persistence, scheduling, and API exposure are automatic.

### Guidelines for sweepers

- **Self-assess honestly**: Return `degraded` or `unhealthy` when appropriate, not just `healthy`
- **Never panic**: Return `SweepStatus::Failed` with an error note instead
- **Be idempotent**: Running twice should produce the same result
- **Gate on subsystem availability**: Check `ctx.state.subsystems.docker.ready` before Docker operations
- **Use `ctx.state`**: Access AppState for offerings, Docker, capabilities, etc.
- **Set `duration_ms: 0`**: The orchestrator overwrites this with actual timing

---

## File Reference

| File | Layer | Purpose |
|------|-------|---------|
| `src/moss/src/domain/maintenance.rs` | Domain | Types, orchestrator, 5 sweepers |
| `src/moss/src/infra/maintenance_store.rs` | Infra | save/load/prune sweep files |
| `src/moss/src/api/v1/maintenance.rs` | API | 2 HTTP handlers |
| `src/moss/src/tasks/coordinator.rs` | Tasks | `start_maintenance_sweep()` background task |
| `src/moss/src/docker.rs` | Infra | `prune_dangling_images()` method |
| `src/moss/src/domain/mod.rs` | Domain | `pub mod maintenance` |
| `src/moss/src/infra/mod.rs` | Infra | `pub mod maintenance_store` |
| `src/moss/src/api/v1/mod.rs` | API | `pub mod maintenance` |
| `src/moss/src/bootstrap/router.rs` | Bootstrap | Route registration |

> Design rationale: [caretaking-sweep-pipeline](../proposals/ongoing/caretaking-sweep-pipeline.md)
