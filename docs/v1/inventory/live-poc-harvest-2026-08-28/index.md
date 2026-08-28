# Live PoC Harvest — 2026-08-28

Every non-destructive PoC capability, exercised against the live fleet and
captured raw (human + JSON output, parameter variations, error paths).
**Not** a porting exercise — this is the delight/behavior reference for v1.

## Targets

| Stone | IP | Role |
|---|---|---|
| stone-obsidian-summit | .195 | read-only (mongodb+qdrant+searxng, 20-core, 15GB) |
| stone-topaz-butte | .111 | read-only (searxng+weaviate) |
| stone-emerald-vale | .82 | **destructive test bed** (empty at start) |
| stone-limpid-dune, quartz-fen | ? | chirping but HTTP-unreachable (finding H1) |

## Captures

| File | Command(s) | What it shows |
|---|---|---|
| `offer-catalog.txt` | `offer` | 40+ curated offerings across 12 categories (ai, auth, data, media, networking, observability…); `*` = large images |
| `offer-redis-plant.txt` | `offer redis` | install from catalog; pond-sign-oracle degrade warning; "[accepted create]" acknowledgment |
| `list-after-plant.txt` | `list` | health table (`redis [thriving]`) |
| `status-redis.txt` | `status redis` | ACCESS + SYSTEM + AI panels (arch, flags, kernel, serial) |
| `redis-rest-wake.txt` | `rest`/`wake` | lifecycle: thriving→dormant→thriving |
| `redis-remove-reoffer.txt` | `remove` (soft) | soft delete empties list; re-offer works |
| `redis-uproot-and-errors.txt` | `uproot`, error paths | hard delete ("container destroyed"); `rest ghost::nope` → "[needs attention] Service not found"; `status ghost::nope` shows stone panel (bug: ignores missing service) |
| `adopt-borrow-errors.txt` | `adopt`/`borrow`/`release` | adopt-400 on nothing-stray; `borrow --url` arg parse bug |
| `backup-readonly.txt` | `backup status`/`list` | "No nurturing snapshots configured"; A/B slots + seed-bank restore concepts |
| `election.txt` + `election-status.txt` | `election start` | the L1 envelope bug, live: response parse fail on /election/start |
| `election-status.txt` | `election status` | "Unknown election action: 'status'" — only `start` exists |
| `stone-hey.txt` | `stone --help`, `hey cricket status` | stone ops: wake/shutdown/reboot/verbosity/install; hey: "Unknown subcommand" |
| `capabilities.txt` | `capabilities list` | OFFERING_NOT_FOUND (needs running offering; models/extensions mgmt) |
| `misc-surfaces.txt` | `template`/`ceremony`/`store`/`manifest` | template: fully built (not "coming soon"); ceremony: "not yet implemented"; store: bucket ops; manifest: authoring toolchain |
| `inspect.txt` | `inspect` | full hardware topology via SSH (Dell Wyse 5070, serial, BIOS) |
| `json-variants.txt` | `status -o json`, `find --format json`, `observe -o json` | machine-readable forms (find returns full FoundService objects) |
| `pond-election.txt` | `pond --help` | security surface: init/status/invite/join/drain/remove/untrust ("Phase 3b - pending") |

## Live findings (delight + gap opportunities)

### H1 — Two stones are chirp-only ghosts
`limpid-dune` and `quartz-fen` appear in topology but `tend`/direct-HTTP
cannot reach them. The room knows they exist; nobody can talk to them.
v1's answer: candidates pool (S4) + posture honesty (B3) — a chirp-only
ghost must be visible AND marked unreachable.

### H2 — `status <service>` bug confirmed live
Status ignores the service argument and renders the stone panel. The
hint-to-help contract ("status <service>") is broken in production.

### H3 — Envelope bug class, caught twice live
`election start` and `rake api` both fail on the same bare-vs-wrapped
response parse. This is the single bug class v1's B1 one-envelope law
eliminates structurally.

### H4 — The pond sign oracle degrades gracefully
Every mutating command warns "pond sign oracle unreachable — sending
unsigned" then proceeds. Graceful, honest, visible. v1's B2/M2 must
preserve this behavior class (degrade loudly, never block).

### H5 — Curated catalog breadth
40+ offerings across 12 categories, with compatibility predicates
(`ai.runtime`, arch flags) and per-offering templates. v1's catalog has
the grammar but a fraction of the corpus. The catalog corpus itself is a
capability, not just the machinery.

### H6 — Full lifecycle in three verbs
offer → rest → wake → remove → uproot: five verbs, one stone, zero
documentation needed. The verb metaphor carries the entire learning curve.
v1 has offer/rest/wake/uproot but no soft-remove (only hard uproot).

### H7 — Hardware serials and BIOS via SSH-backed inspect
Inspect goes through SSH (stone/stone) and returns Dell serial numbers,
BIOS versions, and PCIe topology. v1 has none of this — the facts census
measures runtime state, not serial-number identity.

## Verdict feed for poc-bring-assessment.md

- **Port whole**: offer/lifecycle verbs, backup status/list surfaces,
  capabilities concept, inspect hardware topology, curated catalog corpus
- **Port reshaped**: hey relay (contract-typed), election (B4 leases +
  deterministic arbitration), jobs registry (persisted + streamed),
  backup A/B slots (merged into ADR-0005 checkpoints)
- **Cut**: ceremony stubs, template's "coming soon" lie, the bare
  response envelopes
