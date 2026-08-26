# ADR-0001 — The Offering Directory as the unit of deployment

**Status:** Accepted · 2026-08-26
**Supersedes:** the consolidated `offerings.json` store (pre-O2 persistence)
**Referenced by:** OFFERINGS.md §5.3–5.4, DEBT D1–D13, WITNESSES W3

## Context

Rehydration — rebuilding every deployed workload after total Docker loss —
was the founding promise the PoC witnessed ("wipe recovery": containers rm'd,
image rmi'd, registry intact → moss rebuilt with ports preserved). But the
PoC achieved it via scattered state: one consolidated JSON registry, port
ledger in `{data_dir}`, config extraction to volumes, audit chain elsewhere.
When any piece drifted out of sync, rehydration silently degraded or broke.

v1 needed a structure where the guarantee holds **by construction**, not by
careful coordination of separate artifacts.

## Decision

**Every deployed offering owns one self-contained directory that fully
determines it.**

```
~/.zen-garden/offerings/{slug}/
├── record.json      ← identity, status, mode_data (spec, runtime_kind, port_map)
├── plan.json        ← compiled PlacementPlan (decisions[], workload, hash, facts gen)
├── events.jsonl     ← append-only hash-chained audit trail (FNV-1a stable)
├── configs/         ← materialized config files (mounted read-only into workloads)
│   └── mongod.conf
└── volumes/         ← data volumes nested INSIDE (the heavy stuff)
    └── mongo-data/
```

The **directory IS the source of truth alongside the registry**. The
consolidated index becomes derived/cached — the directory is primary.
An offering's backup = `tar` its directory. Its migration = copy it + call
place(). Its deletion = remove the dir (volumes included if desired).

## Law encoded

> An offering is constituted by its directory. If the directory is
> insufficient to reconstruct the offering, that is a bug — never an
> accepted limitation.

Three supporting laws:

1. **Data outlives registration.** Unregistering an offering removes only
   its JSON metadata; volumes and configs remain on disk until explicitly
   deleted. A re-registration on a fresh stone picks up whatever data was
   left behind.
2. **Preferred ports are placement constraints, not observations.**
   Remembered host-port bindings ride ON the spec (`spec.preferred_ports`)
   so converge/wake inject them BEFORE placement — Docker binds them exactly
   when free, falls back to ephemeral assignment otherwise, and the remap is
   recorded honestly in the placed record.
3. **Audit trails are tamper-evident.** Each event commits to
   `seq|prev_hash|kind|details` via FNV-1a 64 (stable across processes,
   unlike Rust's DefaultHasher). `validate()` RECOMPUTES each hash from its
   fields — any byte changed breaks every later link visibly.

## Alternatives considered

### A. Consolidated JSON registry (status quo pre-O2)

One file holding all offerings in flat arrays (active + candidates).

**Pros:** Simple, one parse target, atomic whole-write.
**Cons:** Offering knowledge smeared across separate artifacts (registry,
volumes root, configs implied-but-unwritten, no audit); backup/migrate =
"figure out which files matter"; individual offerings can't be backed up
or inspected independently. Direct cause of silent-degradation risk.

### B. Database-backed registry (SQLite / sled / postgrey)

Proper ACID semantics, queryable, concurrent-safe.

**Cons:** Violates P0's "no new dependencies without reason" (Rust SQLite
adds ~500 KB build), requires migration tooling forever, adds a server-like
runtime component on what should be a stone binary operating on plain files.
Also incompatible with the "copy the directory to migrate" operation that
makes self-hosting feel simple.

### C. Registry-as-code (Kubernetes-style declarative YAML trees)

Every offering declared as garden.yaml, controllers reconcile.

**Pros:** Declarative-first aligns with M3 charter goals. **Cons:** Overkill
at O2 scope; the declarative layer arrives naturally at M3, but the DIRECTORY
structure remains underneath regardless. Not actually a competing alternative
— more a future layer above it.

### D. Split per-concern directories (configs here, records there)

Same information but spread across multiple roots.

**Cons:** Re-introduces the original problem — scattered artifacts requiring
coordination. Rejected immediately once we framed "an offering is a directory"
as the goal.

## Consequences

### Positive

- **Self-contained units**: copying `offerings/{name}/` captures everything —
  spec, provenance (plan + decisions), history (events.jsonl), configs
  (rendered content), data (nested volumes).
- **Wipe recovery tested**: W3 witnessed total Docker destruction → restart →
  same host port bound, config mounted, Running. The events ledger narrated
  the full life across the gap.
- **Structural single-writer**: DirectoryStore persists into its own scope;
  FileSnapshotStore remains as a test/memory adapter behind the same port.
- **Audit integrity**: hash chains surface tampering; PoC had raw JSONL
  without cryptographic linkage.
- **Placement constraints flow through the model**: preferred ports and
  materialized ConfigMounts are WorkloadSpec fields — adapters bind/mount
  them mechanically, domain never touches fs mechanics.

### Negative / costs

- One-time boot migration from legacy consolidated JSON
  (auto-migrates, renames to `.migrated`; old code can't read the new layout)
- FQN colons (`ollama::adopted`) slugified to underscores for directory names;
  offering_id inside remains the true key
- Individual offerings lose atomic multi-write transactionality (each
  directory is written independently) — acceptable because single-writer
  service sequencing makes partial-failure recovery trivial (re-run converge)
- Consolidated queries across offerings require scanning directories;
  fine at garden scale (<100 offerings typically)

### Neutral

- Consolidated `offerings.json` still readable by the old code for debugging;
  auto-renamed `.migrated` after first directory-store load
- Guidance .md and research .md stay sidecar-only (not machine-truth),
  colocated next to their offering dirs when shipped in catalog packages
- Hardware profiles (`hw/dell/wyse-5070.*`) use the same grammar but have
  different content shape — unaffected by this ADR

## References

- Implementation: `src/v1/crates/moss/src/offerings/directory.rs`,
  `events.rs`, `service.rs` (audit callsites), `docker.rs` (config staging +
  bind mounting), `compile.rs` (materialization)
- Witnesses: W3 — wipe recovery demonstrated end-to-end on Windows workstation
- OFFERINGS.md §5.3 (placed record delight), §5.4 (rehydration contract)
- PoC provenance: `poc/moss/src/domain/offerings/store.rs`, 
  `poc/common/src/constants/paths.rs` (data_dir/config_dir split that made
  the PoC's life harder than this design does)
