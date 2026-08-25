# v1 Lessons Ledger — normative constraints for the greenfield

Each rule below is distilled from PoC evidence (see `inventory/*.yaml` for provenance).
The greenfield must satisfy every rule or explicitly argue an exception in its ADR.
Phrased as directives, not observations.

---

## L1 — One wire contract, generated everywhere
Three live envelope mismatches (`rake api`, `/election/start`, companions bare-vs-wrapped)
are one bug class: hand-maintained contracts across components.
**Rule:** a single schema source generates moss handlers, StoneApi, rake, resolvers, and web
types. Bare-vs-enveloped is a compile-time property, not per-handler taste.

## L2 — Security chains to the trust root or does not exist
The chirp verifier checks attacker-supplied keys; presign secrets derive from published
values; TOFU skips the fingerprint check it already holds; three production clients disable
cert validation; the storage proxy plane is unauthenticated.
**Rule:** every verification path names its trust anchor in code and fails closed when
absent. "Unsigned during transition" needs an expiry date enforced by CI, not a comment.

## L3 — Degrade-don't-crash, but never silently
Uniform Unavailable-degradation is good engineering (gateway port proves it) — but security
off is always silent-success today: Observe default, unsigned cold-start sends, dormant pond,
undocumented ZG_POND_ENFORCE.
**Rule:** posture is observable: every stone advertises its enforcement stage, signing state,
and degraded capabilities in /health and chirps. Operators see the gap.

## L4 — Presence is soft-state; duties need leases
90s offline windows, beacon elections resolved post-hoc, no election term limits, gateway
TTLs work because they're explicit.
**Rule:** any *duty* (primary, update-source, coordinator) carries an explicit lease with
heartbeat + expiry + deterministic takeover. Membership presence stays soft-state.

## L5 — Identity lives on the media; identity is not a database row
The .zen-garden manifest contract is the best design decision in storage — plug-and-recognize
works. But roaming detection stops at a boolean nothing consumes.
**Rule:** identity-on-media stays; device-move events must trigger a defined flow
(quarantine/re-auth/replication catch-up), not a display flag.

## L6 — Replication must detect divergence before destroying it
Full-sync deletes replica-local files Primary-wins; a briefly-promoted replica loses unique
writes silently.
**Rule:** divergence is a first-class state (warn, quarantine, operator decision) — never a
silent delete. Cursor+full-sync machinery itself is sound; keep it.

## L7 — The self-description must be true
API manifest documents 26 of 281 routes; offerings doc says 30 vs ~50; offline threshold
45↔90s drift; compose-files described that don't exist; dead pre-install feature documented
as live; firmware versions wrong.
**Rule:** self-descriptions (manifest endpoint, catalogs, docs' load-bearing claims) are
generated or CI-verified against code. Stale truth is worse than missing truth.

## L8 — Small kernel, guests at the edge
Orchestrators-as-containers with gateway leases, companions as spawned peripherals with
command manifests — the inversion works and scales conceptually.
**Rule:** new capability = new process speaking the contract, unless it cannot be. Moss core
stays supervisor + registry + presence + routing.

## L9 — Manifest-driven surfaces are the house style
Rake's command manifest (generates CLI), companion --dump-commands, offering manifests with
predicate DSL — all prove the pattern.
**Rule:** user-facing surfaces declare themselves declaratively and get behavior wired to
declarations. No parallel hand-written help/validation.

## L10 — Distribution wraps the garden, not the reverse
deploy.ps1 using UDP discovery + /stone/deploy to ship binaries fleet-wide is the right
primitive; perennial Docker builders solve cross-compilation; the musl-under-QEMU trick works.
**Rule:** keep fleet-native deployment; add tags→artifacts→signing upstream of it. Sleeping
stones get wake-and-catch-up on boot (nourish-before-serve) rather than indefinite skew.

## L11 — Ceremonies without recovery consumers are type-fiction
Vacate/Replant modeled, unexecuted; ceremony journals written but nothing resumes them at boot;
jobs non-persistent while carrying long installs.
**Rule:** no state machine ships without its crash-recovery path implemented and tested.
Descope honestly instead.

## L12 — Name things once
Three unrelated "leases", two "banks", two enabled-flag file formats, two canonical-layout
validators, duplicate STONE_CHIRP constant, version scheme drifting between entry points.
**Rule:** a shared glossary crate owns domain nouns; lint for duplicate constants; formats
have one writer.

## L13 — Secrets derive from secrets
Presign-from-fingerprint is the cautionary tale; vault usage elsewhere is sound.
**Rule:** key material comes from the vault or is generated per-install; identifiers
(fingerprints, stone_ids) are public by definition and never key derivation input.

## L14 — Every platform is a citizen or gets a fence
Windows storage is scan-only yet mount allowlists reject drive letters; macOS absent from the
Platform enum while APFS tokens exist; Windows CWD-relative data_dir sprays repo roots;
Pavilion parked out-of-tree.
**Rule:** each platform's supported surface is declared (enum + capability table); anything
outside it is refused loudly at startup, not halfway through operations.

## L15 — Tests run where CI runs
Orchestrators carry ~162 tests CI never executes; local Linux builds skip tests; probe e2e
needs plink + stone/stone creds.
**Rule:** CI matrix mirrors build matrix including test execution; e2e harness authenticates
the way production does.

## L17 — Initialization preserves capability build-up
The PoC's phase machine had holes, mid-sequence process exits, and task sets that
varied between boots — capabilities built early could silently vanish before serve.
**Rule:** startup is an ordered pipeline where every step *produces* capabilities
that later steps consume; nothing tears down or skips silently. A step that cannot
run aborts startup loudly with its name — it never leaves a half-built garden.

## L18 — Events inside, polling only at the edge
The PoC polled itself: 30s health sweeps, 30s topology maintenance, 5s network
probes — domain truth discovered by asking on a schedule.
**Rule:** domain internals communicate by events (broadcast/watch channels),
never by polling each other. Polling is permitted only at external seams that
offer no push alternative — and even there, prefer the event stream the outside
world already provides (Docker's event stream, filesystem watchers, sockets).

## L16 — Delight is load-bearing
Firefly compile-time latency asserts, cricket tunes, pulse's no-raw-mode rule, named ponds,
portrait colors — the aesthetic layer is engineered like the rest.
**Rule:** the greenfield budgets for companions/pulse/portraits as first-class subsystems
with contracts (COMPANION-0004/0008/0016/0018 carry forward), not garnish.

## L19 - Transcribe, don't recall
v1's first wire constants were written from memory of the inventory and came
out UPPERCASE; the PoC's discriminators are lowercase (`stone_chirp`). The
fixture test pinned the error faithfully - a test that quotes memory pins
nothing. Caught by reading the frozen oracle before field day one; had v1
joined the live garden, every PoC stone would have silently ignored it.
**Rule:** every fixed-point constant is transcribed from source evidence at
authoring time (file:line in the comment), never recalled. A fixture's
expected bytes come from the oracle, not the author. When any doubt exists,
capture a live datagram before first contact.

## L20 - Informed inheritance
The PoC proved that things work; v1 exists to make better, informed
decisions about everything the PoC did - not to re-implement its choices.
v1's first defaults inherited the PoC's discovery room "for coexistence";
the need was assumed, never stated, and it shaped code (an --isolate flag,
a debt entry) before anyone asked for it.
**Rule:** every PoC mechanism arrives at v1 as evidence, not as law.
Re-derive the justification before inheriting the shape; when the
justification is absent, decide fresh and record why (topology separation
was one such decision). On-media formats stay forever-compatible (R0.5);
network and internal design answer only to evidence plus intent.

## L21 - Rake is a thin client
PoC rake spent its intelligence on exactly three things: finding and
validating a moss to attach to, routing commands and parameters onto that
moss's methods, and expecting standard return formats. That is all. Every
core capability lived at moss.
**Rule:** rake never computes garden truth - it renders what an attached
moss reports. Rake-side discovery exists to establish attachment, not to
build rake's own view of the world. When a feature tempts rake to know
something moss does not tell it, the feature belongs in moss.

## L22 - Three API categories, one hot cache
Moss's API splits mentally - and therefore literally - into three
categories: local service health (am I well), Stone data/operations (this
machine), and Garden data/operations (the whole room). Garden state lives
in ONE hot topology cache: cheap to write, cheap to read, every update
lands there. Readers take snapshots; nobody polls anyone (L18).
**Rule:** moss routes declare their category in the path
(`/api/v1/{local|stone|garden}/...`). All garden-wide data flows through
the single topology cache - no parallel stores, no per-handler views.

## L23 - Names are universal; host renames are appliance-only
Every stone carries a poetical garden name. The machine's own hostname
changes only on dedicated appliance stones, where the stone IS the box
(PoC first-boot renamed hosts there). A cohabiting workstation keeps its
own name; moss is a guest.
**Rule:** identity is two-layered: the garden name (always) and the host
name (appliance modality only). Default modality is companion - never
mutate a host you were merely invited onto.

## L24 - Rooms take a breath before they carry
W2, first cross-machine contact: joins registered at the kernel, sockets
bound, firewalls open - yet the room was silent between stones for minutes.
Then, with zero code changed, all heartbeats flowed. The switches were
running IGMP snooping without an active forwarding state for our group;
a querier cycle had to pass before group traffic crossed.
**Rule:** after any change to room membership or network path, allow one
querier cycle (roughly 60-125s) before declaring discovery broken. A
silent first minute is convergence, not failure. Witnesses budget for it;
tests asserting instant cross-host delivery measure the switch, not the
garden.
