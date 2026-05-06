---
audience: [developer, ai]
doc_type: decision
status: proposed
date: 2026-05-05
depends_on: [TOOLS-0003, ORCH-0039, ARCH-0026, STORAGE-0013, OFFER-0003]
---

# ARCH-0038: Logical Sets as a First-Class Garden Surface

**Date**: 2026-05-05
**Status**: Proposed
**Tags**: api, sets, replication, hotcache, vocabulary

---

## Context

Two unrelated surfaces in the current codebase both express the same idea —
"these instances form a replication group" — in incompatible shapes:

1. **Offering replication** lives in [`OfferingFqn`](OFFER-0003-offering-fqn.md)
   identity plus [`OrchestrationState`](../../src/common/src/types/orchestration.rs)
   (`role`, `primary_stone_id`) per `Offering`. Same-FQN instances across stones
   form an emergent replica set per [ORCH-0039](ORCH-0039-seed-based-offering-replication.md)
   §"Identity", but there is no wire-format noun for that emergent set. To
   answer "which stones host the `mongodb::prd` set, who's primary, who's
   a replica?" a consumer today must:
   - fan out `/api/v1/garden/services` to enumerate same-FQN entries, OR
   - reach across to the per-orchestrator dashboard at `:7191/api/cluster/...`,
     a mongo-only surface that lives outside the zen-garden API namespace.

2. **Storage replication** lives in [`StorageAnnouncement`](../../src/common/src/storage.rs)
   with `replica_set_id` / `replica_set_name` per [STORAGE-0013](STORAGE-0013-replica-set-identity.md),
   plus [`StorageOrchestrationState`](../../src/common/src/storage.rs) mirroring
   the offering shape (`role`, `primary_stone_id`, `pinned`). Banks are
   surfaced via [ARCH-0026](ARCH-0026-storage-api-surface.md) at
   `/api/v1/{stone,garden}/banks`, which lists every bank but does not
   project the replication relationship as a noun.

Both shapes already replicate into the `GardenRegistry` hotcache (TOOLS-0003)
on every stone via beacons + write-through. The data is local to every
Moss instance; what's missing is a named projection.

The Pavilion canvas (ORCH-0039 §"Set-state visualisation on the canvas")
needs:

- For each offering, "which stones share this FQN" → the edges between
  stone nodes on the 3D sphere.
- For elected offerings, "who's primary on this set" → role badges on
  stone cards.
- For banks, "this bank exists across N stones, primary is X" → bank-card
  enrichment.
- A path that gracefully degrades when an orchestrator dashboard isn't
  reachable.

The implicit projection (fan out `/garden/services`, group by FQN
client-side) works once but burns cycles every time, fragments across
consumers (Pavilion, Rake, future tools each reinvent it), and pushes
membership rules into client code. Naming the projection in Moss is the
honest move.

A separate concern: `OfferingRole::Dormant` and `StorageRole::Dormant`
both carry the word "Dormant" — which suggests *idle / sleeping*. In
reality these instances actively replicate from the primary and (in the
storage case) serve read traffic. The standard distributed-systems
vocabulary is "replica." The set-noun work is the natural moment to
correct this.

---

## Decision

### A new top-level namespace: `/api/v1/sets`

A logical set is *inherently garden-wide* — there is no per-stone set —
so prefixing it with `/garden` is structural noise. `/api/v1/sets` becomes
a peer to `/api/v1/stone`, `/api/v1/garden`, and `/api/v1/pond`.

### Kinds segmented by path

```text
GET /api/v1/sets                       # index of available kinds
GET /api/v1/sets/offerings             # all offering sets
GET /api/v1/sets/offerings/{fqn}       # one offering set
GET /api/v1/sets/banks                 # all bank sets
GET /api/v1/sets/banks/{moniker}       # one bank set
```

Path-segmented kinds (rather than `?kind=` on a polymorphic shape) so each
kind keeps its own clean schema. Bank sets carry `total_capacity_bytes`,
`pin_id`, `visibility`, `protocols`. Offering sets carry `coordination`,
`uri_template`, `connection_string_hint`. Forcing a unified shape would
either be the union of every kind's fields (sparse and unclear) or the
intersection (loses information). Per-kind paths let each evolve
independently.

`GET /api/v1/sets` is a tiny index returning `{ kinds: ["offerings", "banks"] }`
so consumers can introspect without hardcoding.

### Membership rules

| Kind | Emit when |
|---|---|
| `offerings` | At least one running instance exists *and* the offering's compiled `coordination = Elected`. Independent offerings (Ollama, proxies) do not appear here regardless of instance count — they live on `/garden/services` and `/stone/services`. |
| `banks` | The bank exists in the registry (one or more known volumes). Singletons appear; the canvas uses them to drive "you have a bank with no replica — would you like to add one?" affordance. |

The rules are kind-specific and live in the projection. They are not
expressed as registry filters because the registry doesn't know about
`coordination` (that lives in the offering's compiled manifest, accessed
via `state.catalog.get_compiled(name).coordination`).

### Role vocabulary

`Dormant` is renamed `Replica` in both enums. New vocabulary:

| Kind | Role values |
|---|---|
| `offerings` | `Joining \| Primary \| Replica \| Degraded` |
| `banks` | `Primary \| Replica` |

Each kind's role is a string in the wire format (`role: "primary"`),
defined per kind. Consumers render verbatim. There is no shared `Role`
union across kinds.

This is a **break-and-rebuild** rename, not a soft migration:
- `serde` accepts only the new names; deserializing old `"dormant"` from
  on-disk state fails loudly.
- Persistent state files containing `"dormant"` must be either deleted or
  manually rewritten before upgrade. Garden state is small and
  reproducible; this is acceptable.
- All pattern matches on `OfferingRole::Dormant` / `StorageRole::Dormant`
  rename to `Replica` in the same change.

### Hotcache as the source

Sets are *projections* over `GardenRegistry`. No fan-out, no per-stone
HTTP, no orchestrator cross-call. A handler reads the local registry,
groups by `fqid` (offerings) or `replica_set_name` (banks), applies the
membership rule, and returns.

Latency is bounded by `RwLock` acquisition + iteration over a few hundred
tools. Same-host, microsecond scale. No caching layer needed.

### What stays in orchestrator dashboards

Orchestrator-specific enrichment — replica-set lag, oplog freshness, cache
hit ratios, mongo wire-version ranges, future Postgres WAL stats — stays
in each orchestrator's own dashboard. These shapes don't generalise across
backends and don't belong in a uniform set surface. Diagnostic consumers
go direct to `:7191/api/cluster/*` (or the equivalent for other
orchestrators). The `/api/v1/sets/offerings/{fqn}` response will not
include lag / oplog / cache fields.

If a future consumer wants enrichment fronted through Moss, it lands as
an explicit opaque `enrichment: <orchestrator-shaped-json>` blob behind a
query parameter (`?enrich=true`), not as a structured field. That's a
follow-up ADR if and when needed; not in scope here.

### Wire format examples

**`GET /api/v1/sets/offerings/mongodb::prd`**

```json
{
  "kind": "offering",
  "name": "mongodb::prd",
  "coordination": "elected",
  "primary_stone": "stone-crystal-forest",
  "uri_template": "mongodb://{host}:{port}/?replicaSet=zen-garden",
  "connection_uris": [
    "mongodb://stone-crystal-forest.local:27017/?replicaSet=zen-garden",
    "mongodb://stone-mossy-brook.local:27017/?replicaSet=zen-garden"
  ],
  "members": [
    {
      "stone_id": "0193…",
      "stone_name": "stone-crystal-forest",
      "endpoint": "http://stone-crystal-forest.local:7185",
      "role": "primary",
      "status": "running",
      "ready": true
    },
    {
      "stone_id": "0193…",
      "stone_name": "stone-mossy-brook",
      "endpoint": "http://stone-mossy-brook.local:7185",
      "role": "replica",
      "status": "running",
      "ready": true
    }
  ]
}
```

**`GET /api/v1/sets/banks/personal`**

```json
{
  "kind": "bank",
  "name": "personal",
  "replica_set_id": "0193…",
  "primary_stone": "stone-crystal-forest",
  "total_capacity_bytes": 2000000000000,
  "total_used_bytes": 743000000000,
  "visibility": "open",
  "encrypted": false,
  "pin_id": "0193…",
  "protocols": ["s3", "storage"],
  "roles": ["seed-bank"],
  "members": [
    {
      "stone_id": "0193…",
      "stone_name": "stone-crystal-forest",
      "device_id": "0193…",
      "device_name": "WD-SSD-2TB",
      "role": "primary",
      "capacity_bytes": 2000000000000,
      "used_bytes": 743000000000
    }
  ]
}
```

The list endpoints return `{ "sets": [<entry>, ...] }` where each entry
omits the `members[]` detail (just `member_count`, `primary_stone`) for
list-level brevity. Detail endpoints return the full shape.

---

## Rationale

**Why a top-level `/sets` rather than `/garden/sets`.** Sets are inherently
cross-stone. The `/garden` prefix exists to mark *aggregations of per-stone
calls*; sets aren't aggregations, they're a different category of thing.
A peer namespace is honest about what's there.

**Why path-segmented kinds.** Bank sets and offering sets carry genuinely
different fields (capacity vs coordination mode; storage roles vs offering
roles; pin_id vs primary_stone_id). A polymorphic union forces the union
of all fields on every entry, with most nullable; consumers can't tell
which fields are kind-specific from the schema alone. Path segmentation
keeps each kind's schema self-describing and lets kinds evolve without
ripple.

**Why singleton banks emit; singleton offerings do not.** Banks-as-sets
are also UX-driving: a singleton bank is exactly the surface that should
prompt "would you like to add a partner stone?" An Independent offering
running on N stones is N autonomous instances — FQN coincidence is not
replication, and treating it as a degenerate set would be misleading.
Elected offerings with one running instance still emit because they're
*structurally* sets even if degenerate today (the orchestrator's reactive
reconcile loop will absorb a second instance into the set automatically;
a viewer wants to see the singleton state coming).

**Why hotcache as source.** It's already there; it already replicates;
it's already what `/garden/services` and `/garden/banks` project from.
Sets are no different. Anything else duplicates the registry's job.

**Why drop Dormant for Replica.** "Dormant" implies sleeping, idle, not
participating. In MongoDB-style replica sets, secondaries are *actively*
replicating writes from the primary and (configurably) serving read
queries. In zen-garden's storage replication, replicas are also
SSE-pulling changes continuously. "Replica" is the standard
distributed-systems vocabulary that accurately describes what these
instances do.

**Why break-and-rebuild on the rename.** Garden state is small,
reproducible, and not yet at production-deployment scale. A clean wire
format with no two-name aliases is worth the one-time pain. Soft
migrations leave permanent backwards-compatibility code that everyone
has to remember why it's there.

**Why orchestrator-specific data stays out.** Lag / oplog / cache stats
don't share a schema across backends. Forcing them into the set surface
either bloats the schema with mongo-only fields (other orchestrators
return null) or invents a synthetic abstraction (forced unification). The
orchestrator dashboards are the right home for backend-specific
diagnostics.

---

## Consequences

### Positive

- One canonical noun for "logical set" across the garden. Consumers
  (Pavilion canvas, Rake set commands, future tooling) target one
  surface.
- The `/api/v1/sets/offerings` projection unblocks ORCH-0039 §"Set-state
  visualisation" without taking a dependency on orchestrator dashboard
  reachability.
- Bank sets become a first-class concept on the canvas, naturally
  integrating ARCH-0026's bank work with the set view.
- The Replica rename clarifies intent at every call site.
- The orchestrator dashboards stay focused on orchestrator-internal
  diagnostics rather than being mistaken for a public API.

### Negative

- The Dormant → Replica rename is a coordinated multi-file change
  touching `garden_common`, Moss, Rake, Firefly, companion-sdk,
  orchestrators. Break-and-rebuild posture means a coordinated cut.
- Persistent state on disk containing `"dormant"` is invalidated.
  Operators upgrading existing gardens delete the orchestration-state
  files (or accept default-init).
- Pavilion canvas slice depends on `/api/v1/sets/offerings` landing
  before it can wire — the canvas piece slips in scope until the API
  ships.

### Neutral

- The mongo orchestrator's `/cluster/*` keeps existing behaviour. The
  ADR doesn't deprecate it; it just stops being the canonical source
  for set state.
- A future "garden snapshot" or "garden topology export" might read
  `/api/v1/sets/*` as a supplementary data source. Out of scope here.
- Adding a new kind in the future (e.g. `/api/v1/sets/companions` if
  ever relevant) is a new endpoint plus an entry in the index. No
  schema rev required.

---

## Alternatives Considered

### Alternative 1: `/api/v1/garden/sets` (rejected)

**Description**: Place sets under the garden namespace, matching the
`/garden/services` and `/garden/banks` pattern.

**Rejected because**: Sets aren't aggregations of per-stone operations.
The garden prefix exists to disambiguate "this is the cross-stone view";
sets only have a cross-stone view. Prefixing adds a structural noise that
suggests a stone counterpart that doesn't exist.

### Alternative 2: Polymorphic `/api/v1/sets` with `kind` discriminator (rejected)

**Description**: A single endpoint `/api/v1/sets` returns mixed offering
and bank sets, each entry tagged with `kind`. Filtering via `?kind=`.

**Rejected because**: Schemas don't unify cleanly. Bank sets carry
`total_capacity_bytes`; offering sets don't. Offering sets carry
`coordination`; bank sets don't. A unified shape forces the union (most
fields null per row) or the intersection (information loss). Path
segmentation gives each kind a self-describing schema and a stable home
for kind-specific fields.

### Alternative 3: Extend `/api/v1/stone/services` with role + primary_stone_id (rejected)

**Description**: Add `role`, `primary_stone_id`, `coordination` to the
existing `ServiceInfo` wire format. Pavilion's canvas projects sets
client-side from `/garden/services` rows.

**Rejected because**: This is intentional rework — every consumer that
wants a set view re-implements the projection, and those fields would
be deprecated as soon as `/api/v1/sets` ships. Building a thing to
delete it is theatre. Going straight to `/api/v1/sets` saves the
duplicate write.

### Alternative 4: Source from per-orchestrator dashboards (rejected)

**Description**: Pavilion (and other consumers) hit
`http://<orchestrator-stone>:7191/api/cluster/members` to populate set
state.

**Rejected because**: Orchestrator dashboards are orchestrator-private
debugging surfaces, not part of the zen-garden API namespace. They're
mongo-shaped (or ollama-shaped, or future-postgres-shaped); each consumer
would have to handle each orchestrator's distinct schema. Reachability is
fragile (orchestrator container down, stone offline, port not exposed).
The hotcache already has the data Moss-side; using it is the better
substrate.

### Alternative 5: Soft Dormant→Replica migration (rejected)

**Description**: `Deserialize` accepts both `"dormant"` and `"replica"`
for one release window, then drop `"dormant"`.

**Rejected because**: User explicitly opted out — break-and-rebuild is
acceptable. Soft migrations leave permanent backwards-compat scar
tissue; clean cuts are cheaper at this maturity level.

---

## Implementation Plan

> **Note on this plan:** the section below is the current best
> understanding. Expect it to change as the implementation discovers
> better fits with existing code. File targets and ordering are
> indicative; the goal is the shape, not the exact line-of-attack.

### Phase A — Rename `Dormant` → `Replica`

Goal: clean, single-pass rename with no aliases.

**Files (indicative):**
- `src/common/src/types/orchestration.rs` — `OfferingRole::Dormant` →
  `Replica` in the enum, `Display`, `Default`, `is_announced`.
- `src/common/src/storage.rs` — `StorageRole::Dormant` → `Replica`,
  `Display`, `Default`. `ROLE_DORMANT` constant in
  `src/common/src/constants/mod.rs` removed (or renamed to
  `ROLE_REPLICA`).
- All 19 files containing `::Dormant` from the discovery grep, including
  Moss tasks (`offering_orchestration`, `storage_orchestration`,
  `storage_replication`), Moss API handlers (`storage`, `banks`,
  `garden_storage`, `announcement`), Rake commands (`storage`,
  `presence`, `pulse`, `discovery::observe`), Firefly animation,
  companion-sdk core_payloads.

**Unknowns / risks:**
- `StorageMetadata.role: Option<String>` already serializes role as a
  string ("primary" / "dormant"). The string `"dormant"` appears in
  beacons that may still be in flight from peers. **Need to handle**:
  either (a) deserialize `Option<String>` and validate downstream, OR
  (b) the registry layer normalises "dormant" → "replica" on receive
  and emits warnings. Decision deferred to implementation; (b) is
  cleaner if it composes with break-and-rebuild posture (we trust
  freshly-rebuilt peers).
- Persistent state files (e.g. `<data_dir>/orchestration-state.json`)
  containing `"dormant"` may exist on dev boxes. Operators delete them
  on upgrade. Document in the changelog entry.

### Phase B — Set route registration + index endpoint

Goal: the `/api/v1/sets` namespace exists and serves the index.

**Files:**
- `src/moss/src/bootstrap/router.rs` — register `/api/v1/sets` and
  reserve `/offerings`, `/banks` sub-routers.
- New module `src/moss/src/api/v1/sets/mod.rs` — top-level handler for
  `GET /api/v1/sets`. Returns `{ kinds: ["offerings", "banks"] }`.
- `src/moss/src/api/v1/mod.rs` — pub mod sets.

**Unknowns:**
- Whether the existing `ApiResponse<T>` envelope (with suggestions)
  applies. The set list is structural enough that suggestions probably
  don't fit; but consistency may win. Decision: follow whatever
  ARCH-0026 banks endpoints do (likely envelope with empty
  suggestions).

### Phase C — `/api/v1/sets/offerings` projection

Goal: the offering set list and detail endpoints serve from the
`GardenRegistry` hotcache.

**Files:**
- `src/moss/src/api/v1/sets/offerings.rs` — new file. Two handlers:
  `list_offering_sets`, `get_offering_set`.
- The membership rule (only emit when `coordination = Elected`)
  requires reading the catalog. Each offering tool's compiled manifest
  is reachable via `state.catalog.get_compiled(fqid).coordination`.
- The list groups `GardenRegistry` entries by `fqid` where
  `tool.category == "offering"`. Each member's `role` comes from
  the offering's `OrchestrationState` — which today **does not flow
  through the registry**. The registry's `GardenTool` doesn't carry
  `OrchestrationState`. So either:
  - (a) the registry's `ServiceInfo` gets a new `role: Option<String>`
    field populated on upsert from local `Offering::orchestration`
    (and from beacons for remote offerings), OR
  - (b) the projection joins registry entries with local
    `state.offerings` for local-stone members and accepts
    "role: unknown" for remote members until the beacon carries it.
- Decision deferred. (a) is more work but a cleaner model; (b) ships
  faster but leaks beacon-carried-role design into a follow-up.

**Unknowns:**
- Whether the `OrchestrationState` is currently announced via beacons.
  Need to verify beacon payload schema in `src/common/src/types/discovery.rs`.
- Whether `primary_stone_id` is set consistently or sometimes None
  during normal operation (election in progress, recently joined). The
  endpoint surface needs to reflect "primary unknown" cleanly rather
  than misreporting.
- URI template projection: `tool.service.uri_template` exists; the
  `connection_uris[]` field is straightforward substitution per
  member. The exact substitution rule (host = stone hostname or stone
  IP?) is environment-dependent.

### Phase D — `/api/v1/sets/banks` projection

Goal: the bank set list and detail endpoints, including singletons.

**Files:**
- `src/moss/src/api/v1/sets/banks.rs` — new file. Two handlers:
  `list_bank_sets`, `get_bank_set`.
- Group `GardenRegistry` entries by `tool.storage.replica_set_name`
  (or fall back to `tool.storage.id` for unnamed singletons) where
  `tool.category == "storage"`.
- `total_capacity_bytes` / `total_used_bytes` are straightforward sums
  over members.
- `pin_id` and `primary_stone` come from whichever member carries
  `role = "primary"`. With the rename, that's straightforward.

**Unknowns:**
- Banks with `replica_set_name == ""` — STORAGE-0013 says empty means
  the default set "storage". Need to decide: do these all collapse
  into one set called "storage"? Probably yes — but verify intent.
- ARCH-0026's existing `/api/v1/garden/banks` returns a similar shape.
  The set view should be a strict superset (or close to it) to avoid
  consumer confusion. Likely the `/garden/banks` endpoint becomes a
  thin alias over `/api/v1/sets/banks` post-deprecation, or stays
  alongside. Out of scope here, flag for the implementer.

### Phase E — Pavilion canvas wiring

Goal: ORCH-0039 §"Set-state visualisation" uses the new endpoint.

**Files:**
- `src/pavilion/src/connection.rs` — likely a new
  `streaming_api_for_endpoint` already covers it; raw GET is fine via
  the StoneApi typed client once the endpoint is added there. May
  need a `SetsApi` family on `StoneApi` (extending
  `src/common/src/client/stone_api.rs`) — or pavilion can issue a raw
  GET via `connection::raw_client_for_capture`-style.
- `src/pavilion/src/commands.rs` — replace the proposed
  `get_garden_services` Tauri command with `get_offering_sets`
  returning the Pavilion-shaped projection.
- `src/pavilion/frontend/src/views/Canvas.tsx` —
  - `toSphereShape` includes per-stone offerings derived from set
    membership (each stone's row across all sets where it's a
    member).
  - Stone card: per-offering badge showing role; peer-stones list
    derived from set members.
  - Bank card: `replica_count` already from the bank entry; plus the
    set view's primary attribution.

**Unknowns:**
- Mapping the set-shape (members keyed by stone) back to the
  per-stone-keyed `offerings: []` that `garden-sphere.computeEdges`
  consumes is a small client-side transform. The transform is mostly
  a `flat()` + `groupBy(stone_id)`.
- The `useJobProgress` hook (Item 2 from the original brief) is
  unaffected by this ADR — proceeds in parallel.

### Phase F — Tests

Per kind:
- Unit: membership rule (offerings: elected only, with running
  instance; banks: any registered).
- Unit: role projection (rename validation as a side effect — any
  remaining `Dormant` reference fails compilation).
- Integration: live registry fixture, check projection round-trip.
- Pavilion: ServiceInfo-shape tests retire; new shape tests cover
  set responses.

### Out of scope for this ADR

- `?enrich=true` orchestrator pull-through. Land if a consumer asks.
- A `/api/v1/sets/companions` or other future kind. Add when needed.
- Mutations (`POST /api/v1/sets/banks/.../partner` to add a replica).
  This ADR is read-side only; mutations are the existing per-bank
  pin/unpin and per-offering install/remove flows.
- Deprecation of `/api/v1/garden/banks` / `/api/v1/garden/services`.
  These remain; the set view is additive, not replacing.
- Deprecation of orchestrator `/cluster/*`. Orchestrator dashboards
  keep their internal API.

---

## References

- [TOOLS-0003](TOOLS-0003-unified-garden-registry.md) — the hotcache
  this ADR projects from.
- [ORCH-0039](ORCH-0039-seed-based-offering-replication.md) §"Set-state
  visualisation on the canvas" — the consumer that motivates this work.
- [ARCH-0026](ARCH-0026-storage-api-surface.md) — first-class bank
  endpoints, the precedent for "promote an emergent concept to a named
  noun on the API surface."
- [STORAGE-0013](STORAGE-0013-replica-set-identity.md) — the
  `replica_set_name` identity that bank sets project from.
- [OFFER-0003](OFFER-0003-offering-fqn.md) — the FQN that offering
  sets project from.
- [ORCH-0006](ORCH-0006-coordination-mode.md) /
  [ORCH-0007](ORCH-0007-managed-logical-sets.md) — `CoordinationMode`
  and the orchestrator's reconcile loop that makes set membership
  emerge.
- `src/common/src/tools/types.rs` — `GardenTool`, `StorageMetadata` —
  the fields the projection reads.
- `src/orchestrators/mongodb/src/api/cluster.rs` — the orchestrator
  dashboard precedent that this ADR explicitly does *not* canonicalise.
