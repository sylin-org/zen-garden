---
audience: [maintainer]
doc_type: assessment
status: current
date: 2026-06-11
---

> Part of the [June 2026 project assessment](README.md). This is the consolidated, actionable register of
> everything the assessment recommends deleting, parking, consolidating, fixing, or deciding. Every
> **Verified** entry was adversarially re-derived against the tree by an independent pass (consumer greps,
> line counts, git archaeology); confidence reflects that verification, not the original reader's claim.

# Shed Register

Sequencing context lives in [architecture.md](architecture.md) §4 (do #1 "gate the tree" before bulk
deletion; git history preserves everything).

## 1. Delete — verified dead or self-described stubs (~16k lines, zero behavior change)

| Item | Size | Verification | Why |
|---|---|---|---|
| `src/moss/src/infra/cloud_filter/` | 2,475 ln / 6 files | **Verified**: unlinked from build since commit 9fcb49db (PAVILION-0001); uncompilable (deps removed) | Migrated to pavilion; pure dead weight misleading readers about moss's scope |
| `src/moss/src/domain/cloud_drive.rs` | 332 ln | **Verified**: compiles, but sole consumer is the dead cloud_filter code | Consumer left in PAVILION-0001; pavilion's provider reimplemented the logic |
| garden-common `jobs/` | 1,090 ln | **Verified**: zero consumers across 17 crates; moss has its own `Jobs` aggregate | Abandoned architecture wave (ARCH-0003 era) |
| garden-common `events/` | 731 ln | **Verified**: zero external consumers; moss and lantern each have their own EventBus | Same |
| garden-common `stone.rs` | 202 ln | **Verified**: zero consumers — the "canonical value objects" code-standards.md cites are used by nobody; moss's `Current` is a separate, prescribed design | Aspirational canon that lost to reality; update code-standards.md references when deleting |
| garden-common `client/api.rs` (`GardenHttpClient`) | 160 ln | **Verified**: zero consumers incl. the GardenApiResponse alias; also listed in `.agentic/reference/utilities.md` (fix that too) | Superseded by StoneApi |
| garden-common `errors.rs` | 2 ln | **Verified**: "will be populated in Phase 3/4" stub from the initial commit | Phases shipped; stub remained |
| `src/orchestrators/{postgresql,valkey,weaviate}` | 451/448/404 ln | **Verified**: `main()` logs a placeholder and exits; only consumers of the cluster primitives | ORCH-0012 validation scaffolds; remove from build-orchestrators.ps1 (which can publish them to Docker Hub as if real) |
| orchestrator-common cluster primitives | 1,359 ln / 7 files | **Verified**: consumed only by the three scaffolds; mongodb and ollama use zero of it | Abstraction the flagship consumer declined |
| `tests/` directory | 10 files | **Verified**: compose builds a deleted path, pre-rename ports, nonexistent Dockerfile; README falsely claims GitHub Actions CI | Dead since January; actively misleads contributors |
| Root scratch artifacts | 7 files | build-warnings.txt, build-output.txt, check_doc.rs (not valid Rust), test_if_addrs.rs, test-discovery.ps1, test-discovery-direct.ps1, test-hw-detection.ps1 | Author scratchpad in the repo front door |
| Rake dead surface | ~ hundreds ln | LiftCommand/PlaceCommand/InviteCommand + presence.rs (unrouted), `ceremony`/`template` coming-soon stubs, `election` (self-described test cmd) hidden, stale `cmd::` constants (TAKE_ROOT, MAKE, BROWSE), unused OutputWriter (or make it the only pipeline) | The 36-command directory should be the contract |
| Moss stubs | ~700 ln | TimerListener (617 ln, no callback, burns a task slot every boot), NoAuth (never wired), "(testing)" election route, `/api/v1/helpers/json-transform`, service_manager.rs tombstone (13 ln) | Self-described dead or test scaffolding in the production router |
| `installer/publish.ps1` | 1 file | **Verified**: passes parameters deploy.ps1 doesn't accept (broken since February) | Duplicates build-all.ps1; three entry points for one flow |
| Stale branches | 13 local | 6 `arch-0005/*` (work landed via PR #3 in March), 7 `worktree-agent-*` | After merging `fix/snapshot-scheduler-disposal` into dev |
| Companion-SDK unwired surface | small | DeliveryPolicy::LatestEvery/Debounced ship as `All`; `required_dependencies` unenforced | Declare the SDK feature-complete; strip or implement declared surface |

## 2. Park — alive but not earning active-tree residency (~49k lines leave the active tree)

| Item | Size | Action |
|---|---|---|
| `src/orchestrators/ai` | 41.9k Rust + 4.7k TS | Archive branch or sibling repo + a succession ADR (see Decide #1). Operationally dormant since 2026-04-12 (absent from build scripts, no Hub push, no gateway registration) — but it is the *newer* design with the best test density (396 annotations); harvest its EventBus/GPU-claim designs as references. If kept instead: fix the 3 unawaited `events.publish` calls in flow_executor first (verified real bug) |
| `src/pavilion` | 6.8k Rust + 4.6k TS | Own repo or explicit freeze note. Windows-only, flagship Cloud Filter upload admittedly not working end-to-end (PAVILION-0002), idle since May 6 |
| garden-common `uri/` | 838 ln + test corpus | Park with its cross-language conformance corpus — or wire it end-to-end per [strategy.md](strategy.md) opportunity #3. Do not carry it dead while the README headlines it |
| linux-x86 (i686) build leg | 1 Dockerfile + script lanes | Freeze; 32-bit machines run Docker images poorly anyway |
| `installer/branding/` | GRUB/ISOLINUX/GTK themes | Freeze; boot-splash polish belongs to a 1.0; the GTK part is acknowledged unimplemented |

## 3. Consolidate — N implementations → 1

| Item | Today | Target |
|---|---|---|
| Backup subsystems | **Verified**: harvest 1,412 + nurturing 2,732 + ORCH-0039 snapshots 3,242 ≈ 7,400 ln; **three** HTTP route families; two capture engines + nurturing wrapping harvest; two scheduling mechanisms | ORCH-0039 snapshots absorb both (newest, most principled, post-incident hardened); one route family, one scheduler — under the supervisor |
| Moss route table | **Verified**: configure() + configure_public(), ~266 verbatim-duplicated lines; drift bug already exists (`GET .../banks/{moniker}/seeds` public-only, contradicting the file's own doc) | One declarative route list with a public/privileged tag per route |
| garden-common → moss | **Verified**: ~13.6k lines consumed only by moss (compatibility/, console/, resources/, detection/, most of manifests/, infra/{timer,archive,debounce,process,platform}, templates.rs, traits/, persistence/, platform_runtime.rs). NOT moss-only (keep shared): manifests::generate+validation (rake), infra/network.rs (lantern + orchestrators), infra/koi_client.rs (lantern) | Move into moss; interim feature flags (`client`/`system`/`transport`) immediately |
| Duplicate structs | **Verified**: 21 names duplicated directly moss↔rake (the literal rule violation — StorageOverview, HarvestManifest, ListBucketResult, S3Object, PlacementRecommendation…), plus ~12 same-concept common↔moss/rake duplicates (EventBus, Job/Jobs, CompanionSummary, DiscoveredStone, ApiResponse, TransformSpec/FieldMappings/TemplateInfo are verbatim copies). moss's `Current`/`Stone` is prescribed by code-standards §5 — not a violation | Adopt the common type or delete the common copy, case by case; wire-contract duplicates first |
| Background tasks | **Verified**: 32 always-on + 4 conditional supervised; ≥8 long-lived tasks outside the supervisor (snapshot scheduler — JoinHandle discarded at run.rs:183 — DockerMonitor, network monitor, discovery UDP listener, 3 event-bus listeners, transport tap); 77 raw spawn sites | Everything long-lived under the ARCH-0015 supervisor; registry's own contract says "no second path" |
| Discovery stacks | moss-internal (embedded Koi) + garden-discovery + orchestrator-common's own discovery + 2 broken garden-discovery sync wrappers (background Lantern path always caches None; blocking thread join inside async) | One stack: garden-discovery (+transport) with the p2p singleton relocated out of garden-common; orchestrators fold onto it (DISC-0001 finished) |
| Rake storage vocabulary | `store` / `storage` / `backup` three top-level families; homonyms wake/release/remove/refresh across domains | One `storage` family with subverbs; resolve homonyms in the same pass as the docs regeneration |
| Rake output/exit | 4 formatting systems, ~1,247 raw println!s, 3 of 36 commands honor `-o json`, pond failures exit 0, find exits mid-function | One pipeline, one JSON envelope at dispatch, one top-level error→exit mapper |
| StoneApi migration | 12 rake files still on raw ctx.client HTTP (list.rs doesn't even check status before parsing) | Finish ARCH-0012 |
| `find --ensure` saga | 500+ ln CLI-side orchestration parsing job-ids out of prose messages | One moss endpoint returning structured job_id |
| Embedded SPAs | 4 hand-maintained single-file apps (184KB inline JS) in the daemon | Keep portrait as the stone landing page; consolidate/relocate the rest |
| companion-usb | 971-ln crate, single consumer (firefly) | Fold into firefly or document why separate |
| domain/ root files | 37 loose files, 12.9k lines, half-migrated since ARCH-0017 | Finish the migration or formally close the epic and document the final shape |
| bootstrap/run.rs | 1,774 ln, 6+ unrelated concerns, most-fixed file in history (15 fix commits) | Split: build_state phases, Windows DNS/registry → infra/platform, unit-file healing → infra/installer |
| API megafiles | s3_gateway.rs 2,352 / storage.rs 1,993 / updates.rs 1,283 | Carve along sub-resources, mirroring the garden_storage/ split already done |

## 4. Fix or retract — credibility debt (docs that contradict the shipped system)

| Item | Detail |
|---|---|
| README Getting Started | **Verified fictional**: `zen-garden/stone:latest` + `ANNOUNCE_SERVICE` exist nowhere else; rewrite against a real path or delete |
| README headline URI | Parser-only (sole consumer: its own test corpus); wire it or demote to roadmap |
| first-stone.md / troubleshooting.md | **Verified**: 15+ concrete mismatches (nonexistent commands/flags/paths/orgs/parameters; false "SSH disabled by default" claim vs preseed's ssh+stone/stone+NOPASSWD) — pull now, regenerate from the command manifest later |
| Rake help examples | **Verified**: 25 of 130 manifest examples rejected by the parser (23 natural-language relics, 2 dead flags); add the parse-all-examples CI test |
| Weather vocabulary | **Verified absent** from src/ while joy-of-understanding.md claims Implemented; implement (cheap mapping over existing health states) or retract |
| Nourishment rename | Simultaneously deprecated (glossary) and shipped (README, `rake nourish`); finish one direction |
| ADR hygiene | Status sweep: implemented-but-"proposed" (ORCH-0039, STORAGE-0020, ORCH-0013/0028–0030…), unpropagated supersessions (8+), index claims 96 of 182 files — replace hand-maintained index with a generated listing |
| ADR taxonomy | 35 prefixes for a 4.5-month project; collapse to ~8–10 domains; fold the 12 singleton prefixes |
| ARCH-0017 epic books | Archive the 20 same-day completion-record ADRs as a set; keep the epic ADR + postmortem + the pattern spec |
| docs/proposals/ | 40 untriaged entries (aws-bridge, federation, patent-analysis…) inflating apparent scope; promote/archive/delete each |
| `.agentic` bootstrap docs | **Verified inaccuracies**: GardenHttpClient listed, wrong TUI path, wrong test command (`--package moss` → `garden-moss`), module map omits 5 src/ dirs + 5 orchestrators — these actively corrupt AI-assisted sessions |
| code-standards.md §6 | Standard contradicts accepted ADR ARCH-0035 (162 of 186 handlers take full State); amend the standard or run the migration — either, but not both documents disagreeing |
| LANTERN-0001 | Retire the unbuilt pillars (SQLite persistence never implemented; election shipped one day in January then deleted; HA never); rename lantern to what it is — the garden dashboard (resolve/SSE/heartbeats stay) |
| Philosophy essays | Strip volatile facts (offering counts 9 vs 31 vs 51 across three docs; superseded broadcast-cascade description); write or retire the thrice-promised "State" pillar essay (the "stateless daemon" claim is obsolete); replace the nine fictional "Workshop Panel" experts with honest authorship |
| Glossary | Split garden vocabulary (user-facing) from the DDD contributor lexicon; fix the Lantern port contradiction |
| companion-overview.md | Describes a planned port-7189 companion that shipped as firefly adapters two months ago |
| Spec freshness | 18 unstamped specs; security.md "canonical" but last verified 2026-01-19; api-v1.md 2026-03-25 |

## 5. Decide — judgment calls the register can't make alone

| # | Decision | Evidence summary |
|---|---|---|
| 1 | **ai vs ollama succession** | The repo carries 57k lines (~20% of all Rust) in indecision. ollama = deployed, published, documented, gateway-registered; ai = newer design, better-tested, operationally unwired. Either answer beats no answer; the strategy doc leans ollama-now, harvest-ai-designs |
| 2 | **Storage demarcation** | Keep durability (banks/replication/snapshots) core; extract NAS surfaces (S3/WebDAV/sets/garden-FS) behind a feature gate — argued both ways in [architecture.md](architecture.md) §5 |
| 3 | **HTTP exposure model** | Pond-by-default at first boot vs a minimal deploy/admin token when pond is inactive. **Verified now**: deploy endpoint unauthenticated in both route sets; reboot/shutdown/offering-delete/storage-writes open on :7185 when pond inactive; `changeme` default passphrase |
| 4 | **Moss's AI-capability surface (~6.5k ln)** | Capabilities executor/mirroring/placement/fitness/tool-registry exist to serve the orchestrator; move to the orchestrator side or keep as the platform's placement substrate |
| 5 | **Orchestrator deployment model** | Make orchestrators Moss offerings (zen-offering-* lifecycle) or label them operator-managed extras; today's .bat-launched story contradicts the project's own value proposition |
| 6 | **Probe's future** | The only integration-test surface, stale since Mar 22 while the API evolved; revive on a schedule against a dev garden (and split its 3,223-ln nurturing.rs per the project's own 800-line rule) or accept manual-only verification |
