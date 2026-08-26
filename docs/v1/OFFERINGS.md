# Offerings — the design of v1's core domain

**Status:** Law · 2026-08-26. Subordinate to `lessons.md`, `CHARTER.md`,
`CODE-RULES.md`. This document is the single source for offering semantics,
the manifest format, and the facts/evaluation model; code cites it, never
re-states it.

## Provenance

Harvested from the PoC (branch `poc`, 2026-08-25): offering model
(`poc/common/src/types/offering.rs`), aggregate + store
(`poc/moss/src/domain/offerings/`), lifecycle/reconcile/adoption
(`service_lifecycle.rs`, `offering_reconciliation.rs`, `auto_adoption.rs`),
ceremony (`domain/ceremony/types.rs`), Docker touchpoints (`moss/src/docker/*`),
wire entry (`common/src/types/discovery.rs:72-91`), manifest system
(`poc/common/src/manifests/`, `poc/moss/embedded/manifests/**` — 51 software
families surveyed field-by-field 2026-08-26). `poc:` line refs below.

## 1. What an offering is

An **offering** is a named unit of work placed on a stone — the thing the
garden is *for*. Three modes (inherited verbatim from PoC):

| Mode | Meaning | Who drives its lifecycle |
|---|---|---|
| **managed** | Planted by the garden from a catalog manifest | The stone (deploy/stop/wake/nourish) |
| **adopted** | Found already running on the host (e.g., hand-installed ollama) | Nobody, unless control level says so; the stone watches |
| **borrowed** | A service living elsewhere entirely, registered here for discovery | Nobody; a pointer with a health probe |

Identity: GUIDv7 `offering_id` (survives renames) + FQN name
(`mongodb`, `ollama::adopted`). Statuses: `installing → running ↔ stopped`,
plus `cordoned`, `maintenance`, `degraded`. Health vocabulary per glossary.
Wire strings lowercase, byte-compatible with poc constants.

## 2. The registry (hot cache + store port)

One aggregate per stone: **active** pool + **candidates** pool behind one
lock; every mutation funnels facts → persist (via the store PORT) → broadcast.
Persistence: **one directory per offering** (see [ADR-0001](decisions/ADR-0001-offering-directory.md)) at
`~/.zen-garden/offerings/{slug}/` — record.json, plan.json, events.jsonl,
configs/, volumes/. Atomic temp+rename per file.

**Ghost prevention (keep exactly):** adopted offerings load into
*candidates*, invisible until their detector confirms them again
(poc run.rs:806-858). A hand-stopped service must not haunt the garden.

**Port honesty:** stored `port_map` reflects observed reality; until the
ledger lands (O2), wake re-derives and records remaps rather than lying.

## 3. Lifecycle rules worth keeping (decided, do not re-litigate)

1. Desired-state converge with bounded patience: backoff 30/60/120/240/480s
   ×5 → `degraded` (poc offering_reconciliation.rs:27-32).
2. **Stopped stays stopped**: converge never auto-starts what the operator
   rested (poc :202-206).
3. Events inside: runtime event streams drive status; sweeps are floors (L18).
4. Ceremony ≠ install: install is a job; ceremonies are journaled multi-phase
   rituals — deferred (D11).
5. Adoption is detection, not conquest: promote on confirmation, demote on
   silence; compatibility may forbid outright (poc auto_adoption Phase 1A/1B).
6. Borrow is registration only: excluded from reconcile, present in discovery.

**The rehydration contract** — an offering is fully determined by three
artifacts: its **registry entry**, its **directory on disk** ([ADR-0001](decisions/ADR-0001-offering-directory.md)), and
its **catalog manifest**. If rehydration fails for any constituent, the
placed record says which one and why — silent degradation is banned.

## 4. The runtime seam

```rust
// moss::offerings::runtime — the pluggable substrate (see source for full types)
trait Runtime { fn kind(); async place(name,&WorkloadSpec)->Placement;
                start/stop/remove; observe(name)->Option<Observed>; list(); }
```

- **The registry knows modes; runtimes know containers.** Managed offerings
  translate plans into `WorkloadSpec`s; adopted/borrowed never touch a
  Runtime.
- **Multi-runtime hosts (amended 2026-08-26):** binding is per-offering
  (`ManagedData.runtime_kind`), chosen at placement, permanent like the port
  ledger. Ops dispatch to the *remembered* world — they never guess. The
  host probes which worlds exist at boot and advertises them
  (`posture.runtimes[]`). Placement into an absent world is refused before
  any side effect; explicitly configured absence aborts startup loudly (L17).
- Podman speaks Docker's API: one OCI adapter, two connection strings. Real
  families: oci-engine / systemd / process / null.
- Container naming is adapter-internal (`zen-offering-*` kept for compat).
- Orchestrators remain guests speaking moss REST (topology GET, gateway
  lease, config PATCH) — never the runtime directly.

## 5. The offering compiler

Three stages, one spec type, no clones (PoC had three overlapping
intermediates: ServiceTemplate / CompiledOffering / ContainerSpec):

```
Manifest ──compile(facts × compat × inputs)──▶ PlacementPlan ──place(world)──▶ Reality
(handcrafted,      · compat resolved (decisions logged)                        · observed state
 versioned,        · spec finalized                                            · drift = converge()
 checked)          · guidance rendered
```

### 5.1 Manifest format (garden manifest v1)

**One `<name>.offering.yaml` per offering** (single machine-truth parse
target), optional sidecars paired by stem: `<name>.guidance.md` (human words),
`<name>.research.md` (institutional memory, never parsed). Shared catalogs:
taxonomy dictionary, well-known-ports remediation catalog, category.json.
Hardware manifests use the same grammar with `kind: hardware` plus inverse
compatibility lists (`recommends/cautions/avoids`).

Sections (presence defines supported modes — poc convention kept):

```yaml
kind: software            # software | hardware
name: mongodb             # must equal file stem
category: data
description: ...
tags: [database, document, nosql]

inputs:                   # declared install form (Runtipi/Yunohost pattern):
  admin_password:         # rendered at plant time; feeds ${refs} below.
    ask: "Admin password"
    secret: true

managed:                  # ---- placement intent ----
  world: oci              # which adapter family places it
  image: mongo:7
  ports: { default: 27017 }          # name -> CONTAINER port (single truth)
  volumes: [{ name: mongo-data, mount: /data/db }]
  env: { TZ: UTC }
  config_files:
    - mount: /etc/mongod.conf
      format: yaml
      flag: "--config /etc/mongod.conf"
      reload: restart            # restart | signal: SIGHUP
  healthcheck: { exec: [...], interval: 10s, retries: 5 }
  resources: { memory: 2g, cpus: 1.5, gpu: false }
  advanced: {}                   # passthrough bag: cap_add, shm_size, sysctls, ulimits
  tasks: {}                      # cron maintenance commands
  placement:                     # hints consumed at compile
    static_ip: preferred
    static_ip_reason: "DNS servers need stable addresses"

adopted:                  # ---- detection intent ----
  detect:
    process: { executable: ollama, args_contain: serve }
    http: { port: 11434, path: /api/tags, expect_status: 200 }
    installed: { cmd: "ollama --version", pattern: "version is" }  # dormant-vs-absent
  control: monitor               # full | monitor | announce (all shipped PoC manifests chose monitor)
  commands: { start: ..., stop: ... }   # honored only at control: full
  ports: { default: { value: 11434, remember: true } }

borrowed:
  connection: { protocol: http, uri_template: "http://{host}:{port}" }
  health: { method: http, path: / }

compatibility:            # ---- compile-time knowledge (see §6) ----
  - when: { arch: x86_64, cpu.features_lacks: avx }
    decide: fallback
    into: { image: mongo:4.4 }
    because: "MongoDB 5.0+ requires AVX"
    source: "https://mongodb.com/docs/..."     # citation travels with the rule

capabilities: {}          # RESERVED (D14) — grammar name claimed, design parked
```

Format laws:
- **Structured YAML only**; no compose-string-in-YAML. Compose import is a
  tool (`garden manifest import`), not a format feature.
- **Declared install forms** (`inputs:`) render a guided plant experience;
  `${input.name}` references resolve at plant. No `${VAR:-default}` incantations.
- Guidance templating contract (documented magic, poc-derived): variables are
  `name`, `server-name`, `static_ip`, `port.<role>` (from managed ports),
  plus any `inputs.*`. Conditional blocks allowed.
- Identity is stated once; file stem must equal `name` (validated — path is
  redundant, not authoritative).
- Every compatibility rule requires `because`; `source` strongly recommended.
  Feature rules MUST carry an arch scope (the ARM/AVX false-downgrade scar,
  encoded as a validation rule).
- **Layered catalog (2026-08-26):** a moss loads its base catalog tree
  (`MOSS_CATALOG_DIR`, default `~/.zen-garden/catalog`) plus one operator
  overlay layer (`MOSS_CATALOG_OVERLAY_DIR`, default
  `~/.zen-garden/manifests`; tree shape `{category}/{name}.offering.yaml`
  or deeper). Overlay entries OVERRIDE base entries by NAME; missing
  layers are routine, malformed manifests are skipped with warnings like
  any other. One stone's private adjustments must not fork the corpus.
- **Named installations (2026-08-26):** `{stem}:{instance}` plants a second
  copy of a catalog offering under its own name (`redis` and `redis:prod`
  coexist on one stone). Instances inherit the stem's manifest and
  category, carry their own identity/directory/decisions, draw INDEPENDENT
  addresses from the ledger (the `offering` field keeps the stem as
  provenance), and appear separately in chirps. Suffixes accept letters,
  digits, '-' and '_'; anything else refuses loudly. FQN names (`::`)
  remain adoption namespace territory.

### 5.2 Compile

`compile(manifest, facts, inputs, stone_context) -> Result<PlacementPlan, CompileError>`

PlacementPlan = `{manifest_version, workload: WorkloadSpec, guidance_rendered,
decisions: Vec<Decision>, plan_hash}`. Decisions record EVERY choice —
compatibility outcomes, fallback swaps, port assignments, input applications —
each with `because` and `source` where applicable. `plan_hash` makes drift
detectable: reality ≠ plan ⇒ converge. Stages fail loudly with their names:
*manifest-load* → *compile* → *place*.

### 5.3 The placed record (the delightful reference)

`GET /api/v1/stone/offerings/{name}` returns the placed record with its plan
attached: what it is, which image and WHY (decision log), named ports in
human form ("http → 52324"), rendered guidance, health, provenance hash.
Self-explaining deployments. `rake explain <offering>` renders it.

## 6. The facts domain

### 6.1 Contributors and generations

At boot, **contributors** fire in parallel — one per concern (cpu, ram,
disks, gpu, os/sysctls, runtime-worlds). Each owns exactly its nodes
(single-writer made structural) and reports into the **Factsheet**: an
immutable, in-memory generation of the complete `StoneFacts` snapshot plus
provenance (who, when, how long).

- **Nobody probes directly, ever** — readers consume cheap snapshots
  (kills hidden probe duplication by construction, R1.2).
- Refresh produces a NEW generation swapped atomically; evaluators record
  the generation they decided against. Refresh triggers: invalidation
  events, timer floor (L18-compliant).
- Contributor failure = `unknown` nodes + visible warning; the census still
  completes. L17 applies to the census step, not each concern.
- Fixtures are recorded generations — compat-rule regression tests replay
  captured machines (the Wyse 5070's real no-AVX census evaluates mongodb's
  rules without hardware).

### 6.2 Fact grammar — nouns first, units as projections

Canonical storage in base units; dotted unit aliases are DECLARED and
generated (one source of truth, zero conversion bugs):

```text
machine.architecture        = x86_64
cpu.model / cpu.cores / cpu.features = [sse4_2]
ram.total.bytes = ...       ram.total.mb = 4096        ← generated alias
ram.available.bytes         = ...
disk.space.total.bytes / disk.space.free.bytes / .free.mb / .free.gb
disk.format = ext4|ntfs...  disk.kind = nvme|ssd|hdd    disk.standard = sata|nvme|usb
gpu.present / gpu.count / gpu.vram.max.bytes(.mb)
ai.runtime = cuda|rocm|cpu
os.family                   sysctl.vm.max_map_count = 262144
runtime.docker.present = true                          # L25 probing feeds the tree
```

Nouns read as sentences about the world (*disk space free*), qualifiers
after. Qualitative siblings ride along (`format`, `kind`, `standard`).

### 6.3 One node, one writer

Each equivalence class has exactly ONE owning contributor. Writing to a node
you don't own is a programming error — rejected loudly, asserted in tests.
Equivalences between nodes are DECLARED invariants
(`gpu.present ⇐ gpu.count > 0`; unit aliases) computed to fill gaps only,
never overwriting observations. If two sources disagree, the schema is wrong
— fix the schema, not the readers. (This deletes the PoC's dual-detection
precedence machinery by construction.)

### 6.4 Evaluation — pure, severe, transparent

`evaluate(rules, facts_generation) -> DecisionReport`. Rules address fact
paths with human-unit sugar (`ram.total.gb >= 0.5`); unknown alias or unknown
path = validation error at load. Semantics:

- **Order-independent severity**: ALL rules evaluate; strongest wins —
  `deny > fallback > place`. A lint flags denies shadowed by earlier
  fallbacks. No positional fragility.
- **Tri-state matches**: matched / no-match / **unknown** (fact absent or
  unprobeable). Unknown never folds silently into no-match; the report says so.
- **DecisionReport records everything**, including passing rules, with
  observed-vs-derived provenance per matched fact.
- **Plan mode**: compile + evaluate + port scan without touching reality —
  `rake offer mongodb --plan`.

Fact checks the PoC lacked (all cheap pre-flight): sysctl gates
(`vm.max_map_count` — poc called it impossible), disk headroom at the volume
root, cgroup-aware `ram.available`, registry-manifest arch verification
before pull (SQL Server's arm64 scar), OOMKilled/exit-code taxonomy.

### 6.5 Post-place: failure signatures, not one-shot scans

Manifest-declared log patterns PLUS generic signals (restart-count window,
OOMKilled, healthcheck flip count) watched continuously off the runtime
event stream. Signature hit → auto-`degraded` with evidence attached to the
placed record. Evaluator observes itself: per-rule counters and probe
timings surface in posture (B3).

## 7. Sequencing

| Slice | Delivers |
|---|---|
| **O0** ✅ 2026-08-25 | Vocabulary, model + registry (ghost prevention) + store port, Runtime seam + null adapter, wire `ports` parity |
| **O1** ✅ 2026-08-26 | DockerRuntime + stone-category routes (plant/rest/wake/uproot); nginx planted→served→rested→woken→resurrected→uprooted, witnessed |
| **O2** | This document's compiler: manifest parser + facts census (contributors/generations) + PlacementPlan + decision log + Converger + port ledger |
| **O3** | Adoption detectors, borrow registration, full stone-category API, rake verbs (offer/explain) |
| later | Ceremonies (D11), orchestration roles (D12), borrow vaulting (D13), capabilities DSL (D14), podman/systemd adapters beyond oci (D10 tail) |

Open debts live in `DEBT.md`.
