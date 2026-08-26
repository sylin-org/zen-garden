# CONTINUATION — read me first, then delete me

Written 2026-08-26 at the close of a massive O2 session. Self-contained for
a clean context. Verify everything against the tree — trust files over this
document.

## Project in one paragraph

Zen Garden: self-hosted service orchestration on repurposed hardware
("stones"). Services outlive machines. The PoC (`src/poc/`, branch `poc`,
tag `poc-final`) is the frozen oracle — 5-stone live fleet + this Windows
workstation. v1 is being built in `src/v1/` under an accepted constitution.

## Authority (read in order; conflicts resolve downward)

1. `docs/v1/lessons.md` — **L1–L25**, normative constraints from PoC experience
2. `docs/v1/CHARTER.md` — ACCEPTED, amended twice (topology ownership §2026-08-25,
   rehydration contract §2026-08-26)
3. `docs/v1/CODE-RULES.md` — P0–P5 engineering law; R0.5 amended for network-vs-on-media
4. `docs/v1/OFFERINGS.md` — the offerings design law (§5 compiler+format, §6 facts domain)
5. `docs/v1/inventory/` — 85-entry PoC evidence base (file:line)
6. `src/v1/DEBT.md` — D1–D13 open items; RC0 gates on zero-open
7. `src/v1/WITNESSES.md` — W1/W2/W3 + PoC bar

## Git state (branch `dev`; nothing pushed; no remote/main)

```
d4e8364a feat(v1): the Offering Directory - rehydration contract as architecture
b1fb58e8 feat(v1): full manifest corpus migration - 50 offerings converted from PoC
d60eb417 feat(v1): O2 tail - plant-through-compiler, GET placed record, facts census wired
8d5a0a21 feat(v1): OFFERINGS.md set in stone; SoC/DDD refactor of the offering stack
48582607 feat(v1): O2 slice - manifest catalog derived from disk, facts census, evaluator, converger
45e46d67 feat(v1): O1 - DockerRuntime adapter; offerings live end-to-end
a2bcad90 feat(v1): offerings O0 - the domain model, registry, and the runtime seam
36f353c1 feat(v1): daemon binary + ask/tell discovery - two stones meet in 2s (W1)
0bf54efb witness(v1): W2 - the room crosses the LAN; three stones + workstation
6e543d7f feat(v1): name the assets - moss is the service, rake reserved for the client
33b92e81 feat(v1)!: v1 owns its discovery topology - separate room by default
db9b0415 feat(v1): docker cross-compile + CI workflow
7b9eb83e wip(v1): scaffold - glossary/contract/kernel written, daemon pending
```

Other branches: `poc` (FROZEN + tag `poc-final`), `archive/pavilion`
(KEPT deliberately). Nothing pushed. Clippy `-D warnings` clean at every commit.

## Environment

- Windows, PowerShell (pwsh), cargo/rustc 1.95, edition 2024
- koi sibling at `../koi` is v1.0.0-rc.2 (PoC-only dependency; v1 is koi-free)
- Live fleet: 5 Linux stones (ips: .82=.1, .111=.1, .195=.1 — host keys cached in plink) + this workstation
- Three stones have v1 binaries deployed at `~/zen-v1/` with identities minted
  (stone-translucent-clearing@82, stone-crystalline-dune@111, stone-tranquil-pass@195);
  processes stopped, binaries remain for cross-machine witnesses
- Docker Desktop running on this machine; test containers available
- v1 topology: discovery UDP 7284 / group 239.255.42.199; HTTP TCP 7285
  (block 7284–7299 reserved); PoC fleet untouched by construction
- Shell gremlin: long compound PowerShell commands get randomly killed.
  Keep ceremonies as short discrete steps.

## Architecture snapshot (all under `src/v1/crates/moss/src/offerings/`)

| Module | Role |
|---|---|
| `model.rs` | Offering entity, ModeData sum type, WorkloadSpec (domain language), ManagedData |
| `registry.rs` | Aggregate: active+candidates pools, SnapshotStore port, ghost prevention (adopted→candidates until detected) |
| `directory.rs` | **Offering Directory** — one dir per offering: record.json, plan.json, configs/, volumes/. Auto-migrates legacy consolidated JSON. Volumes nested inside. |
| `events.rs` | Hash-chained audit ledger (FNV-1a stable across processes). Emitted: Placed/Stopped/Started/Resurrected/Uprooted/Healed |
| `runtime.rs` | Runtime port: place/start/stop/remove/observe/list; WorkloadSpec includes preferred_ports (port ledger as placement constraint) and configs (materialized ConfigMounts) |
| `docker.rs` | OCI adapter via bollard. zen-offering-* naming (PoC compat). Port ledger binds remembered ports exactly; configs written + mounted ro before start. Defensive cleanup of Docker bind-placeholder directory corpses. |
| `manifest.rs` | Catalog::load_dir derives from MOSS_CATALOG_DIR tree (*.offering.yaml; stem=name enforced). Parse-time validation: stem identity, arch-scoped feature rules required |
| `facts.rs` | Factsheet generations from parallel contributors (machine/cpu/ram/disk/worlds via sysinfo). Canonical bytes storage. |
| `evaluate.rs` | Pure evaluate(rules, facts) → DecisionReport. Severity: deny > fallback > place. Tri-state unknowns. Unit-suffixed paths convert to canonical bytes. |
| `compile.rs` | Manifest × facts × inputs → PlacementPlan {workload, decisions[], plan_hash, facts_generation}. Deny carries because+suggest. Input substitution across spec string leaves. |
| `converge.rs` | 30s floor sweep. missing+Running heals via stored spec (preferred_ports injected); stopped-stays-stopped; failures backoff → Degraded after 5; observed-running clears degradation; port remaps recorded honestly. |

Plus: `crates/glossary` (vocabulary/naming), `crates/contract` (wire types/fixtures),
`crates/kernel` (discovery/presence/runtime-facts/probe/responder), `crates/rake`
(thin client CLI: observe/find, attachment cascade).

## Key laws encoded

- An offering IS its directory (`~/.zen-garden/offerings/{slug}/`). If the
  directory is insufficient to reconstruct it, that's a bug — never accepted
- One node, one writer (facts schema assigns exactly one owning contributor
  per equivalence class; writing to another's node = programming error)
- Preferred ports ride ON the spec: converge/wake inject from port_map;
  adapters bind them exactly when free
- Stop-stays-stopped: converge never auto-starts what the operator rested
- Lowercase discriminators on the wire (transcribed from PoC source, not memory — L19)
- Adoption is detection, not conquest (L21/L22/L25)

## WITNESSES recorded so far

- W1: two stones meet, and ask (~2s convergence via ask/tell)
- W2: room crosses the LAN (3 Debian stones + workstation; IGMP snooping
  needs a querier cycle before forwarding new groups — budget for silence)
- W3: wipe recovery (container rm + image rmi + moss killed → restart →
  same host port bound, config mounted, Running; audit chain intact)

## OPEN THREADS (what comes next, roughly ordered)

### O2 tail (small finishing moves)
- Plant-through-catalog already works but POST body still accepts optional image;
  tighten when ready to require catalog path for named offerings
- Rake verbs `offer`, `explain`, `rest`, `wake`, `uproot` (thin-client calls into moss HTTP)
- Catalog overlay directories (`{data_dir}/manifests/sw/<category>/` overrides embedded)

### O3 — adoption detectors + borrow
- Detection DSL parsing from `.adopted:` manifest section (process/http/installed rules)
- Auto-adoption loop matching PoC Phase 1A/1B semantics (promote/demote candidates)
- Borrow registration (`POST /api/v1/stone/offerings/borrow`)
- Migrate the 8 `.adopted.yaml` manifests from PoC (ollama is richest)
- rake adopt/release/borrowed verbs

### Audit fan-out surfacing
- EventLog currently writes but nothing reads except validate() tests
- Surface recent events per offering in GET /offerings/{name}
- Stream or feed posture aggregates

### Runtime events stream (DEBT)
- bollard events filtered to container lifecycle → drive converge reactively
  instead of purely polling sweeps
- Will need Runtime trait addition: subscribe() → broadcast channel

### Graceful-goodbye witness
- Needs console ctrl_c harness (Start-Process can't send CTRL_C easily;
  GenerateConsoleCtrlEvent approach exists but untested)
- Worth doing to complete the lifecycle story

### M1 release pipeline (charter gate: "stranger installs from public artifact")
- Requires remote push decision first (no remote exists yet)
- Tag → build → sign → GitHub releases → installer script consumption

### Hardware manifests (hw/)
- dell wyse-5070 profile exists in PoC (identity/firmware/profile/bios sections)
- Inverse compatibility lists (recommended/cautions/avoids)
- Same grammar, kind: hardware; feeds placement decisions later

### Catalog enrichment opportunities
- 8 PoC `.adopted.yaml` files have rich multi-layer OS-specific detection DSLs
- Post-install healthcheck log patterns were dropped during conversion (§6.5
  failure signatures will supersede; need `post_install:` section in v1 format)
- taxonomy.dictionary.yaml (user-token → canonical mapping) — reuse for find/search
- well-known-ports.yaml remediation catalog (DNS auto-fix systemd-resolved etc.)

## Conventions & gotchas

- `${VAR:-default}` env patterns → declared `inputs:` entries (ask/default/secret);
  resolved at compile via string substitution over spec leaves
- Compatibility rule operands actually used by the corpus:
  ram.total.mb(77), architecture(29), ai.runtime(16), cpu.pattern(5),
  gpu.vram.total.mb(5), cpu.features(4), gpu(1), os.family(1)
- Volumes are ALWAYS named-only in v1 format; bind mounts go under advanced.binds
  with provenance comments
- guidance templating vars: name, server-name, static_ip, port.<role>, inputs.*
- FQN `::` separator slug-safe in directory names (colons → underscores)
- `gen` is a RESERVED keyword in edition 2024 — never use as identifier
- tokio interval fires immediately on first tick — consume once before loops
- Windows SO_REUSEADDR works for same-host sharing; Unix needs SO_REUSEPORT (D8)
- rg+PS quoting through two hops breaks easily — use include_str!/Write tool instead

## Key file locations (quick reference)

| What | Where |
|---|---|
| Offerings design | docs/v1/OFFERINGS.md |
| **ADR: offering directory** | docs/v1/ADR-0001-offering-directory.md |
| Lessons | docs/v1/lessons.md (L1–L25) |
| Code rules | docs/v1/CODE-RULES.md |
| Charter | docs/v1/CHARTER.md |
| Debt register | src/v1/DEBT.md |
| Witnesses | src/v1/WITNESSES.md (W1/W2/W3 recorded) |
| Cargo workspace | src/v1/Cargo.toml |
| Moss binary | src/v1/crates/moss/ |
| Rake CLI | src/v1/crates/rake/ |
| Kernel | src/v1/crates/kernel/ |
| Contract | src/v1/crates/contract/ |
| Glossary | src/v1/crates/glossary/ |
| Converted manifests | src/v1/catalog/sw/{category}/{name}.offering.yaml |
| CI workflow | .github/workflows/v1.yml |
| Docker builder | installer/v1/Dockerfile.linux-x64 |
| Deployed binaries | dist/v1/linux-x64/{moss,rake} |
| Offering directories | ~/.zen-garden/offerings/{name}/{record,plan,events}.{json,jsonl} + configs/ + volumes/ |
| Tending cache | ~/.zen-garden/.tending |
| Identity | ~/.zen-garden/identity.json |

## Quality gates enforced automatically

- clippy all=warn → -D warnings pipeline catches everything
- unwrap_used=deny, expect_used=deny, panic=deny (allow in test modules)
- undocumented_unsafe_blocks=deny
- todo=warn (DEBT.md is the escape valve)
- Every commit expected clean; historical rule-bending documented inline
