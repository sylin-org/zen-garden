# Offerings — the design of v1's core domain

**Status:** Design · 2026-08-25. Subordinate to `lessons.md`, `CHARTER.md`,
`CODE-RULES.md`. This document is the single source for offering semantics;
code cites it, never re-states it.

## Provenance

Harvested from the PoC on 2026-08-25 (branch `poc`): offering model
(`poc/common/src/types/offering.rs`), aggregate + store
(`poc/moss/src/domain/offerings/`), lifecycle/reconcile/adoption
(`service_lifecycle.rs`, `offering_reconciliation.rs`, `auto_adoption.rs`),
ceremony (`domain/ceremony/types.rs`), Docker touchpoints (`moss/src/docker/*`),
wire entry (`common/src/types/discovery.rs:72-91`). Line references below use
`poc:` prefixes.

## 1. What an offering is

An **offering** is a named unit of work placed on a stone — the thing the
garden is *for*. Three modes (inherited verbatim from PoC):

| Mode | Meaning | Who drives its lifecycle |
|---|---|---|
| **managed** | Planted by the garden from a catalog manifest | The stone (deploy/stop/wake/nourish) |
| **adopted** | Found already running on the host (e.g., hand-installed ollama) | Nobody, unless control level says so; the stone watches |
| **borrowed** | A service that lives somewhere else entirely, registered here for discovery | Nobody; it is a pointer with a health probe |

Mode-specific data rides in a tagged enum (`mode: managed|adopted|borrowed`),
poc:238-249. Adopted carries detection rules and a **control level**
(full / monitor / announce; default monitor, poc:40-48). Borrowed carries a
connection URL and health method.

Identity: GUIDv7 `offering_id` (survives renames) + FQN name
(`mongodb`, `ollama::adopted`). Statuses: `installing → running ↔ stopped`,
plus `cordoned` (scheduling fence), `maintenance` (nourish in flight),
`degraded` (reconcile exhausted / connectivity lost). Health:
`healthy | degraded | offline`. Wire strings are lowercase, byte-compatible
with poc constants (constants/mod.rs:331-352).

## 2. The registry (hot cache + store)

One aggregate per stone: **active** pool + **candidates** pool, behind one
lock; every write funnels persist → emit (poc aggregate.rs:476-502).
Persistence: JSON at `{config_dir}/offerings.json`, atomic temp+rename
(poc persistence.rs:22,232).

**Ghost prevention (keep exactly):** on load, adopted offerings go to
*candidates*, not active — invisible until their detector confirms them
again (poc run.rs:806-858). A hand-stopped service must not haunt the garden.

**Port ledger (keep):** once an offering's container port is published on a
host port, that mapping is remembered and reused across redeploys
(PORT-0001, poc port_ledger.rs); reconcile restores stored port maps so the
witnessed "ports preserved" recovery stays true.

Chirps carry offerings as `ServiceEntry[]` (contract::chirp), including the
defaulted `ports` map (R0.5, fixed 2026-08-25).

## 3. Lifecycle rules worth keeping (decided, do not re-litigate)

1. **Desired-state reconcile with bounded patience**: missing actual +
desired running → rebuild; backoff 30/60/120/240/480s ×5, then `degraded`
(poc offering_reconciliation.rs:27-32).
2. **Stopped stays stopped**: reconcile never auto-starts what the operator
rested (poc :202-206).
3. **Events inside**: Docker/runtime event streams drive status immediately;
the 30s sweep is only the floor (L18 satisfied by design, poc docker_events.rs).
4. **Ceremony ≠ install**: install is a background job; "ceremony" names the
multi-phase rituals (nourish upgrade, vacate, replant, store) with journals
and rollback (poc ceremony/types.rs:14-34). v1 defers ceremonies (D11).
5. **Adoption is detection, not conquest**: candidates promote on detector
success, demote on silence (poc auto_adoption.rs Phase 1A/1B); compatibility
rules can forbid adoption outright.
6. **Borrow is registration only**: no orchestration, excluded from
reconcile, participates in discovery (poc adoption.rs:378+).

## 4. The runtime seam (new in v1 — the point of this design)

The PoC's aggregate/lifecycle layers were already runtime-blind; everything
below them assumed Docker (full touchpoint list: harvest §8 — bollard client,
`zen-offering-*` naming, port remap, events stream, exec, GPU, restart
policies). v1 names that below-layer as a port:

```rust
// moss::runtime — the pluggable execution substrate
pub struct WorkloadSpec { image, named_ports, volumes, env,
                          config_files, health_probe, restart_policy, resources }
pub enum RuntimeEvent { Deployed, Started, Stopped, Died, HealthFlipped, Removed }
#[async_trait::async_trait]
pub trait Runtime: Send + Sync {
    fn kind(&self) -> &'static str;                       // "docker" | "podman" | "null" | ...
    async fn deploy(&self, name: &str, spec: &WorkloadSpec) -> Result<DeployOutcome>;
    async fn start(&self, name: &str) -> Result<()>;
    async fn stop(&self, name: &str) -> Result<()>;
    async fn remove(&self, name: &str, preserve_volumes: bool) -> Result<()>;
    async fn inspect(&self, name: &str) -> Option<RunningWorkload>;
    async fn list(&self) -> Vec<RunningWorkload>;
    fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent>;
}
```

Rules:
- **The registry knows modes; the runtime knows containers.** Managed
  offerings translate desired state into `WorkloadSpec`s; adopted/borrowed
  never touch a Runtime.
- Adapters are selected by configuration (`MOSS_RUNTIME=docker|podman|null`),
  registered in one place (P1: one registry). Selection failure aborts
  startup loudly (L17).
- Container naming is adapter-internal (Docker keeps `zen-offering-*`
  compat; podman may choose its own); the registry stores `offering_id`,
  never names.
- Orchestrators remain *guests*, not adapters (poc contract preserved):
  they speak moss REST (topology GET, gateway register with lease TTL,
  config PATCH) and never touch the runtime directly.

## 5. Sequencing

| Slice | Delivers |
|---|---|
| **O0** (this commit) | Vocabulary (glossary), model + registry + store + ghost-prevention + tests, Runtime trait + null adapter, wire `ports` parity |
| **O1** ✅ (2026-08-26) Docker adapter + stone-category routes (plant/rest/wake/uproot) — nginx planted, served, rested, woken, resurrected after behind-the-back destruction, uprooted. Runtime events stream deferred (see D10 note) |
| O2 | Manifest catalog + install jobs + reconcile loop w/ backoff + port ledger |
| O3 | Adoption detectors + borrow registration; stone-category API surface |

Open debts recorded: D10 (docker/podman/systemd adapters), D11 (ceremonies),
D12 (orchestration roles/elections), D13 (borrow credentials vaulting).
