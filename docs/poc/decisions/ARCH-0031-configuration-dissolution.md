---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0031: Configuration Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XIII of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Configuration (dissolved)

## Context

ARCH-0017 Book XIII specifies: "Extract typed configuration into a
`Configuration` context with event-driven hot-reload for the parts that
support it. Deliverables: `domain/configuration/` module, `Configuration`
aggregate holding typed env + runtime settings, `ConfigChanged` event for
hot-reload scenarios, `ConfigSource` port."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (7 findings)

1. **Configuration is loaded once at boot and never mutated.** `MossConfig`
   is loaded from `moss.toml` by `MossConfig::load()`, merged with CLI
   arguments and environment variables by `DaemonConfig::from_cli()`, and
   the resulting `DaemonConfig` is consumed during `build_state()`. After
   bootstrap, configuration values are frozen.

2. **No hot-reload paths exist.** No file watcher, no runtime config
   mutation API, no `SIGHUP` handler, no admin endpoint for config
   changes. The plan anticipated `ConfigChanged` events, but nothing in
   the codebase would produce or consume them.

3. **`MossConfig` is a pure value object.** It has no mutable state, no
   invariants to enforce, no events to emit. It is deserialized from
   TOML, provides accessor methods with defaults, and is passed to
   constructors. The pattern spec explicitly says: "Do not apply when a
   type is a pure value object (no mutation, no events, no ports)."

4. **`DaemonConfig` already serves as the merged config facade.**
   `bootstrap/config.rs` implements the full priority chain
   (CLI > Env > File > Defaults) and produces a `DaemonConfig` struct
   that is consumed exactly once during state construction. This is the
   correct architecture for static configuration.

5. **`EnvConfig` in `garden-common` is cross-crate.** It serves rake,
   common utilities, and orchestrators — not just moss. Moving it into a
   moss domain aggregate would break the dependency direction. It is
   correctly positioned as a shared utility.

6. **Scattered `std::env::var` calls (~12 in moss) are all at I/O
   boundaries.** Platform-specific infra calls (`PROGRAMDATA`,
   `USERPROFILE`, `USERNAME`), stone name resolution, lantern endpoint
   resolution, service detection. These are not configuration in the DDD
   sense — they are boundary-appropriate environment probes.

7. **Six `MossConfig` timeout accessor methods are dead code.** The
   methods `health_check_interval_secs()`,
   `docker_reconnect_interval_secs()`,
   `http_capabilities_timeout_secs()`, `http_health_timeout_secs()`,
   `http_quick_health_timeout_millis()`, and
   `http_long_operation_timeout_secs()` are defined but never called
   outside of `config.rs`. Their corresponding fields are also unused.
   These are vestigial from a planned but never-implemented configurable
   timeout system.

## Decision

**Dissolve Book XIII.** Configuration does not warrant a bounded context
or aggregate. The existing two-layer architecture (`MossConfig` for file
deserialization + `DaemonConfig` for merged runtime config) is correct
and well-located.

### Actions taken

1. **Delete dead accessor methods and fields** — remove the 6 unused
   timeout accessors and their corresponding `Option<u64>` fields from
   `MossConfig`. This reduces the config surface to what is actually
   consumed.

2. **No `domain/configuration/` module created** — there is no domain
   state to own, no invariants to enforce, no events to emit.

3. **No `ConfigSource` port** — file loading is a one-shot bootstrap
   operation, not a runtime dependency that needs test substitution.

4. **No `ConfigChanged` event** — nothing would produce or consume it.

5. **Context map updated** — Configuration marked as dissolved with
   rationale.

## Consequences

- `MossConfig` remains in `infra/config.rs` as a TOML deserialization
  struct. This is the correct layer — it is infrastructure (file I/O),
  not domain logic.
- `DaemonConfig` remains in `bootstrap/config.rs` as the merged config
  facade consumed during state construction.
- `EnvConfig` remains in `garden-common` as a cross-crate utility.
- If hot-reload is ever needed (future requirement), a new ADR should
  evaluate the scope at that time rather than building speculative
  infrastructure now.
- 6 dead fields + 6 dead methods removed from `MossConfig`, reducing
  maintenance surface.
