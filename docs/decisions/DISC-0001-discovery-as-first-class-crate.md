---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-05-04
canonical: true
---

# DISC-0001: Discovery as a First-Class Crate

**Status**: Accepted
**Date**: 2026-05-04
**Deciders**: Architecture
**Tags**: discovery, soc, ddd, crate-structure, mdns, udp

---

## Context

Stone discovery — finding Moss instances on the LAN via mDNS and UDP
broadcast, caching their endpoints, providing a typed resolution surface —
is currently fragmented across five files in three locations:

| File | LOC | Role |
|---|---|---|
| [src/rake/src/discovery.rs](../../src/rake/src/discovery.rs) | 693 | All active discovery logic (mDNS browse, UDP send/receive, parallel auto, certmesh CA, Lantern discovery) |
| [src/common/src/discovery.rs](../../src/common/src/discovery.rs) | 88 | Service-discovery response wire types (`FoundService`, `ResolvedConnection`, `StoneRef`) |
| [src/common/src/types/discovery.rs](../../src/common/src/types/discovery.rs) | 250 | Protocol wire types (`DiscoveryRequest`, `DiscoveryResponse`, `UdpAnnouncement`, `TopologyServiceEntry`, `GatewayRegistration`, `StoneGoodbyePayload`) |
| [src/common/src/client/discovery.rs](../../src/common/src/client/discovery.rs) | 141 | TTL cache singleton (RAKE-0010): `Discovery`, `KnownStone`, `STONE: LazyLock<Discovery>` |
| [src/common/src/traits/discovery.rs](../../src/common/src/traits/discovery.rs) | 74 | `DiscoveryProvider` trait + `DiscoveryError` — **defined but never implemented** anywhere in the codebase (verified via grep for `impl DiscoveryProvider` — zero matches) |

The status today:

- **Rake owns the implementation**, but it is otherwise a CLI tool — not the
  natural home for a primitive that other clients also need.
- **Lantern can only listen passively.** [src/lantern/src/tasks/discovery.rs](../../src/lantern/src/tasks/discovery.rs)
  uses Koi's mDNS handle to receive announcements but has no client-side
  discovery — it cannot actively probe for stones or coordinate cross-subnet
  resolution.
- **Pavilion ([PAVILION-0001](PAVILION-0001-windows-client-separation.md)) needs discovery from M0** but should not depend on `garden-rake`. Importing Rake
  means inheriting its CLI surface, presentation logic, and command tree.
- **The dead `DiscoveryProvider` trait** is a fossil of an earlier extraction
  attempt. Its existence confirms the structural problem has been recognised
  before but not resolved.
- **Wire types are already shared** in `garden-common`; the UDP transport
  (`garden_common::infra::communications::p2p`) is already shared
  infrastructure. Only the *active discovery logic* and the *cache* are
  captive in Rake/common-client.

Three consumers want the same primitive — Rake (today), Pavilion (M0),
Lantern (active subnet bridging). The clean answer is a dedicated crate.

---

## Decision

Discovery becomes a first-class workspace member: **`garden-discovery`** at
`src/discovery/`. It owns the active discovery logic (mDNS, UDP, parallel
auto, certmesh, Lantern resolution) and the resolution cache. Wire types
remain in `garden-common`; only logic moves.

We will:

1. Add `src/discovery/` as a workspace member with `garden-discovery` as
   the package name.
2. Move active discovery logic (~693 LOC) from `src/rake/src/discovery.rs`
   into `src/discovery/src/` split by transport.
3. Move the TTL cache from `src/common/src/client/discovery.rs` into
   `src/discovery/src/cache.rs`. Remove from `garden-common`.
4. Reify the `DiscoveryProvider` trait (`src/common/src/traits/discovery.rs`)
   as the public API of `garden-discovery`, with at least one functional
   implementation. Remove the orphan from `garden-common`.
5. Keep all wire types (`DiscoveryRequest`, `DiscoveryResponse`,
   `UdpAnnouncement`, `FoundService`, etc.) in `garden-common`.
   `garden-discovery` depends on `garden-common`. **No cycle.**
6. Update Rake to import from `garden-discovery`. Rake's command tree
   under `src/rake/src/commands/discovery/` stays in Rake (CLI presentation,
   not discovery primitives).
7. Open the door for Lantern to consume `garden-discovery` for active
   subnet-bridging discovery (separate work; this ADR enables it, doesn't
   schedule it).
8. Pavilion imports `garden-discovery` from M0; never depends on
   `garden-rake`.

---

## Crate Structure

```
src/discovery/
├── Cargo.toml                    # depends on garden-common, mdns-sd, if-addrs, hostname
├── src/
│   ├── lib.rs                   # public surface; re-exports
│   ├── cache.rs                 # Discovery (TTL cache), KnownStone, STONE singleton
│   │                            #   from common/src/client/discovery.rs
│   ├── provider.rs              # DiscoveryProvider trait — now implemented
│   │                            #   from common/src/traits/discovery.rs (functional)
│   ├── error.rs                 # DiscoveryError
│   ├── mdns.rs                  # mDNS browse via mdns-sd
│   │                            #   from rake/src/discovery.rs:234–409
│   ├── udp.rs                   # UDP broadcast/listen via garden_common::p2p
│   │                            #   from rake/src/discovery.rs:79–203
│   ├── auto.rs                  # Combined parallel mDNS + UDP discovery
│   │                            #   from rake/src/discovery.rs:534–693
│   ├── lantern.rs               # Lantern endpoint discovery
│   │                            #   from rake/src/discovery.rs:13–69
│   └── certmesh.rs              # Certmesh CA / cornerstone discovery
│                                #   from rake/src/discovery.rs:434–525
└── tests/
    └── ...                      # ported from rake/src/discovery_tests.rs
```

### What does NOT move

- **Wire types** in `garden-common`: stay where they are. `garden-discovery`
  imports them. Moss continues to import them from `garden-common` for
  announcement marshalling.
- **UDP transport** (`garden_common::infra::communications::p2p`): stays
  in `garden-common`. Discovery is a *consumer* of the transport, not its
  owner.
- **Server-side announcement** in `src/moss/src/domain/discovery/`: this
  is a different concern (publishing, not finding). Stays in Moss.
- **Rake's CLI commands** under `src/rake/src/commands/discovery/`: these
  are presentation, not primitives. Stay in Rake.
- **Rake's `stone_cache.rs` and `stone_bag.rs`**: these are Rake-specific
  aggregation/UX state, not discovery primitives. Stay in Rake. (May be
  revisited in a follow-up if Pavilion needs equivalent aggregation.)
- **Lantern's passive listener** in `src/lantern/src/tasks/discovery.rs`:
  passive Koi mDNS reception is a Lantern infrastructure concern, not a
  client-side discovery concern. Stays. May later add an *additional*
  active path that uses `garden-discovery`.

### Dependencies

`garden-discovery` Cargo.toml:

```toml
[package]
name = "garden-discovery"
version = "0.2.0"
edition.workspace = true

[dependencies]
garden-common = { path = "../common" }

# Wire / async
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
tokio = { workspace = true, features = ["sync", "time", "macros", "rt"] }
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
uuid.workspace = true
chrono = { workspace = true, features = ["serde"] }

# Transport
mdns-sd.workspace = true     # moves from garden-rake
if-addrs.workspace = true    # moves from garden-rake
hostname.workspace = true    # moves from garden-rake
```

Rake's Cargo.toml drops `mdns-sd`, `if-addrs`, `hostname` (handled by
`garden-discovery`) and gains `garden-discovery = { path = "../discovery" }`.

Pavilion adds `garden-discovery = { path = "../discovery" }` from day zero.

---

## Public API

The crate exports a small, focused surface:

```rust
// src/discovery/src/lib.rs

pub mod cache;
pub mod error;

mod auto;
mod certmesh;
mod lantern;
mod mdns;
mod udp;
mod provider;

pub use cache::{Discovery, KnownStone, STONE};
pub use error::DiscoveryError;
pub use provider::{DiscoveryProvider, DiscoveryResult};

// Top-level convenience functions, mirror current Rake names so the
// migration is a near-mechanical search-and-replace:
pub use auto::{discover_moss_auto, discover_moss_auto_stream};
pub use udp::{discover_moss, discover_all_moss_stream, discover_all_moss_stream_async};
pub use mdns::{discover_moss_mdns, discover_moss_mdns_stream};
pub use lantern::{discover_lantern_background, get_cached_lantern};
pub use certmesh::{discover_certmesh_ca, CornerstoneInfo};
```

Wire types are imported from `garden-common` by all consumers, not
re-exported from `garden-discovery`. There is one canonical home for each
type — no shadow re-exports.

---

## Migration Strategy

The lift is staged so each commit compiles and tests pass.

1. **Create empty crate.** Add `src/discovery/` with stub `lib.rs`, register
   in workspace `Cargo.toml`. Single commit.
2. **Move types and cache.** `git mv` the cache from `common/client/discovery.rs`
   to `discovery/src/cache.rs`. `git mv` the trait from `common/traits/discovery.rs`
   to `discovery/src/provider.rs`. Update `garden-common` re-exports to
   point to `garden-discovery`. Pure rename commit, content unchanged
   (per [code-standards §14](../../docs/code-standards.md)).
3. **Move active discovery from Rake.** Split [src/rake/src/discovery.rs](../../src/rake/src/discovery.rs)
   into `discovery/src/{mdns,udp,auto,lantern,certmesh}.rs` via `git mv`
   and minimal slicing. Rake's `discovery.rs` becomes a thin re-export
   of `garden-discovery::*` for one transition release, then deleted.
4. **Implement `DiscoveryProvider`.** Provide at least one functional
   implementation (`DefaultProvider` over the `auto` discovery primitives).
   Remove the trait's "defined but unused" status.
5. **Cargo dependency cleanup.** Remove `mdns-sd`, `if-addrs`, `hostname`
   from `garden-rake/Cargo.toml`; add `garden-discovery` path dep.
6. **Pavilion ADR amendment unnecessary.** PAVILION-0001 already lists
   `StoneApi` from common as the import path. Pavilion adopts
   `garden-discovery` directly when M0 work begins.

Each step is a separate commit. Tests run between each.

---

## Rationale

- **Three consumers, one primitive.** Rake, Pavilion, and Lantern each
  need active discovery. A shared crate is the only structurally honest
  home.
- **The cache is logic, not contract.** `Discovery` and `KnownStone` in
  `garden-common::client::discovery` are an oddity — they carry behaviour
  (TTL eviction, locking, singleton state) but live in the contract crate.
  Moving them out reduces `garden-common`'s scope.
- **The trait was the right idea.** `DiscoveryProvider` already exists,
  pointing at the same extraction, and was abandoned mid-flight. Completing
  it resolves a known incomplete refactor.
- **Wire types stay where contracts live.** `garden-common` is the contract
  crate by convention; protocol types belong there. Splitting them would
  create either a dependency cycle or a third "discovery-types" crate —
  unwarranted at this scale.
- **No cycle.** `garden-discovery` depends on `garden-common`; `garden-common`
  does not depend on `garden-discovery`. Moss imports types from
  `garden-common`; clients import logic from `garden-discovery`.
- **Lantern gains active discovery as a side effect.** This is not the
  goal of the ADR but is enabled by it — Lantern's subnet-bridging /
  hot-cache role per the user's design now has a primitive to build on.

---

## Consequences

### Positive

- Pavilion has a clean import path from M0 with no Rake dependency.
- Rake's `discovery.rs` (693 LOC) leaves Rake; Rake becomes more clearly
  a CLI tool rather than a CLI-plus-discovery hybrid.
- The `DiscoveryProvider` trait fossil is resolved.
- `garden-common` shrinks slightly and clarifies its role as a
  contract/types crate.
- Lantern can adopt active discovery when ready (separate work).
- mDNS / UDP / Lantern discovery are split by transport into named files,
  improving navigability over the 693-line monolith.

### Negative

- One additional workspace crate to maintain.
- One transitional release where Rake's `discovery.rs` is a thin
  re-export shim; cannot ship the crate split and the Rake CLI cleanup
  in one commit without a flag day.
- Lantern adopting active discovery is *enabled* but requires its own
  follow-up work to actually do — the ADR doesn't deliver that capability,
  just unblocks it.

### Neutral

- Wire types (`DiscoveryRequest`, etc.) remain in `garden-common`. Anyone
  reading the codebase needs to know that contracts live in common,
  logic in discovery. The split is principled but is a thing to learn.
- `stone_cache.rs` and `stone_bag.rs` in Rake remain. They are Rake-specific
  multi-stone UX state. Pavilion may want analogous aggregation later;
  whether to share or replicate that logic is deferred.

---

## Alternatives Considered

### Alternative 1: Pavilion imports from `garden-rake`

- **Description**: Make Pavilion depend on `garden-rake`'s library exports
  (Rake already has a `lib.rs`).
- **Pros**: Zero new crate; immediate.
- **Cons**: Pavilion inherits Rake's CLI surface, command tree, presentation
  helpers, terminal-rendering deps. Couples a GUI client to a CLI tool's
  release cadence and dependency graph. Architecturally backwards.
- **Rejected because**: Rake is a CLI consumer of discovery; making it a
  provider for another consumer inverts the dependency direction.

### Alternative 2: Move discovery logic into `garden-common`

- **Description**: Keep everything in `garden-common`; widen the
  `client/discovery.rs` to include mDNS and UDP logic.
- **Pros**: No new crate.
- **Cons**: Inflates `garden-common` with `mdns-sd`, `if-addrs`, `hostname`
  deps that don't belong in a contract crate. Muddies SoC: contract types
  and active behaviour live together.
- **Rejected because**: `garden-common` is the contract crate. Adding
  active discovery (transport, side effects, singletons) violates that
  role. The user's framing — "near commons/core, but its own concern" —
  is exactly this.

### Alternative 3: Two crates — `garden-discovery-types` + `garden-discovery`

- **Description**: Split contract types into a third tiny crate.
- **Pros**: Theoretically purest; types crate has no deps.
- **Cons**: Wire types are already in `garden-common` and used by Moss
  directly. Splitting would force Moss to depend on a new types crate or
  re-shimmed re-exports. Over-engineering for the scale.
- **Rejected because**: `garden-common` already serves the
  contract-types role. Adding a fourth layer is friction without payoff.

---

## References

- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Primary new consumer; M0 import path
- [RAKE-0010](RAKE-0010-caching.md) — Origin of the TTL cache being moved
- [LANTERN-0001](LANTERN-0001-registry.md) — Lantern's role; active discovery adoption is downstream of this ADR
- [LANTERN-0003](LANTERN-0003-mdns-service-discovery.md) — mDNS service discovery, single-service-type
- [MDNS-0001](MDNS-0001-single-service-type.md) — mDNS service-type convention this crate respects
- [COMM-0001](COMM-0001-p2p-transport-singleton.md) — UDP transport singleton; `garden-discovery` is a consumer
- [COMM-0004](COMM-0004-multicast-first-discovery.md) — Multicast-first behaviour preserved by the lift
- [docs/code-standards.md §14](../code-standards.md) — File renaming convention (rename commit + content commit separated)
