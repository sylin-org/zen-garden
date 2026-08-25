---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-22
canonical: true
---

# ARCH-0012: Typed StoneApi Client Layer

**Date**: 2026-03-22
**Status**: Accepted
**Depends on**: ARCH-0007 (edition 2024, MSRV 1.92)

## Context

Rake commands that call Moss endpoints shared a repetitive pattern: manually
construct a URL string, call `client.get(url).send().await`, deserialize to
`ApiResponse<T>`, unwrap `.data`, and propagate errors. This was duplicated
across every command file with no type-checking on the URL construction and no
single place to update when endpoint paths changed (as happened during ARCH-0006
when ~30 paths were renamed).

Additionally `GardenApiResponse<T>` existed as a duplicate of `ApiResponse<T>`,
creating divergence between components that used each name.

## Decision

Introduce `StoneApi` — a typed HTTP client in `garden-common` (`src/common/src/client/stone_api.rs`) that organises all Moss stone endpoints into seven endpoint-family methods:

- `services()` — service listing, create, get, restart, rest, wake, upgrade, logs, env
- `offerings()` — list, search, plant, remove, inspect, refresh, heal
- `storage()` — overview, health, candidates, banks, add, release, pin, unpin, rename, roles, changes, stream
- `pond()` — init, status, join, invite, unlock, drain, untrust, promote, rename, ca_pem
- `companions()` — list, get, command, up, down, refresh
- `capabilities()` — get hardware capabilities
- `updates()` — pending, execute, stream

Each family method returns domain types from `garden_common` directly — the client handles URL construction, request serialization, response deserialization, and `ApiResponse<T>` unwrapping internally.

`GardenApiResponse<T>` is consolidated as a type alias for `ApiResponse<T>`,
removing the divergence.

## Rationale

- **One place to change paths**: endpoint renames (cf. ARCH-0006) require
  changing one file instead of N command files.
- **Domain types at call sites**: `ctx.stone_api().services().list().await?`
  returns `Vec<ServiceInfo>`, not `ApiResponse<Vec<ServiceInfo>>`.
- **Type-safe URL construction**: `urlencoding` crate for path segments; no
  manual `format!("/api/v1/stone/services/{service}/...")` strings scattered
  across command implementations.
- **Consistent error surface**: one `StoneApiError` enum covers network,
  deserialization, and API-level errors uniformly.

## Consequences

### Positive
- Rake command files reduced from URL-building + deserialization boilerplate to
  single-line typed calls.
- Endpoint path changes require editing `stone_api.rs` only.
- Edition 2024 match-ergonomics inference errors fixed as a side-effect of the
  cleanup pass (lantern, rake, moss).
- `reqwest` 0.13 TLS feature flag corrected (`rustls-tls` → `rustls`).
- `windows-sys` missing features added (`Win32_Foundation`, `Win32_System_IO`).

### Negative
- `StoneApi` is currently a proof-of-concept covering 3 Rake command files;
  remaining command files still use the old pattern and must be migrated
  incrementally.

### Neutral
- `GardenApiResponse<T>` is now a type alias — existing code compiles unchanged.

## References

- [ARCH-0006](ARCH-0006-unified-interface-language.md) — endpoint renames that motivated centralized path management
- [ARCH-0007](ARCH-0007-monomorphic-domain-traits.md) — edition 2024 upgrade that this ADR depends on
- Implementation: `src/common/src/client/stone_api.rs` (~680 lines, 7 endpoint families)
