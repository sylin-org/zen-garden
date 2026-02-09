---
audience: [developer, contributor]
doc_type: proposal
status: ongoing
created: 2026-02-09
---

# Caretaking: Automated Maintenance Sweep Pipeline

## Problem

Stones accumulate cruft over time: stale staging files, dangling Docker images, old backup binaries, orphaned task entries. Without automated cleanup, this degrades performance and disk usage. Currently requires SSH to diagnose and fix.

Compounding this: `get_current_compat_capabilities()` shells out to `docker images` on every call (subprocess per invocation). During startup adoption of N containers, this fires N times — the primary cause of slow startups on stones with many Docker images.

**Evidence**: stone-coral-prairie investigation found:
- 2 stale `.staged` files (31 MB, from Jan 30) in staging/
- 31 Docker images (many dangling `zen-harvest/*` snapshots)
- 1 stale `.backup` binary (17 MB, from Jan 25)
- "Scanning docker images for AI runtimes..." log spam — dozens of subprocess calls during startup

## Solution: Caretaking

A domain-pluggable maintenance pipeline called **caretaking**. Each domain contributes a sweeper function. The orchestrator runs them sequentially, times each one, aggregates results, and persists reports to disk.

### Contract (minimal by design)

```
SweepStatus: Healthy | Degraded | Unhealthy | Failed
SweepReport: { domain, status, notes[] }
SweepRun:    { timestamp, duration_ms, overall_status, reports[] }
```

Each domain implements: `async fn sweep_X(ctx: &SweepContext) -> SweepReport`

The domain checks its own state, cleans what it can, and self-assesses. The orchestrator adds timing, error catching, and persistence. That's it.

### Persistence

One JSON file per sweep run: `{data_dir}/maintenance/sweep-{timestamp}.json`
- Write-once (no read-modify-write)
- Natural sort by filename
- Pruned to last 20 files by the orchestrator
- No in-memory cache (cold data, read on API request)

### Triggers

1. **Scheduled**: Every 1 hour (5 min delay after boot)
2. **On-demand**: `POST /api/v1/stone/maintenance/sweep` — returns the fresh report
3. **History**: `GET /api/v1/stone/maintenance/history` — returns last N runs

### UX

**Stone Portrait** (web UI): "Maintenance" section shows last sweep status + timestamp. Click for full history with per-domain breakdown.

**Rake CLI** (future): `rake caretake` hits POST endpoint, displays per-domain results.

**API consumers**: Poll history endpoint for monitoring/alerting on degraded stones.

### Initial Sweepers

| Sweeper | Domain | What it does |
|---------|--------|-------------|
| `sweep_staging` | Deployment | Deletes `.staged` files older than 24h |
| `sweep_docker` | Docker | Prunes dangling images (bollard `prune_images`) |
| `sweep_binaries` | System | Deletes `.backup` files older than 7 days |
| `sweep_task_history` | Tasks | Cleans orphaned task entries (>30 days, offering removed) |

### Compat Capabilities Cache Fix (bundled)

Separate from sweeping but addresses the same root cause (stone-coral-prairie slowdown):
- `get_current_compat_capabilities()` reads from `AppState.capabilities` cache when `detection_status == Complete`
- Falls back to live detection only during early startup (before hardware detection completes)
- Eliminates N subprocess calls per startup adoption cycle

## Architecture

```
domain/maintenance.rs     — SweepStatus, SweepReport, SweepRun, SweepContext
                            run_sweep(), worst_status()
                            sweep_staging(), sweep_docker(), sweep_binaries(), sweep_task_history()

infra/maintenance_store.rs — save_sweep_run(), load_sweep_history(), prune_old_sweeps()

api/v1/maintenance.rs      — GET history, POST sweep

tasks/coordinator.rs       — start_maintenance_sweep() (hourly background task)
```

Domain owns the sweep logic. Infra owns persistence. API is a thin pass-through. Coordinator schedules.

## Files

### New files
- `src/moss/src/domain/maintenance.rs` — types, orchestrator, 4 sweepers
- `src/moss/src/infra/maintenance_store.rs` — file persistence
- `src/moss/src/api/v1/maintenance.rs` — 2 endpoints

### Modified files
- `src/moss/src/domain/mod.rs` — `pub mod maintenance`
- `src/moss/src/infra/mod.rs` — `pub mod maintenance_store`
- `src/moss/src/api/v1/mod.rs` — `pub mod maintenance`
- `src/moss/src/bootstrap/router.rs` — 2 new routes
- `src/moss/src/tasks/coordinator.rs` — `start_maintenance_sweep()`
- `src/moss/src/domain/compatibility.rs` — cache-aware `get_current_compat_capabilities()`
- `src/moss/src/domain/adoption.rs` — pass cached capabilities
- `src/moss/src/domain/offerings.rs` — pass cached capabilities
- `src/moss/src/domain/reconciliation.rs` — pass cached capabilities
- `src/moss/src/tasks/health_monitor.rs` — pass cached capabilities
- `src/moss/src/docker.rs` — `prune_dangling_images()`

## Implementation Order

1. Compat cache fix (Part A) — small, high-impact
2. Domain types + orchestrator (Part B1-B3)
3. Sweepers (Part B4)
4. Persistence store (Part B5)
5. Background task (Part B6)
6. API endpoints (Part B7)

## Verification

```bash
cargo check --all
cargo test --package garden-moss
cargo clippy --all -- -D warnings
```

## Extending

To add a new sweeper:
1. Write `async fn sweep_foo(ctx: &SweepContext) -> SweepReport` in `domain/maintenance.rs`
2. Add it to the `SWEEPERS` list in `run_sweep()`
3. Done — persistence, scheduling, and API exposure are automatic
