# CONTINUATION — read me first, then delete me

Written 2026-08-26 at a planned pause, mid-epic. Updated 2026-08-27: S3
(songs/rich ask), S4 (candidates), S5 (URI cut), S6 (rake sync), S5.5
(persistence v3), S7a (storage MVP), S7b (the room's storage grid), S8
(fleet deploy) + **W6 witnessed** all landed. THE EPIC IS COMPLETE — see
WITNESSES.md W6 for the live proofs and the three findings it harvested.
What remains is OUTSIDE this epic: W7 (ADR-0005 capture/replant),
Lantern, O3 adoption, then the M-milestones. Self-contained for a clean
context. Verify everything against the tree — trust files over this doc.

## Project in one paragraph

Zen Garden: self-hosted service orchestration on repurposed hardware
("stones"). Services outlive machines. The PoC (`src/poc/`, branch `poc`,
tag `poc-final`) is the frozen oracle. v1 is being built in `src/v1/` under
an accepted constitution. Current epic: **the garden that knows itself** —
ADR-0004 discovery envelope (chirp/song, inventory map, topology cache) +
storage MVP with the USB adoption ceremony, ending in fleet deploy + W6
witness.

## Authority (read in order; conflicts resolve downward)

1. `docs/v1/lessons.md` — L1–L26 normative
2. `docs/v1/CHARTER.md` — accepted, amended; bets B1–B11
3. `docs/v1/CODE-RULES.md` — P0–P5; **R3.9 "records are paths" is new**
4. `docs/v1/OFFERINGS.md` — offerings law (§5.1 layered catalogs, FQN
   namespace, named installations)
5. `docs/v1/decisions/ADR-0001..0005` — directory, ports, FQN namespace,
   discovery envelope (A1 records-are-paths, A2 inventory-map/songs/framer),
   living will (capture/checkpoints/replant + §8 storage-on-envelope)
6. `src/v1/DEBT.md` (D1–D14; D14 closes on W5/W6 work), `src/v1/WITNESSES.md`
   (W1–W4 recorded)
7. `docs/MEMORY.md` — durable memory index; `local/NOTES.md` — machine
   facts (gitignored): fleet IPs (.82/.111/.195, keys in plink), v1 room =
   UDP 7284 / group 239.255.42.199, HTTP 7285

## Git state (branch `dev`, PUSHED to origin; `main` deferred to the release candidate)

Remote: `git@github.com:sylin-org/zen-garden.git` (SSH — the HTTPS PAT
lacks `workflow` scope and GitHub rejects pushes touching
`.github/workflows/` over it). `dev` pushed 2026-08-27 (a73bf12a). Branch
law: only `dev` (trunk) and, from the RC onward, `main`. Local branches
were pruned to dev alone: the PoC lives only as tag `poc-final`; Pavilion
is parked at tag `pavilion-parked` (charter B10 recovery). The two remote
`ecc-tools/*` tool-branches were deleted.

```
a73bf12a docs: continuation updated - S3 landed (rich ask/reply + songs), resume at S4
df3dfbe5 feat(v1): S3b - boots and changes sing, heartbeats chirp lean; the cache merges by rev
7ebe96de feat(v1): S3a - rich ask, rich tell: probe speaks depth, responder answers with inventory
fbf889cc docs: CONTRIBUTING - the lightweight contributor path
f7b3a9b3 docs(v1): orientation - one datagram's life, boot to expiry
ee607965 docs(v1): glossary speaks its metaphors - every garden word carries its standard-term gloss
```

Working tree is CLEAN (91 tests green, clippy -D warnings clean). 2026-08-27
also landed (before S3): R1.1 registers amendment, glossary metaphor
glosses, `docs/v1/orientation.md`, root `CONTRIBUTING.md` (lightweight
contributor path), `docs/v1/design/dx-delight-research.md` (one OPEN ruling
recorded there: CLI register — session recommends keeping garden verbs;
operator agreed in conversation, amendment not yet written).

## ~~⚠️ BLOCKER~~ RESOLVED (kept for the lesson)

The S2 hang was a tokio watch deadlock: `send_replace(source.version_tx
.borrow()…)` holds the read guard across the write attempt. Fixed with
`send_modify(|v| *v = v.wrapping_add(1))` under one lock (9df8c53b). The
idiom comment lives in source.rs. No open blockers.

## The canonical shape (S1.5/S1.6 — MEMORIZE before touching wire code)

`ChirpFrame` (contract/src/chirp.rs), sections per R3.9:
`stone{id, name, moss.version, network{address{ip,port,tls_port}, mac}} ·
presence{health, status} · inventory{...} · meta{proto, boot_id, seq,
part{n,of}} · received{discovered_at, last_seen}`.

- `inventory: InventoryMap` — closed rootspace. Typed knowns:
  `services: Option<Inventory<ServiceEntry>>`; `banks` claimed slot
  (`_banks_slot: Option<serde_json::Value>` — type it in S7b);
  `extra: Map` passthrough round-trips unknown domains losslessly.
- `InventoryMap::insert("services", v)` decodes typed, preserves verbatim
  if undecodable; `from_pairs` builder; `merge_frame` = per-domain rev rule
  (absent key keeps, present block's rev speaks; unknown = last-write-wins).
- `ServiceEntry{offering_id, name(FQN!), stem, category, state{status,
  role}, ports}`. `Offering::service_entry()` in model.rs produces it.
- **Song vs chirp**: `stone_chirp` = lean heartbeat (rev-only inventory
  blocks since S3b); `stone_song` (`stone_song` discriminator, EMITTED by
  the announcer since S3b on boot + change) = full-voice.
  `contract::song::frame_song(base, blocks, seq)` quantizes domain blocks
  whole against `FRAME_BUDGET_BYTES=3500`: biggest-first greedy,
  every frame re-anchored, `meta.part` informational, empty → no frames.
  The composer caps songs alphabetically at INVENTORY_CAP=24 with `total`
  declared; heartbeats speak revs, never items.
- Fixtures (`contract/tests/wire_fixtures.rs`) pin the CANONICAL shape;
  v0-compat RETIRED (v1 owns its room). R3.9 law in CODE-RULES.
- Discogs: lowercase discriminators (`announcement::*`) — pinned.

## Epic map (todowrite list mirrors this)

- [x] S1/S1.5/S1.6 — wire anchors, canonical frame, A1+A2 amendments,
      inventory map, song+framer (committed)
- [x] **S2 — DynamicChirpSource LANDED (9df8c53b).** Rev starts at
      max(snapshot.len(),1); bumps on OfferingChanged (lagged bumps once);
      version watch fires → announcer's existing debounce (L18) emits
      change-chirp. Interim: inventory rides plain chirps; songs wire-up
      lands in S3.
- [x] **S3 — rich responders + songs wire-up LANDED (7ebe96de, df3dfbe5).**
      Probe `ask_the_room_rich`; responder parses `rich` and answers with
      `DiscoveryResponse{stone, services: Some(inventory)}` from the
      source's song blocks (undecodable ask → lean card, R2.5); moss boot
      ask is rich. `ChirpSource` gained `song_blocks()`; body() is LEAN
      (rev-only blocks); announcer sings `stone_song` (new discriminator,
      contract consts) on boot + debounced change, quantized by the
      framer, capped 24 alphabetical with `total`. Topology claims
      `stone_song` and merges inventory by per-domain rev (merge_frame) —
      a lean heartbeat never wipes what a song taught (regression test
      pins this). S4's merge-on-ingest is thereby ABSORBED here; S4 keeps
      candidates pool + promotion.
- [x] **S4 — candidates pool LANDED (af2ebe41).** Overheard rich answers
      land as TTL'd `Candidate`s (id required; live truth ignores gossip;
      first live frame retires the rumor; `CANDIDATE_TTL_SECS=300` in
      contract consts — outlives the L24 querier window). Candidates are
      NOT members: no snapshot rendering, no version bump, silent expiry.
      The old on_response hint-entry behavior is gone (no more `starting`
      phantoms in observe / no Expired events for stones never met).
      merge_frame-on-ingest landed earlier with S3b. S4 complete.
- [x] **S5 — URI grammar cut LANDED (0588c244).** Clean cut, no aliases:
      `/api/v1` front door (the manifest), `/stone` + `/stone/this`
      (SelfView: chirp source body re-voiced with song_blocks — AppState
      gained `chirp_source`), `/stone/{ref}` (me by name-or-id; peers
      answered 404 + Location + `knows_at` — the delight face; unknown =
      plain 404), `/stone/posture` (gained candidates count),
      `/garden/stones` (self spliced first, `"self": true`), `/catalog`,
      `/offerings[/{fqn}][/rest|/wake]`. L9/R4.7 structural win: the
      router is BUILT FROM the `Face` enum table — routes exist only as
      manifest rows; tests: every face answers, legacy spellings dead,
      front door lists all faces exactly once, redirect + splice pinned.
      Topology gained `find(id_or_name)`. **rake is knowingly BROKEN until
      S6** (calls /garden/observe + /stone/offerings paths).
- [x] **S6 — rake sync LANDED (10d07809).** Paths repointed
      (`/garden/stones`, `/api/v1/offerings`, moss_http fixture);
      GardenStone parses the splice (`is_self` from `"self"`,
      `chirps: Option`); observe table gained an OFFERINGS column
      (declared total or visible items, `-` when silent) and a `(me)`
      marker on the spliced self row. Rake speaks the new grammar.
- [x] **S5.5 — persistence v3 LANDED (6f1e72bf).** record.json/candidate.json
      render the sectioned v3 view (`record.rs`: identity{offering_id,
      name, stem, category} · state{status} · location · mode ·
      registered_at/updated_at); plan scalars re-homed under `meta`
      (PlacementPlan{workload, decisions, meta{plan_hash,
      facts_generation}}) — record embed AND plan.json sidecar speak one
      shape. Load auto-migrates v2 flats: source renamed
      `*.json.migrated`, sectioned truth written fresh, embedded plan
      re-sectioned; idempotent; HTTP offerings faces render the SAME v3
      view (rake renderers updated). model::Offering's flat serde REMAINS
      as the legacy reader — that is the forever-compat surface (R0.5).
- [x] **S7a — storage MVP LANDED (af0abed6).** `offerings/storage.rs`:
      Bank {fqn (ADR-0003 grammar), device_id GUIDv7, state mounted|ejected
      (glossary::bank), roles[], capacity/used TELEMETRY}; scan of
      removable volumes (sysinfo); adopt ceremony writes
      `.zen-garden/manifest.json` (STORAGE-0009) — `rake storage adopt` 1:1
      with POST `/api/v1/storage/adopt` (operator's new standing law: every
      CLI verb has its API face, recorded in MEMORY.md); `rake storage`
      1:1 with GET `/api/v1/storage` (banks + adoptable). Mount watcher
      (5s edge poll) reconciles: mount/eject bump, measurements ride.
      **Pulled forward from S7b:** the contract `banks` slot is TYPED
      (`Inventory<BankEntry>`, DOMAIN_BANKS const) and merge_frame
      generalizes rev-merge to it — no interim passthrough debt. Source
      composes banks in BOTH registers (lean rev-only, songs full);
      follow_storage_changes wires storage bumps -> bank_rev -> song.
- [x] **S7b — the room's storage grid LANDED (26724924).** `GET
      /api/v1/garden/storage`: self's banks spliced first, then every
      peer's banks from the one topology cache — rows name the holding
      stone; 1:1 `rake storage garden`. Eject verb pair: POST
      `/api/v1/storage/{fqn}/eject` + `rake storage eject <bank>` —
      authoritative absence, sung. Eject LAWS (storage.rs, pinned by
      tests): an operator's eject holds for the same slot for the boot's
      life (no flip-flop with the watcher); physical absence releases the
      hold (return = true re-plug, remounts); a different slot is a true
      re-plug; vanish-ejected banks remount on return. End-to-end test:
      peer song with banks -> topology merge -> /garden/storage renders
      the foreign bank.
- [x] **S8 + W6 — fleet deploy + witness LANDED (6566fad3 + WITNESSES.md
      W6).** linux-x64 release via the perennial builder; deployed and
      upgraded on 192.168.1.111 + .195 (`.82` offline — same procedure
      when it returns). Witnessed live: mutual presence; plant on A
      visible from B <= 1 interval; rev-heal drill (cache refill by rich
      ask + rehydration); USB adopt garden-wide (adoptable scan ->
      recognized -> heard on the peer -> eject -> absence heard). THREE
      findings harvested: self-ingest defect (fixed, 6566fad3),
      MOSS_RUNTIME=docker needed for appliance default, adopt-vs-
      point-of-restore permission posture (operator's call).

Deliberately OUT of this epic: capture/checkpoint pipeline (ADR-0005 core,
W7), Lantern, O3 adoption.

## Conventions & gotchas (new since last continuation)

- **tokio watch deadlock**: never `.borrow()` inside `send_replace` args.
  Prefer `send_modify`.
- `#[tokio::test]` = current_thread runtime; spawned tasks interleave at
  awaits — hangs above looked like "watch never fires", were lock deadlock.
- `expect_used` is deny in prod code: test modules need
  `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
- Witness/ADR convention: once Accepted, ADRs get AMENDMENT sections (A1,
  A2…), never edits. todowrite list mirrors the epic slices.
- The full workspace `cargo test` timed out at 600s during the hang — after
  the fix, run per-crate first (`-p garden-moss --bin moss source::`),
  then full suite.
- Old continuation's gotchas still valid: `gen` reserved; tokio interval
  first-tick; rg+PS quoting; SO_REUSEADDR (D8); one moss per host while
  developing (stop old moss.exe before rebuilding — file lock).

## Key file locations

| What | Where |
|---|---|
| Frame/inventory/song/framer | src/v1/crates/contract/src/{chirp,discovery,song}.rs |
| Wire fixtures (canonical pins) | src/v1/crates/contract/tests/wire_fixtures.rs |
| Announcer (chirp on change, heartbeats) | src/v1/crates/kernel/src/announce.rs |
| Topology cache | src/v1/crates/kernel/src/topology.rs |
| Probe/responder (ask/tell) | src/v1/crates/kernel/src/{probe,responder}.rs |
| **S2 source (BLOCKER here)** | src/v1/crates/moss/src/source.rs |
| moss wiring | src/v1/crates/moss/src/main.rs |
| HTTP surface | src/v1/crates/moss/src/http.rs |
| rake | src/v1/crates/rake/src/main.rs |
| Offerings stack | src/v1/crates/moss/src/offerings/ |
| ADRs | docs/v1/decisions/ADR-0001..0005 |

## Resume procedure

1. Read this + `git log --oneline -5` + `git status` (expect clean tree,
   dev pushed).
2. The epic is DONE. Pick the next arc per charter sequencing: M1 release
   pipeline (main branch + tag->build->sign->publish), or W7's living-will
   work (ADR-0005 capture/checkpoint/replant — the next epic candidate).
   Re-run the deploy procedure for 192.168.1.82 when that stone returns.
