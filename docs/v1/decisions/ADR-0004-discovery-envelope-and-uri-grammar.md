# ADR-0004 — The discovery envelope: depth tiers, revision merges, and the URI grammar

**Status:** Accepted · 2026-08-26
**Supersedes:** the lean-only chirp (presence without inventory); the
`/local/*` scope prefix; `/garden/observe` and `/api/v1/manifest` spellings
**Amends:** L22 (three API categories — law survives; prefixes re-expressed),
[ADR-0003](ADR-0003-offering-fqn-namespace.md) (FQN travels raw in paths)
**Referenced by:** OFFERINGS.md §7 slices, DEBT D4 disposition, W6 witness

## Provenance

Post-W4/W5 evaluation cycle, resolved across several exchanges with the
operator. Binding rulings and phrases:

- *"You cannot contact what you don't know exists."* — discovery is a
  precondition, not convenience; the room's ambient traffic must carry
  enough truth to seed contact.
- On joining or on a service going up, a **rich announcement** flows
  ("this is me, and here is what I have"); newcomers may ask **"who are
  you guys, and what do you have?"**; ordinary operations ride *lean*
  heartbeats only.
- Every moss keeps a `garden.topology.stones.*` hot cache, cheap to check;
  rich answers populate it fast; query results backfill it; self is a
  **projection**, never a stored peer (operator accepted the distinction
  and supplied its consequence: the HTTP faces are projections of the
  cache that obviously include the current stone).
- Where an aggregator (Lantern) exists: *"just ask Lantern"* — as a feed,
  never a bypass.
- On URIs: the single objective was *"how clear is it to derive the point
  of the API from its uri?"* — the operator's reading table made `stone` a
  resolvable noun (`/stone`, `/stone/this`, `/stone/{ref}`) and eliminated
  the `/local` vs `/stone` split; the storage grid capped it (*"/api/v1/
  storage … /api/v1/garden/storage — now all capabilities fall into
  place"*).

## Context

v1's chirps assert presence only; `source.rs` composes a static body and its
doc comment admits the dormant intent ("when services exist, a source that
bumps…"). The kernel `Topology` keeps `HashMap<stone_id, StoneView>` fed by
`STONE_CHIRP`/`STONE_GOODBYE` with expiry sweeps — precisely the hot cache
shape required, minus everything that isn't liveness. Consequences: the room
cannot answer "who grows ollama?", O3 adoption detectors have nothing to
consult before probing reality, wall-monitor-class consumers stay impossible,
and PoC experience fills the gap badly (fat chirps forced COMM-0005's ~50%
payload diet; per-domain beacons splintered into three caches/merges).

## Decision

### 1. One envelope, three depths

Every datagram remains one JSON envelope (contract types only; lowercase
discriminators untouched) carrying, at ALL depths, the anchor fields:

```
stone_id, stone_name, address, health, proto, seq, boot_id,
svc_rev          ← monotonic per-boot generation of the offering set
(svc_total)      ← count when truncated below
```

| Depth | When | Carries |
|---|---|---|
| **lean heartbeat** | protocol cadence (default) | anchors only |
| **rich change-frame** | `OfferingChanged` bumps `svc_rev`; debounced ~2s; doubled-send like goodbye | + capped `services[]` (`ServiceEntry` verbatim: FQN, category, status, PORT-0001 residence map), `svc_total` |
| **rich reply** | answering a rich ask (unicast) | same rich body |

Cap: 24 entries alphabetical; truncation is *declared* by counts, never
silent. Envelope ceiling stays under 4 KB with headroom reserved for signed
chirps (PoC ECDSA work). **Never announced:** secrets, declared inputs,
borrowed `connection_url`s.

### 2. Merge by revision, healed by arithmetic

`svc_rev` (later joined by `bank_rev` in the storage slice) is the whole
merge function: concurrent writers and middlemen may reorder frames freely;
per-stone revision comparison makes convergence order-independent with zero
coordination. A peer missing a rich frame heals within one heartbeat (rev
mismatch) via a rate-limited, single-flight-per-target **rich ask** —
multicast while candidates are unknown, unicast once addressed. Stale-rev
frames drop as duplicates. Deterministic local arithmetic replaces consensus
(the BLAKE3-election philosophy, simplified).

### 3. The topology cache grows richness, not machinery

`StoneView` gains `{svc_rev, services[], svc_total}` (bank analog rides the
storage slice); stored/updated through the SAME `STONE_CHIRP` ingest door.
Cache swaps whole immutable generations atomically (`send_replace(Arc<_>)`,
the Factsheet idiom) — snapshot reads are an Arc clone; "cheap to check" is
mechanical. Rules:

- **Self is never ingested.** A `SelfView` projector subscribes to
  `OfferingChanged` (+ facts generation) and maintains one immutable arc;
  chirp composition, `/stone`, and the self-splice of `/garden/stones` are
  renderings of that single composed view. Ingest knows peers; projection
  knows everyone; neither owns truth.
- Query-backfilled knowledge ("who has X?" answers heard in passing) enters
  as **TTL'd candidates**: `(source, seen_at)` stamped, always outranked by
  chirp-borne truth about the same stone, promoted to full entry on that
  stone's first live frame — the offerings ghost-prevention pool,
  transplanted.
- Room-level answers must be reconstructable FROM the cache alone; HTTP faces
  are projections, never recomputation.

### 4. The URI grammar

Tier law (one sentence): **bare nouns name this stone's domain resources ·
`/garden/*` projects the room read-only · deeper paths hang off nouns, not
prefixes.**

| Now | Becomes |
|---|---|
| `GET /api/v1/manifest` | `GET /api/v1` (front door self-description; kills the offering-manifest collision) |
| `GET /health` | unchanged |
| `GET /api/v1/local/posture` | `GET /api/v1/stone/posture` |
| `GET /api/v1/garden/observe` | `GET /api/v1/garden/stones` |
| — | `GET /api/v1/stone` (= me; the SelfView), `GET /api/v1/stone/this` (explicit alias) |
| — | `GET /api/v1/stone/{name-or-id}` → not-here answer carrying that stone's current address (`Location:` header / `knows_at` field) — the garden's only true redirect |
| `POST/GET/DELETE /api/v1/stone/offerings/{n}` | `POST/GET/DELETE /api/v1/offerings[/{fqn}]` |
| `…/rest`, `…/wake` | unchanged suffixes under the new root |
| — | `GET /api/v1/catalog` (derived catalog face) |
| — | *(claimed, storage slice)* `/storage`, `/garden/storage`, `/garden/offerings` |

Rules: requests accept monikers (server canonicalizes per glossary::fqn);
responses speak FQN verbatim; `:` travels RAW in path segments (RFC 3986
legal) — no `%3A%3A`, curl stays human. Clean cut, no legacy aliases: the
route-manifest test extends to forbid BOTH unrouted claims AND unadvertised
emissions (PoC's silent-404 scar — `/api/register` vs `/api/v1/register` —
is thereby structurally barred).

**Mutation clause (reads delegate, writes bind):** reads may be answered or
redirected by any knowing peer; writes execute ONLY at their authority —
for offerings, the home stone (never forwarded); when named storages gain
Primaries (STORAGE-0008 precedent), garden-tier writes RESOLVE TO THE
AUTHORITY (forward to Primary or refuse) rather than executing locally.
Aggregators observe; they never mediate bytes (LANTERN-0001 principle,
retained despite its park).

### 5. Aggregators are feeds, never bypasses

With Lantern present it polls the same projections anyone can poll and
writes through the same candidate-promotion door, its data merely stamped
with its own freshness class. Absent Lantern, gossip continues seamlessly;
nothing checks whether it exists. Stones remain source of truth; the cache
stores projections and liveness only.

## Law encoded

> Presence is asserted cheaply; inventory is earned by change or by question;
> the cache remembers what the room said, stamped and versioned, and every
> surface — wire, command, console — renders the same projection from the
> same single composer.

Supporting rules: revisions settle disputes between mouths, not votes; a
candidate is a rumor until a chirp confirms it; self is rebuilt, never
stored; and if a human can't derive a URI's purpose from its spelling, the
grammar — not the reader — is wrong.

## Amendment A1 — records are paths (2026-08-26, same day)

Operator review of the first envelope implementation rejected the flat
field zoo (`stone_id`, `stone_name`, `moss_version`, `svc_rev`… as root
siblings): *"rootspace holds sections; sections hold facts."* The frame —
and every record of this system — became sectioned, because the rest of
the house already speaks paths (FQNs, URIs, the facts tree):

- `ChirpFrame` replaces `ChirpBody`: `stone{id, name, moss.version,
  network{address, mac}} · presence{health, status} · services{rev, total,
  items[]} · meta{proto, boot_id, seq} · received{discovered_at,
  last_seen}`. The svc anchors become the `services` inventory block; each
  garden domain gets one such block (banks arrive via ADR-0005 §8 with
  `banks{rev,total,items[]}`) — the revision vector is a shape, not a
  field list.
- `ServiceEntry`: `name` speaks the FQN verbatim; `offering` renamed
  `stem` (ADR-0003's lexical term); `status`+`role` grouped under
  `state{}`.
- Discovery request/response join the grammar (request carries the rich
  flag; response's `stone:` block mirrors the frame's).
- **Reception facts separated**: `received` is the listener's record —
  senders emit placeholders, listeners overwrite. The cache can finally
  hold announced truth apart from what we saw.
- **v0 wire compatibility RETIRED.** The flat-shape fixture pins existed
  for a fleet-migration story that died when v1 took its own room (ADR
  cited: own group/port; PoC fleet frozen at `poc-final`; zero contact by
  construction). Fixtures now pin the canonical shape.
- **B1 clarification:** the charter bet (one canonical shape; envelope-vs-
  bare unrepresentable) is honored literally — the topology cache stores
  contract types directly, and HTTP projections render the same frame.
  The earlier "flat at the boundary" comment was a local idiom, not the
  law.
- **Nesting rule:** every level must be a nameable noun (`stone.network.
  address` ✓; `stone.data.info` ✗). Flat remains correct for same-kind
  maps (`ports: {role: n}`) and records too small to have sections.
- Construction ergonomics: `Default` derives on optional-heavy sections;
  call sites use struct-update — the cure for builder noise is Rust's,
  not shallower models.
- Costs accepted: persistence schema changes ride the S5.5 migration slice
  (directory auto-migration pattern, pre-fleet); pure-internal computation
  types adopt opportunistically.

## Alternatives considered

- **Rich everything (services in every heartbeat)** — the exact posture
  COMM-0005 later taxed at ~50% payload; presence must not amortize
  inventory.
- **Periodic rich rebroadcast instead of rev-mismatch asks** — still leaves
  lossy windows without healing guarantees and pays full freight constantly;
  STORAGE-0003's event-driven-beacon principle applies.
- **Per-domain beacon types (STORAGE_BEACON/TOOLS split, PoC-shaped)** —
  produced three caches, three merge paths, three expiries. One envelope
  with a revision vector keeps the pipeline singular while keeping each
  domain's changes independent; the beacon escape valve returns only if a
  future domain proves pathological.
- **Keep `/local/*` alongside `/stone`** — failed the derivability audit
  (two words, no rule distinguishing them); `/stone/posture` teaches the
  noun while naming the guts.
- **Registry-style aggregation through Lantern only** — re-couples
  availability to an optional component; contradicts stones-as-source-of-
  truth and the parked registry pillars' own retrospective.

## Consequences

### Positive

- W6 becomes witnessable end-to-end (plant on A visible from B within one
  interval; loss drill self-heals by rev).
- O3 adoption detectors consult room belief before probing; placement/orchestration
  consumers inherit the same feed.
- Room-level resolution ("who serves ollama::default?") resolves from cache at
  microsecond cost — LANTERN-0001's /resolve TTL contract without the registry.
- D4 closes by construction: rich frames are STONE_DETAIL's push face,
  `/stone` + `/catalog` the pull faces.
- The route-manifest gate bars the unadvertised-emission class permanently.

### Negative

- Wire churn again (tolerable pre-deployment; fixtures updated).
- Consumers must learn FQN-on-wire/moniker-in-rendering (B1/L21 already
  required this discipline for explain).
- Rich asks need politeness bounds (rate-limit + single-flight) — small new
  moving parts, each individually tested.

### Neutral

- `OfferingChanged` broadcast already exists; announcer consumes it debounced.
- L18/L22 data-flow laws untouched: categories still serve from hot state;
  the grammar renamed doors, not flow direction.
- Bank-facing fields are claimed slots per the grid; emitted only when the
  storage slice provides their model.

## References

- Implementation surfaces: `crates/kernel/src/topology.rs`, `probe.rs`,
  `responder.rs`, `crates/contract/src/chirp.rs`, `crates/moss/src/source.rs`
  (dormant-comment payoff), `crates/moss/src/http.rs`, `crates/rake`.
- Slices follow this ADR: fixtures+rev → dynamic source → rich ask/reply →
  cache generations/candidates → SelfView + URI cut → rake sync → **W6**
  witness.
- Prior art: PoC CHANGELOG COMM-0005 (chirp hygiene), STORAGE-0003
  (event-driven beacons + discovery-triggered inventory), topology-cache spec
  (`upsert_from_chirp`, self-entry rebuild), LANTERN-0001 (control-plane/
  multi-active/degradation principles, parked pillars cited as roadmap, not
  blueprint), lantern-dashboard brief ("Lantern is just another offering"),
  the `/api/register` silent-404 scar.
- Operator provenance quotes preserved above verbatim (2026-08-26 sessions).
