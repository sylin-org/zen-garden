---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0007, ARCH-0024]
completed: 2026-04-12
---

# ARCH-0030: ContainerRuntime Port

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XII of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: ContainerRuntime (port + adapter; no aggregate state)

## Context

ARCH-0017 Book XII specifies: "Extract the Bollard/Docker dependency
behind a `ContainerRuntime` port with an anti-corruption layer. Foreign
Bollard types never bleed into domain code."

### Discovery findings (6 findings)

1. **`docker::Client` is already a good abstraction layer.** The
   `src/moss/src/docker/` module (~82 kB across 7 files) wraps all
   Bollard calls behind domain-friendly methods. Only one method
   (`container_events()`) returns a Bollard type (`EventMessage`) to
   callers. Every other method returns domain types
   (`ServiceStatus`, `ServiceHealthStatus`, `ContainerResources`,
   `ContainerSpec`, `LogLine`, `Vec<String>`, etc.).

2. **A `ServiceRuntime` trait already exists but is dead code.** The
   trait lives at `domain/traits/service_runtime.rs` (10 methods) and
   `infra/container.rs` has a `ContainerRuntime` struct that implements
   it. Zero call sites outside their own modules reference either type.

3. **23 call sites use `state.platform.docker.*` directly.** All of
   them go through `Arc<docker::Client>` stored at
   `AppState::platform.docker`. None use the existing
   `ServiceRuntime` trait or `ContainerRuntime` struct.

4. **`docker::Client` is also injected as `Arc<Client>` into 8 infra
   and domain types.** These include `DockerHealthProbe`, adoption
   functions, `NurturingStore`, `HarvestOps`, `ContainerInspector`,
   `DockerMonitor`, and `ImageInspector`.

5. **The `ServiceRuntime` trait is incomplete.** It covers 10 of ~30
   methods. Missing operations include: install, upgrade, recreate,
   rename, signal, pull image, commit, exec, logs stream, container
   events, inspect spec, needs_cycle, container ports, volumes,
   topology mount check, port occupancy scan, prune images, bridge
   gateway, DNS reconciliation, image inspection, Docker version.

6. **Bollard type leakage is minimal.** Only `container_events()`
   returns `Stream<Item = Result<EventMessage, bollard::errors::Error>>`.
   The `docker_events` task consumes this and translates to domain
   types. One other leak: `inspect_image_metadata()` returns
   `bollard::models::ImageInspect`, consumed only by
   `infra/image_inspect.rs`.

### Plan change

The ARCH-0017 plan anticipated a full `ContainerRuntime` trait replacing
all Docker access with anti-corruption types. Discovery reveals that
`docker::Client` already IS the anti-corruption layer — it accepts
domain types and returns domain types, with Bollard confined to method
bodies.

Creating a second trait that mirrors all ~30 methods would produce a
1:1 forwarding layer with no practical benefit — it would not enable
podman/containerd alternatives (those have fundamentally different APIs
for operations like GPU passthrough, DNS injection, port remediation)
and would not improve testability (the operations are inherently
integration-level).

**Revised scope:**

1. **Delete dead code.** Remove the unused `ContainerRuntime` struct
   and consolidate `ServiceRuntime` trait.
2. **Seal the one Bollard leak.** Replace `container_events()` return
   type with a domain event type so zero Bollard types cross the
   `docker::` module boundary.
3. **Rename `docker::Client` to `docker::ContainerRuntime`.** The name
   communicates role, not implementation. Import paths become
   `crate::docker::ContainerRuntime`.
4. **Rename `Platform.docker` to `Platform.container`.** Call sites
   become `state.platform.container.*` — implementation-agnostic.
5. **Clean the `ServiceRuntime` trait** to cover what domain code
   actually needs through a trait boundary, and wire the Health
   aggregate's `DockerHealthProbe` to use it instead of raw `Client`.

This is a **scope reduction**, not a scope expansion. The original
plan estimated ~1800 lines; the revised scope is ~600 lines. The
reduction is justified because `docker::Client` already provides
the anti-corruption the plan wanted — it just had the wrong name.

## Decision

Book XII performs a **rename + seal + delete** pass on the container
runtime boundary:

1. Delete the unused `infra/container.rs` `ContainerRuntime` struct.
2. Define a domain-level `ContainerEvent` enum to replace the Bollard
   `EventMessage` in `container_events()` return type.
3. Rename `docker::Client` → `docker::ContainerRuntime`.
4. Rename `Platform.docker` → `Platform.container` across all call
   sites.
5. Wire `DockerHealthProbe` through the `ServiceRuntime` trait for
   `get_service_status`/`get_service_health` instead of raw client.
6. Delete `domain/traits/service_runtime.rs` — the existing trait is
   dead and incomplete. If a future book needs a trait boundary, it
   can define one scoped to its actual needs.

## Consequences

- Zero Bollard types leak outside `src/moss/src/docker/`.
- `state.platform.container` is implementation-agnostic at the name level.
- Dead code (`infra/container.rs`, unused `ServiceRuntime` trait) is
  removed.
- ~30 call sites get a `docker → container` rename — mechanical but
  necessary for naming consistency.
- Future container runtime alternatives would replace
  `docker::ContainerRuntime` internals without changing any call site.
