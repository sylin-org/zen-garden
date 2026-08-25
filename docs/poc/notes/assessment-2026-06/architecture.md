---
audience: [maintainer, contributor]
doc_type: assessment
status: current
date: 2026-06-11
---

> Part of the [June 2026 project assessment](README.md). Line counts and consumer claims in this document
> were adversarially re-verified against the tree (target/ excluded); corrections are folded in.

# Architecture: Findings and the Lean-Core Target

## 1. What the codebase is today

~284k active Rust lines across 13 workspace crates + 7 standalone orchestrator crates. The center of
gravity:

- **moss** (118.1k lines, 392 files) — far more than its own `lib.rs` description ("service discovery
  daemon"): ~27 functional subsystems, 280 route registrations (213 unique method-path endpoints) in one
  hand-written 1,317-line router, 32 always-on + 4 conditional supervised tasks plus ≥8 long-lived tasks
  spawned outside the supervisor.
- **garden-common** (38.0k lines) — not a contracts crate but "moss's second half": ~13.6k verified
  moss-only lines (compatibility/, console/, resources/, detection/, most of manifests/, infra/{timer,
  archive,debounce,process,platform}, templates.rs, traits/, persistence/, platform_runtime.rs), ~3k
  verified zero-consumer lines (jobs/ 1,090, events/ 731, uri/ 838, stone.rs 202, client/api.rs 160,
  errors.rs 2 — all confirmed with zero imports across 17 consumer crates), plus a live UDP multicast
  transport singleton that no contracts crate should carry. No feature flags, so every companion and
  orchestrator image compiles the whole mass.
- **orchestrators** (68.5k lines) — generational pile-up: ollama (14.8k, the deployed/published/documented
  generation) and ai (41.9k Rust + React dashboard, the newer design but operationally dormant — last
  commit 2026-04-12, absent from build scripts, no Hub push, never registers with the gateway). Plus three
  scaffolds (postgresql 451 / valkey 448 / weaviate 404 lines) whose `main()` logs a placeholder and
  exits, and 1,359 lines of orchestrator-common cluster primitives consumed only by those scaffolds.
- **peripherals** — companions (13.7k; engineering-excellent SDK serving exactly two in-tree binaries),
  pavilion (6.8k Rust + 4.6k TS Windows Tauri client, flagship feature admittedly not working end-to-end
  per PAVILION-0002, idle since May 6), lantern (2.4k; the LANTERN-0001 registry pillars — SQLite,
  election, HA — absent from current code; what shipped is a topology dashboard with TTL heartbeats,
  resolve, and SSE), probe (9.7k; the only real integration-test surface, stale since Mar 22).

### Verified architectural debts (the load-bearing ones)

| Debt | Verified detail |
|---|---|
| Dead code in-tree | `infra/cloud_filter/` 2,475 lines / 6 files, unlinked from the build since commit 9fcb49db (PAVILION-0001) — uncompilable; `domain/cloud_drive.rs` 332 lines compiles but its only consumer is the dead cloud_filter code |
| Two AI-orchestrator generations | 57k lines carried in indecision; the dormancy contrast is *operational* (deploy/announce/docs), not code-age — ai's last substantive work (Apr 11–12) postdates ollama's (Mar 25) |
| Three backup generations | harvest 1,412 + nurturing 2,732 + ORCH-0039 snapshots 3,242 ≈ 7,400 lines; **three** HTTP route families; two independent capture engines (harvest, snapshots) plus nurturing as an orchestration layer wrapping harvest; only the snapshot scheduler is in-process (nurturing is fired by OS timers hitting HTTP triggers); the seam already caused the May disk-fill incident |
| Router duplication | `configure_public()` (84 registrations) vs `configure()` (196): ~266 verbatim-duplicated code lines; confirmed drift bug — `GET /api/v1/stone/banks/{moniker}/seeds` exists **only** in the public set, contradicting the file's own doc comment that HTTPS serves all routes |
| Duplicate structs | The "NO duplicate structs between moss and rake" rule is violated *directly* by 21 struct names duplicated moss↔rake (StorageOverview, HarvestManifest, ListBucketResult, S3Object, PlacementRecommendation…), several field-identical wire contracts; plus ~12 genuine same-concept duplicates between common and moss/rake (EventBus, Job/Jobs, CompanionSummary, DiscoveredStone, ApiResponse…). Note: moss's `Current`/`Stone` is *prescribed* by code-standards.md §5, so that one is a documented design, not a violation |
| Unsupervised spawns | Registry says "No second path, no duplication" (`task_registry.rs:35`) while ≥8 long-lived tasks run outside it (snapshot scheduler — its JoinHandle discarded at `bootstrap/run.rs:183` — DockerMonitor, network monitor, discovery UDP listener, 3 event-bus listeners, transport tap); 77 raw `tokio::spawn` sites in moss (excl. tests). The snapshot-runaway incident lived in the bypass path (though its root cause — no retention — was not a supervision failure) |
| Layering vs standard | code-standards.md §6 (FromRef narrow handlers): followed by 24 handlers, while 162 take full `State<Moss>` — and ARCH-0035 explicitly decided to skip the migration, so the published standard contradicts a published decision; ~31 domain→infra references remain against the "domain never imports infra" rule, 8 of them hard (each has a sanctioned default-type-param pattern available) |

The honest counterweight: the post-ARCH-0017 core is genuinely disciplined — a 286-line thin `Moss`
container with 22 domain aggregates, the bollard anti-corruption layer with zero leaks (verified), typed
domain errors, an unwrap-free domain layer, and 2,483 unit tests. The discipline is real; it is recent,
uneven at the edges, and never enforced by automation.

## 2. The lean core (target shape)

The project already wrote its own target architecture. The philosophy corpus defines the core as
"discovery + curated offerings + lifecycle + opt-in pond + opt-in lantern," gated by a one-sentence test
and a "real users asked for it" trigger (`docs/philosophy/staying-focused.md`). The repo stopped applying
those gates around February; the maturation task is to apply them retroactively.

| Part | Mission (one sentence) |
|---|---|
| **moss** (daemon) | Discover peer stones, plant/adopt/tend curated offerings on Docker, keep them healthy, updated, and self-updating — and replicate their data so a dying laptop loses nothing. |
| **rake** (CLI) | The single human interface: one manifest-driven grammar, one output pipeline, one exit-code contract. |
| **garden-contracts** (common, reborn) | The serde wire contracts, constants/paths, utils, and the typed `StoneApi` client — and nothing that only one consumer compiles. |
| **garden-discovery** (+ transport) | The client-side browse half of discovery and the single UDP/mDNS transport, on one mDNS stack instead of two. |
| **installer / deploy pipeline** | Bare hardware → enrolled, self-updating stone, on Linux/Windows/Android (HOST-0001, DEPLOY-0001). |

### Before → after map

| Unit | Today (Rust LOC) | Target | What moves |
|---|---|---|---|
| moss | 118.1k | ~105k | +13.6k absorbed from common (the verified moss-only mass); −13k storage gateways extracted (S3, WebDAV, sets, garden-FS); −4k backup consolidation (3 generations → 1); −2.8k dead (cloud_filter + cloud_drive); −~5k AI-capability surface to the orchestrator side; −~1.5k stubs (TimerListener 617, NoAuth, router duplication, tombstones) |
| garden-common | 38.0k | **~18k** | −13.6k to moss, −3k zero-consumer dead, −1.5k p2p transport out, uri/ (838) parked with its corpus until a resolver is wired; feature flags (`client`, `system`, `transport`) so cricket stops compiling reqwest+sysinfo for a type import |
| rake | 27.5k | ~25k | delete unrouted commands (Lift/Place/Invite/presence), coming-soon stubs, dead flags; one output system (OutputWriter has zero uses today) |
| orchestrators | 68.5k | **~24k** | ai parked (archive branch/sibling repo); stubs deleted (1.3k); unused cluster primitives deleted (1.36k); ollama ported onto orchestrator-common, its ~1.6k drifted forks deleted |
| pavilion | 6.8k + 4.6k TS | **parked** | own repo or explicit freeze; flagship feature broken end-to-end (PAVILION-0002), idle since May 6 |
| companions | 13.7k | 13.7k, frozen | declared feature-complete (COMPANION-0011 closed the epic); strip declared-but-unwired surface (DeliveryPolicy variants shipping as `All`, `required_dependencies`) |
| lantern | 2.4k | 2.4k, renamed | retire the LANTERN-0001 registry pretense; it is the opt-in garden dashboard (resolve + SSE + heartbeats stay) |
| probe | 9.7k | 9.7k, revived | the single integration-test surface; the dead `tests/` directory and the other two integration generations retire |
| **Repo total** | **~284k active** | **~205–220k active** | ~16k hard-deleted, ~49k parked out of the active tree, ~13k feature-gated |

The honest caveat: moss itself barely shrinks, because it absorbs ~13.6k lines of its own code currently
mislabeled "common." The "less" is real but lives at the repo level — unit count drops from ~20 crates to
~13, two whole product surfaces leave the tree, and every remaining crate's label becomes true.

## 3. Tiering

**Tier 0 — mission core.** Moss's discovery/topology/P2P (~5.5k), offerings lifecycle (~11k — the core
value, battle-tested), the sealed docker/ layer, pond (~4k, crypto delegated to koi-certmesh),
updates/self-update + installer, health/jobs/metrics/capacity aggregates, **and the storage durability
substrate** (banks, volume roles, replication, one snapshot subsystem — see §5). Plus rake,
garden-contracts, garden-discovery, and the deploy pipeline. Every item passes the one-sentence test
against "a grandmother's retired laptop runs your development database."

**Tier 1 — differentiating extensions.** (a) The **ollama orchestrator**: VRAM-aware placement across a
mixed fleet is shipped by nobody at homelab scale (see [strategy.md](strategy.md)); ollama is the
deployed, published, documented generation. (b) The **mongodb orchestrator**: autonomous choreography is
the validated gap management UIs don't touch, and it delegates deployment to Moss correctly. (c) The
**companion joy layer**: joy is a stated functional requirement and the SDK is among the best-engineered
code in the repo — frozen, not growing, until a third-party companion exists. (d) **Lantern-as-dashboard**
and portrait. (e) **Probe**, wired to scheduled execution.

**Tier 2 — park or extract.** (a) The **ai orchestrator** — archive to a branch/sibling repo with a
succession ADR; harvest its EventBus and GPU-claim designs as design references. The "real users asked"
trigger has never fired for it. (b) **Pavilion** — park with a status note. (c) **Storage protocol
gateways** (S3 + WebDAV + sets + garden-FS HTTP, ~13k) — feature-gate or sibling crate behind the existing
Storage domain ports. (d) **uri/** — park with its cross-language corpus until a resolver is wired, or
wire it; do not carry it dead. (e) The greenhouse/pond/pulse embedded SPAs move toward one shell;
portrait stays as the stone landing page.

**Tier 3 — delete.** Everything in the [shed-register](shed-register.md) marked Delete — all verified
zero-consumer or self-described dead.

## 4. Sequencing

| # | Workstream | Contents | Effort | Risk |
|---|---|---|---|---|
| 1 | **Gate the tree** | Merge `fix/snapshot-scheduler-disposal` into dev; switch koi path deps to published crates.io versions (+ local patch for sibling dev); minimal CI (check + test + deny, plus an orchestrator lane); cut a v0.x tag; delete dead `tests/` + root scratch files | M | Low. Unblocks everything — no 100k-line refactor is safe without an automated gate |
| 2 | **Pure deletion (Tier 3)** | cloud_filter/cloud_drive, common's dead modules, orchestrator stubs + unused cluster primitives, rake dead surface, stale branches | S | Low — every item verified zero-consumer; git history preserves all of it. ~12–16k lines, zero behavior change |
| 3 | **Generation decisions (Tier 2 parks)** | Park ai (succession ADR; flip ORCH-0013/0028–0030 statuses); park pavilion; rename lantern; retire LANTERN-0001's unbuilt pillars | S decision / M execution | Low-medium. ~49k lines leave the active tree. Parallel with #2 |
| 4 | **Common split** | Move the ~13.6k moss-only lines into moss; p2p transport into garden-discovery/-transport (collapsing to one mDNS stack); resolve duplicate structs (the 21 moss↔rake wire-contract duplicates first); interim feature flags | M | Medium — wide but mechanical; directly serves the musl/Android targets under active investment. Depends on #1 |
| 5 | **Consolidate the machinery** | One backup subsystem (ORCH-0039 absorbs harvest + nurturing); all spawns under the supervisor; one declarative route table replacing configure()/configure_public(); close the unauthenticated-HTTP hole; finish or formally close the domain/ root-file migration (37 loose files, 12.9k lines) | L | Medium-high — touches lifecycle and security where reality pushed back hardest (`bootstrap/run.rs` is the most-fixed file, 15 fix commits). Revive probe first |
| 6 | **Storage demarcation** | Extract S3/WebDAV/sets/garden-FS behind the feature gate; keep banks/replication/snapshots in core; port ollama onto orchestrator-common; move moss's AI-capability surface to the orchestrator side | L | Medium — the domain-port seam exists, but integration tests are near absent; sequenced last so #1/#5 protect it |

## 5. The storage question, argued both ways

**Keep it core:** replication is the *reliability answer to scavenged hardware* — the premise is machines
past their warranty, and the philosophy puts reliability first. It is the data-sovereignty story made
concrete. No competitor combines orchestration with data durability at this scale (single-node app stores
don't; Uncloud's replicated volumes are roadmap). Snapshots and the upgrade ceremony already depend on
seed banks.

**Cut it:** it is ~25k lines — over a fifth of the daemon — implementing a NAS inside a discovery daemon,
with three overlapping access surfaces (per-bank S3 ports + legacy S3 path + WebDAV), a custom non-SigV4
presign scheme, fake WebDAV locks, 18 production unwraps in `s3_gateway.rs` beside the helper written to
prevent them, and zero end-to-end tests for replication or S3 conformance. Projects die of maintenance
obligation, not code quality; Sandstorm died under self-imposed update obligations. Syncthing and TrueNAS
already serve file-sovereignty; users can compose them.

**Verdict — split along the seam that already exists.** Keep storage-as-durability in Tier 0: banks,
volume roles, Primary/Replica election, changelog replication, and the single surviving snapshot store.
That is what makes offerings on dying hardware trustworthy — the differentiator. Extract storage-as-NAS
to Tier 2: S3 gateway, WebDAV, sets, and garden-FS HTTP become a feature-gated extension with its own
cadence, behind the Storage ports. One blessed access path remains in core (the garden storage API rake
already uses). This keeps the data-sovereignty claim while shedding the least production-credible ~13k
lines from the daemon's default attack surface — which currently includes unauthenticated storage writes
on :7185 whenever pond is inactive.

## 6. Do not break

Stage 1 found these genuinely excellent; refactoring must treat them as invariants, not raw material:

- **The bollard seal** (ARCH-0030) — zero references outside `docker/`, verified; the strongest boundary in the codebase.
- **The task supervisor** — DAG validation, panic capture, per-task tokens, `/tasks` API. Extend it to cover everything; never add a second path.
- **The thin `Moss` container** (app_state.rs, 286 lines, 22 aggregates) and the 2,483-test unit corpus, including the verified unwrap-free domain layer.
- **`StoneApi` + the `ApiResponse` envelope** — well-designed and adopted across ~15 rake command files; finish the migration, don't fork it.
- **Rake's skeleton** — manifest-driven CLI generation, the 4-level connection cascade with provenance, the `Resilient` retry, and the best-in-class discovery-failure error.
- **The offerings lifecycle** — embedded manifests with filesystem overlay, `zen-offering-*` namespace discipline, the 3-phase ceremony with journaled rollback.
- **The pond lobby split** and koi-certmesh delegation — pragmatic security bootstrap, crypto not hand-rolled.
- **The capacity governor** — incident-derived admission control; exactly the hardening pattern to replicate.
- **mongodb's check()/reconcile() single-authority design** and its Moss-delegating deployment — the template for any future orchestrator.
- **The companion SDK's Pulse/anti-corruption architecture and test harness** — frozen, but preserved as the third-party contract template.
- **The honest decision culture** — dissolution ADRs, the ORCH-0013 reverted-attempt post-mortem, self-skipping integration tests, cargo-deny posture, the cross-language URI corpus (parked with uri/, not deleted).
- **The core metaphor vocabulary** (Stone/Moss/Rake/Lantern/Pond) — stable since commit one; the project's most production-ready layer.
- **The June trajectory itself** — DEPLOY-0001/HOST-0001/STONE-0001 deployment hardening on heterogeneous real hardware is the validated white space; the consolidation above must not stall it, which is why merging that branch is workstream #1, not an afterthought.
