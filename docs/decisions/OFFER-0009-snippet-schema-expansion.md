---
audience: developer
doc_type: decision
status: accepted
---

# OFFER-0009: Offering Snippet Schema Expansion

**Date**: 2026-05-29
**Status**: Accepted

---

## Problem

The offering snippet (`<name>.snippet.yaml`) is a Docker-Compose-style service
body, but `ServiceConfig` (the parsed shape, `src/common/src/manifests/offering.rs:425`)
consumes only a subset of Compose keys. Three gaps force authors to write
configuration that Moss silently discards:

1. **No resource limits.** `DeployConfig` parses only
   `deploy.resources.reservations.devices` (GPU). There is no
   `deploy.resources.limits`, so a memory-heavy offering cannot be capped. The
   FlareSolverr evaluation made this concrete: its single most-recommended
   safeguard is a hard `mem_limit` (it spawns a headless Chromium per request and
   climbs toward ~1.2 GB over a day), and that cap is **impossible to express**
   today. Every memory-hungry offering (JVM services, Elasticsearch, browsers) is
   uncapped, so one runaway container can OOM a small stone shared by other
   offerings.

2. **The snippet `healthcheck:` is decorative.** Every shipped snippet writes a
   `healthcheck:` block, but `ServiceConfig` ignores it. The bollard
   `ContainerCreateBody.healthcheck` field is never set, so Docker never runs the
   author's health probe and `docker ps` shows no health status. Authors
   reasonably assume the block they wrote takes effect; it does not.

3. **Periodic work can only run *inside* the container.** A snippet `tasks:`
   entry runs its `command` via `exec_in_container`. But the canonical mitigation
   for a leaking container (FlareSolverr's nightly restart to reclaim Chromium
   memory) is a Moss-level *restart*, which cannot be expressed — `tasks` has no
   way to say "recycle this container."

## Decision

Expand the snippet schema along the existing `deploy`/`tasks` axes, threading the
new fields through the established
`ServiceConfig → ServiceTemplate → CompiledOffering → ContainerSpec → bollard`
pipeline (the same path GPU `device_requests` already travels).

### 1. Resource limits

Add `deploy.resources.limits` alongside the existing `reservations`:

```yaml
deploy:
  resources:
    limits:
      memory: "2g"      # 512m / 2g / 1073741824 — parsed to bytes
      cpus: "1.5"       # fractional cores
```

- `ResourcesConfig` gains `limits: Option<LimitsConfig>`; `LimitsConfig` carries
  `memory: Option<String>` and `cpus: Option<String>`.
- `memory` accepts a Docker-style size suffix (`k`/`m`/`g`, binary) or a raw byte
  count; parsed to `i64` bytes.
- `cpus` is a decimal core count; parsed to `nano_cpus = round(cpus * 1e9)`.
- These map to bollard `HostConfig.memory` and `HostConfig.nano_cpus` in
  `build_container_config` (`src/moss/src/docker/lifecycle.rs:527`).

### 2. Container healthcheck

Promote the snippet `healthcheck:` block from decorative to functional:

```yaml
healthcheck:
  test: ["CMD", "curl", "-fsS", "http://localhost:8191/health"]
  interval: 30s
  timeout: 10s
  retries: 5
  start_period: 30s
```

- A new `ContainerHealthcheck` parsed struct (named to avoid collision with the
  unrelated application-probe `HealthConfig` in `detection.rs`) carries `test`,
  `interval`, `timeout`, `retries`, `start_period`.
- Duration strings (`30s`, `1m30s`) parse to nanoseconds; `test` maps directly to
  bollard's `Test` vector (`["CMD", ...]` / `["CMD-SHELL", ...]`).
- Maps to `ContainerCreateBody.healthcheck` (bollard `HealthConfig`) in
  `build_container_config`.
- Absent block = no container healthcheck (today's behavior).

### 3. Recycle task action

Add a typed action to `TaskDefinition` (`src/common/src/types/task.rs:33`) rather
than a sentinel command string (code-standard rule 8 — state machines as enums,
not stringly-typed encodings):

```yaml
tasks:
  nightly-recycle:
    description: "Restart the container nightly to reclaim leaked memory"
    schedule: "0 4 * * *"
    action: recycle        # exec (default) | recycle
    category: maintenance
```

- `TaskAction { Exec, Recycle }`, `#[serde(rename_all = "lowercase")]`, default
  `Exec`. `TaskDefinition.action: TaskAction` with `#[serde(default)]`.
- `command` becomes `#[serde(default)]` so a `recycle` task may omit it.
- The single execution branch point — `execute_task`
  (`src/moss/src/tasks/task_scheduler.rs:63`) — matches on `action`: `Exec` keeps
  `exec_in_container`; `Recycle` calls the existing
  `ContainerRuntime::restart_service` (`lifecycle.rs:349`, graceful bollard
  `restart_container`). The scheduler's "container must be Running" gate already
  guards it.

## Implementation Requirements

- **Threading.** New `ServiceTemplate` fields (`memory_bytes`, `nano_cpus`,
  `healthcheck`) propagate to `CompiledOffering` (`catalog/entry.rs`) and through
  every `ContainerSpec` construction site (`infra/plant.rs:592`,
  `tasks/job_executors.rs:708`, `domain/services_internal.rs:115`,
  `api/v1/config.rs:424`, `domain/offering_resolution.rs:184`). Image-direct and
  config-patch paths default to absent.
- **Parsing is defensive.** Size/duration/cpu parsing rejects malformed values
  with a clear error rather than panicking (rule 17 — no `.unwrap()` on manifest
  content). A malformed limit fails the offering's template parse, surfaced by
  `garden-rake manifest validate`.
- **Serde back-compat.** All new fields are `#[serde(default, skip_serializing_if)]`
  so existing `CompiledOffering` JSON and on-disk task registries deserialize
  unchanged; `TaskAction` defaults to `Exec`.
- **Tests.** Unit tests in `offering.rs` (size/duration/cpu parsing, snippet →
  template threading) and `task.rs` (action default + deserialize); a
  `task_scheduler` test that a `Recycle` task dispatches to restart.

## Consequences

- Memory- and CPU-bound offerings can be capped at the manifest level; safe
  co-tenancy on small stones becomes expressible. FlareSolverr is plant-able with
  a real ceiling.
- Author-written `healthcheck:` blocks take effect; `docker ps` reports health and
  the value stops being a silent no-op.
- A leaking container can self-recycle on a cron schedule without an in-container
  command, via a typed `action: recycle`.
- The snippet stays Docker-Compose-shaped — `limits`, `healthcheck`, and `tasks`
  are all standard or near-standard Compose constructs, so the manifest remains
  readable by Compose-literate authors.
- The `ContainerSpec` bridge grows three fields touched at ~6 construction sites;
  the GPU `device_requests` precedent keeps the change mechanical.
