---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0004, STORAGE-0009, STORAGE-0011, STORAGE-0017]
---

# ARCH-0025: Bank Aggregate — Storage Domain Model

**Date**: 2026-04-12
**Status**: Accepted
**Book**: VIII-a (Storage Domain Model)
**Epic**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)

## Context

The storage domain has grown organically across three STORAGE ADRs
(0009, 0011, 0017). Volume is a clean domain entity with a proper state
machine, but the **bank** concept — the user-facing named storage
container that groups volumes across stones — exists only as scattered
fields (`replica_set_id`, `replica_set_name`) on `Volume::Management`.

Every call site that needs bank-level operations reconstructs the concept
from volume fields:

- `StorageRoute::find_local()` scans all volumes matching on
  `mgmt.display_name()` to find "which volume backs this bank name"
- `LocalStorage` is a projection of bank-level fields extracted from
  a volume
- API handlers like `rename_storage_v1` iterate volumes by replica set
  name, mutate each, then persist — duplicating the iteration pattern
- `StorageBank` in `bank.rs` is misnamed: it is a volume event router
  (routes OS monitor events into Volume state machines), not a bank

The result: bank-level invariants (rename, pin, set-roles,
set-visibility) are enforced ad-hoc in API handlers rather than in the
domain. Bank identity lives in volume fields rather than in a
first-class entity.

## Decision

### Bank as aggregate root

**Bank** is promoted to a first-class domain entity. It is the
user-facing named storage container (FQN). A bank groups volumes across
stones — a bank named "personal" might have a Primary volume on
stone-A and a Dormant replica on stone-B.

```
Bank
├── id: String              // GUIDv7 (the replica_set_id)
├── name: String            // FQN the user sees ("personal", "media")
├── roles: Vec<String>      // composable: "seed-bank", "storage"
├── visibility: Visibility  // Open | Closed | ReadOnly
└── volumes: local identity derived from volume scan
```

Bank is a **view aggregate** in VIII-a: it is derived from the volume
collection at query time rather than maintained as a separate
persistent entity. This is pragmatic — volumes already persist the
`replica_set_id` / `replica_set_name` in their on-disk manifests, and
introducing a separate bank persistence layer would double-write.

### Volume is infrastructure

Volume remains the physical device entity with its state machine
(Online / Degraded / Offline). The relationship:

- A bank has one or more volumes (across stones)
- A volume belongs to exactly one bank (via `replica_set_id`)
- Local vs remote is routing, not identity: derived from
  `volume.stone_id == my_stone_id`

### Rename StorageBank to VolumeIngestor

The current `StorageBank` struct routes OS monitor events into Volume
state machines. Its name conflicts with the new Bank aggregate.
Rename to `VolumeIngestor` to describe what it actually does.

### BankChanged domain events

The Bank aggregate emits `BankChanged` events that wrap the existing
`StorageChanged` variants. This provides a bank-scoped event stream
for downstream consumers (SSE, replication, S3 listeners).

### What VIII-a does NOT do

- Does NOT change API endpoint paths (VIII-b)
- Does NOT unify S3/WebDAV/REST protocol handlers (VIII-b)
- Does NOT restructure ContentStore/ObjectStore (VIII-b)
- Does NOT introduce bank-level persistence separate from volume
  manifests

## Deliverables

1. `VolumeIngestor` — rename from `StorageBank`, all references updated
2. `Bank` view aggregate with typed queries:
   - `local_banks()` — banks with a volume on this stone
   - `by_name(name)` — single bank lookup
   - `primary_volume(bank_name)` — which volume accepts writes
3. `BankChanged` event enum wrapping interesting `StorageChanged`
   variants
4. Bank-level commands moved from API handlers into the domain:
   - `rename`, `set_roles`, `set_visibility`, `pin`, `unpin`
5. Tests for bank queries and commands

## Exit criteria

- `rg 'StorageBank' src/moss/src/` returns 0 matches (renamed)
- `rg 'replica_set_name' src/moss/src/api/` returns 0 matches
  (API handlers use Bank, not raw volume fields)
- Bank queries tested
- 724+ tests pass
- `cargo clippy --package garden-moss --lib -- -D warnings` clean

## Consequences

- Bank becomes the domain concept that API handlers and routing operate
  on. Volume fields are accessed through Bank, not directly.
- `VolumeIngestor` clearly names the OS event routing responsibility
  that was previously hidden behind the `StorageBank` name.
- VIII-b can build on Bank as aggregate root to unify the write path
  through `Bank::write()`.
