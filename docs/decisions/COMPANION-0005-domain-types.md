---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-13
canonical: true
completed: 2026-04-13
---

# COMPANION-0005: Domain Types — Book IV of COMPANION-0001

**Date**: 2026-04-13
**Status**: Completed (2026-04-13)
**Book**: IV of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0002](COMPANION-0002-event-envelope.md), [COMPANION-0004](COMPANION-0004-transport.md), [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)

## Context

Book IV adds the shared **domain vocabulary** that moss, the companion SDK, and (later) cricket + firefly all speak in. The Garden aggregate in Book V will be built on these types; the Adapter contract in Book VI exposes them to consumers.

Per the Discovery Mandate in COMPANION-0001, Ch0 re-evaluated the plan against the live code. Findings:

### What the re-evaluation found

1. **`Stone` already exists** at `src/common/src/stone.rs` as the canonical node struct — moss uses it for both local (`Moss.current.stone`) and peer stones. No new type needed; promote through re-export only. Its `health: String` field is the untyped hole we'll cap via conversions without breaking existing code.

2. **`Offering` already exists** at `src/common/src/offerings.rs`. Same treatment: re-export, no new type.

3. **Vitality strings are already centralized** at `src/common/src/constants/mod.rs:235-246` — five `VITALITY_*` consts (`thriving`, `needs_attention`, `withering`, `wilting`, `dormant`). Book IV promotes these to a proper `Health` enum, with serde representation preserving the existing wire strings so no consumer breaks.

4. **No typed `Load` struct exists.** Today, `StoneLoadUpdatedPayload` in `garden-common::presence::types` carries the eight raw f64/u64 percent fields ad hoc. Book IV extracts the bag of values into a cohesive `Load` domain type and provides a conversion.

5. **No `SeedBank` or `Pond` domain types.** Wire-level `StoragePresence` has `name/used_gb/total_gb`. `Pond` is implicit in moss via `pond_active: bool` on `StoneState` plus ceremony state in `Security`. Book IV defines compact domain types (`SeedBank`, `Pond`) that fold the relevant fields together.

6. **Moss migration is NOT required for Book IV to close.** The ADR says "moss uses domain types at their boundaries" — a strict reading would be a workspace-wide refactor on par with ARCH-0017 Book IX (Security). That's out of proportion for this epic's scope. Instead, Book IV:
   - Publishes the domain types in `garden-common::domain`
   - Provides `From` / `TryFrom` conversions from wire types
   - Updates SDK core payloads to expose **typed accessor methods** (`StoneHealthChangedPayload::health_domain()`) alongside the existing string field
   - Leaves moss's internal usage alone

   Book V (Garden) will consume the typed accessors. Moss adoption is a natural consequence of wanting the benefits and can happen organically in post-epic work.

7. **Serde compatibility is the critical constraint.** Moss emits `{"health": "thriving"}` on the wire; companions must accept it. `Health` serialises as the corresponding lowercase string via `#[serde(rename_all = "lowercase")]` + explicit rename arms for `NeedsAttention` → `"needs attention"`. Round-trip test required.

No plan change vs COMPANION-0001 beyond narrowing the "migrate moss" interpretation — which is consistent with the break-and-rebuild tenet scoped to the companion segment.

## Decision

Introduce `garden-common::domain` as the curated shared vocabulary. Content:

### Types

```rust
// garden-common/src/domain/health.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Health {
    #[serde(rename = "thriving")]           Thriving,
    #[serde(rename = "needs attention")]    NeedsAttention,
    #[serde(rename = "withering")]          Withering,
    #[serde(rename = "wilting")]            Wilting,
    #[serde(rename = "dormant")]            Dormant,
}

impl Health {
    pub fn as_str(&self) -> &'static str;       // matches VITALITY_* constants
    pub fn is_ok(&self) -> bool;                // Thriving only
    pub fn needs_attention(&self) -> bool;      // anything not Thriving
    pub fn is_terminal(&self) -> bool;          // Wilting
    pub fn parse(s: &str) -> Option<Self>;      // forgiving string parse
}

impl std::fmt::Display for Health { ... }
impl From<Health> for &'static str { ... }

// garden-common/src/domain/load.rs
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Load {
    pub cpu: Percent,
    pub memory: Percent,
    pub disk: Percent,
    pub io: Percent,
    pub gpu: Percent,
    pub gpu_active: bool,
    pub net_rx_bytes_per_sec: u64,
    pub net_tx_bytes_per_sec: u64,
}

/// Clamped-at-construction percentage (0..=100) stored as f64 for precision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Percent(f64);

impl Percent {
    pub fn new(v: f64) -> Self;                 // clamps to [0, 100]
    pub fn value(&self) -> f64;
    pub fn as_u8(&self) -> u8;                  // rounded, for display
}

// garden-common/src/domain/seed_bank.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedBank {
    pub name: String,
    pub used_gb: u64,
    pub total_gb: u64,
}

impl SeedBank {
    pub fn free_gb(&self) -> u64;
    pub fn fill_percent(&self) -> Percent;
}

// garden-common/src/domain/pond.rs
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Pond {
    /// Stone has not joined any pond.
    Solo,
    /// Stone is a member of a pond.
    Member,
    /// Stone is the cornerstone (first stone, holds CA).
    Cornerstone,
}

// garden-common/src/domain/mod.rs
pub mod health;
pub mod load;
pub mod pond;
pub mod seed_bank;

pub use crate::offerings::{Offering, OfferingFqn};     // re-export existing
pub use crate::stone::{Stone, StoneStatus};             // re-export existing

pub use health::Health;
pub use load::{Load, Percent};
pub use pond::Pond;
pub use seed_bank::SeedBank;
```

### Wire-type conversions

```rust
// In garden-common::presence::types (existing module)
impl From<&StoneLoadUpdatedPayload> for Load { ... }
impl From<&StoragePresence> for SeedBank { ... }
```

Both are infallible; existing numeric fields map 1:1.

```rust
// Added to garden-common::domain::health (new)
impl Health {
    /// Parse a wire string (e.g. "thriving"). Returns None for unknown
    /// values instead of panicking; callers may default to Dormant.
    pub fn parse(s: &str) -> Option<Self>;
}
```

Moss's existing string comparisons (`s == VITALITY_THRIVING`) continue to work; the typed enum is available alongside.

### SDK consumer surface

In `src/companion-sdk/src/garden/core_payloads.rs`:

```rust
impl StoneHealthChangedPayload {
    /// Typed health value parsed from the wire string. Returns
    /// `Health::Dormant` if the string is unrecognized.
    pub fn health_domain(&self) -> Health {
        Health::parse(&self.health).unwrap_or(Health::Dormant)
    }
}

impl StoneLoadUpdatedPayload {
    /// Typed load snapshot.
    pub fn load_domain(&self) -> Load { Load::from(self) }
}

// PresenceSnapshot::stone.health stays a String; typed view via:
impl PresenceSnapshot {
    pub fn stone_health(&self) -> Health { ... }
    pub fn stone_load(&self) -> Load { ... }
    pub fn seed_bank(&self) -> Option<SeedBank> { ... }
}
```

Book V (Garden) uses these typed accessors in its projection logic so the aggregate's `health()` / `load()` / `seed_bank()` getters return the typed values directly.

## Implementation plan

**Chapter 1 (this ADR)** — land this document.

**Chapter 2** — implement domain module + SDK accessors + tests:
- `src/common/src/domain/{mod,health,load,pond,seed_bank}.rs`
- Wire-type conversions in `src/common/src/presence/types.rs` (additive impls)
- SDK accessor methods in `src/companion-sdk/src/garden/core_payloads.rs`
- Re-export `garden_common::domain` items through SDK prelude
- Unit tests: Health parse/round-trip via serde, enum helpers (is_ok, is_terminal), Percent clamping, Load conversion, SeedBank free/fill, Pond variants, SDK accessor methods

**Chapter 3** — update COMPANION-0001 revision history, close book.

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_common::domain::{Health, Load, Percent, Pond, SeedBank, Stone, Offering};` compiles.
2. `serde_json::from_str::<Health>(r#""thriving""#)` returns `Ok(Health::Thriving)`; round-trips back to `"thriving"`.
3. `Health::parse("wilting") == Some(Health::Wilting)` and `Health::parse("invalid") == None`.
4. `Percent::new(150.0).value() == 100.0` (clamping).
5. `StoneHealthChangedPayload::health_domain()` returns `Health::Thriving` for a wire payload with `health = "thriving"`.
6. `cargo check --all` green.
7. `cargo test --package garden-common domain:: --package garden-companion-sdk` green.
8. `cargo clippy --package garden-common --package garden-companion-sdk -- -D warnings` green.
9. COMPANION-0001 revision history amended with Book IV closure.

## Out of scope (deferred)

| Item | Deferred to |
|------|-------------|
| Migrating moss's `Stone.health` field from `String` to `Health` | Post-epic targeted ADR — not required for Garden/Adapters/Companion books |
| Migrating moss's wire types to use `Load` internally | Same |
| Defining all domain operations (e.g. `Stone::promote_to_cornerstone()`) | Out of scope — Book IV is *vocabulary*, not behavior |
| Tool / offering domain taxonomy beyond re-export | Post-epic refactor; moss already has rich `OfferingFqn` |
| Custom serde for `Load` wire compat with flat JSON | Not needed — `StoneLoadUpdatedPayload` stays the wire type; `Load` is the typed view accessed via conversion |

## Closure notes (2026-04-13)

Book IV closed with all exit criteria met. Summary of what shipped:

- **`garden-common::domain` module** at `src/common/src/domain/` with four new files (`health.rs`, `load.rs`, `pond.rs`, `seed_bank.rs`) plus a module root that re-exports canonical types (`Stone`, `OfferingFqn`, `StoneStatus`) for a single domain import path.
- **Five typed values**: `Health`, `Load`, `Percent`, `SeedBank`, `Pond`. All `Debug + Clone + Serialize + Deserialize`; small ones are `Copy`.
- **Wire conversions**: `From<&StoneLoadUpdatedPayload> for Load` (clamps out-of-range values), `From<&StoragePresence> for SeedBank`, `Health::parse` / `Pond::from_active_flag` for string/bool boundaries.
- **SDK extension traits** in `core_payloads.rs`: `StoneHealthChangedExt`, `StoneLoadUpdatedExt`, `PresenceSnapshotExt`. All re-exported through the SDK prelude.
- **Zero new workspace deps.**
- **36 new tests** (31 in garden-common, 5 in companion-sdk), all green. 85 SDK garden tests overall.

### Minor refinements during implementation

- **`Offering` not re-exported from `domain`**: discovered during compile that `garden-common::offerings` exports `OfferingFqn` (the identity type) but `Offering` is moss-internal. The ADR mentioned both; implementation kept only `OfferingFqn`. Future books may promote a shared `Offering` view if needed. Noted in the module doc.
- **`StoneStatus` re-exported from `types::discovery`**, not `stone` — caught at compile time. Final placement documented in the module.
- **`Percent` is `serde(transparent)`** — wire-compatible with bare numbers in JSON, but serde does not clamp on deserialize. Callers should normalize at the boundary via `Percent::new`. Documented in the module doc and covered by an explicit test.

### Follow-on work picked up by later books

- Book V (Garden) uses the `PresenceSnapshotExt` typed accessors to populate the Garden aggregate's read-model state — no string handling in the projection code.
- Book VI (Adapters) adapters receive the `Garden` handle and query typed properties (`garden.health() -> Health`, `garden.load() -> Load`).
- Moss migration to typed `Health` / `Load` fields is deferred post-epic. When an adopter needs it, `Health::parse` + `as_str()` make the transition incremental.

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0004](COMPANION-0004-transport.md) — Transport (Book III)
- [companion-architecture.md §Garden context](../specs/companion-architecture.md#garden-context)
- [garden-common::stone::Stone](../../src/common/src/stone.rs) — existing canonical stone type
- [VITALITY_* constants](../../src/common/src/constants/mod.rs) — source of the five-valued `Health` enum
