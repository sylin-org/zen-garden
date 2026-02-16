# Execution Prompt: Offering Orchestration & Autonomous Resilience

## Context

You are implementing the Zen Garden offering orchestration system as defined in three specification documents:

- **ORCH-0001**: Offering Orchestration & Autonomous Resilience (core system)
- **ORCH-0002**: AI Capability Router (Ollama)
- **ORCH-0003**: Database Choreographer (MongoDB)

Read all three specs thoroughly before beginning any implementation work. **This prompt implements ORCH-0001 only.**

## Project Overview

Zen Garden is a distributed computing platform built in Rust. The core daemon is `garden-moss` (Moss), the CLI is `garden-rake` (Rake), and the registry is `lantern`. The project uses a workspace layout with shared types in `garden_common`.

### Existing Structures You Must Know

The codebase already has key systems that this work extends. Read and understand these before writing any code:

| Concept | Actual Location | Key Types |
|---------|----------------|-----------|
| **Runtime offering instance** | `src/common/src/types.rs` | `Offering` — live instance with `offering_id`, `name`, `status`, `health`, `sub_capabilities`, `mode_data: OfferingModeData` |
| **Manifest offering definition** | `src/common/src/manifests/offering.rs` | `manifests::offering::Offering` — template with `name`, `category`, `managed`, `adopted`, `borrowed`, `metadata` |
| **Manifest registry** | `src/common/src/manifests/registry.rs` | `ManifestRegistry` — in-memory catalog |
| **Election system** | `src/common/src/election.rs` | `ElectionType`, `ElectionRequest`, `ElectionCandidate`, `ElectionResult`, `ElectionWinner` — BLAKE3-hash delay, first-respondent-wins |
| **Election service** | `src/moss/src/tasks/election_service.rs` | `ElectionService` — UDP via p2p, manages pending/initiated elections |
| **Election API** | `src/moss/src/api/v1/election.rs` | `POST /api/v1/election/start` |
| **Election CLI** | `src/rake/src/commands/election.rs` | `garden-rake election start` |
| **P2P announcement types** | `src/common/src/infra/communications/announcement_types.rs` | String constants: `ELECTION_REQUEST`, `ELECTION_CANDIDATE`, `ELECTION_RESULT`, `STONE_CHIRP`, `STONE_GOODBYE`, `STORAGE_BEACON`, `TOOLS_BEACON` |
| **Chirp listener** | `src/moss/src/infra/listeners/chirp.rs` | Domain event → chirp trigger (payload is `TopologyEntry`) |
| **UDP dispatch** | `src/moss/src/tasks/coordinator.rs` | `start_discovery_listener()` dispatches by announcement type string |
| **Topology entry** | `src/common/src/types/topology.rs` | `TopologyEntry` — wire format for chirp: `stone_id`, `stone_name`, `address`, `services: Vec<TopologyServiceEntry>`, `capabilities`, `health`, `status`; has `stripped_for_chirp()` |
| **TopologyServiceEntry** | `src/common/src/types.rs` | 5 fields: `offering_id`, `name`, `offering`, `category`, `status` — lightweight per-service struct in chirp |
| **AppState** | `src/moss/src/app_state.rs` | 41-field shared state; has `election_service: Arc<ElectionService>` |
| **Tools API** | `src/moss/src/api/v1/tools.rs` | `list_garden_tools_v1` (GET), `stream_garden_tools_v1` (SSE) |
| **Tool projection** | `src/common/src/tools/types.rs` | `ToolProjection` — 15 fields; participates in `ToolsBeacon` (UDP) |
| **Presence events** | `src/common/src/presence/event_types.rs` | Constants like `offering.status.up`, `offering.removed`, `stone.health.changed` |
| **Offering events** | `src/moss/src/domain/events.rs` | `OfferingEvent` enum: `Deployed`, `Started`, `Stopped`, `Removed`, `Destroyed`, `Updated`, `Renamed`, `HealthChanged` — emitted on event bus |
| **Health monitor** | `src/moss/src/tasks/health_monitor.rs` | Runs every 30s, checks Docker health, updates `offering.status`/`health`, triggers chirp via `sync_self_services(true)` |
| **Offering deployment** | `src/moss/src/tasks/job_executors.rs` | `install_service_task()` — status: `Installing` → `Running`, emits `OfferingEvent::Deployed` |
| **Offering mutation** | `src/moss/src/app_state.rs` | `upsert_offering()` — write + touch + sync + persist + chirp. BUT many callers bypass it, mutating `state.offerings` directly |
| **Cordon stub** | `src/moss/src/api/v1/services.rs` | `cordon_service_v1` at `POST /api/v1/stone/services/{service}/cordon` — returns `NOT_IMPLEMENTED` (designed for "mark unavailable / drain") |
| **Hardware capabilities** | `src/common/src/types.rs` | `HardwareCapabilities`, `HardwareInventory` |
| **Constants** | `src/common/src/constants/mod.rs` | Sub-modules: `headers`, `limits`, `paths`, `timeouts` |
| **Job system** | `src/moss/src/app_state.rs` | `Job`, `JobStatus` — `AppState.jobs` |
| **Auto-adoption task** | `src/moss/src/tasks/auto_adoption.rs` | `auto_adoption_task(state, config, token)` — pattern to follow for background tasks |
| **Offering mode data** | `src/common/src/types.rs` | `OfferingModeData { Managed(ManagedData), Adopted(AdoptedData), Borrowed(BorrowedData) }` |
| **State provider** | `src/moss/src/tasks/state_provider.rs` | `MossStateProvider` — election criteria provider |
| **Koi handle** | `src/moss/src/app_state.rs` | `koi_handle: Arc<koi_embedded::KoiHandle>` — `.mdns()`, `.dns()`. DNS NOT yet used in Moss |
| **Tending** | `src/rake/src/tending.rs` | `TendingState`, `read_tending()`, `execute_on_stone()` |
| **Bootstrap** | `src/moss/src/bootstrap/run.rs` | Where `ElectionService` is spawned (~L558–636), separate from coordinator |
| **Garden API** | `src/moss/src/api/v1/garden.rs` | `get_garden_v1`, `get_stone_v1`, `get_topology_v1` |
| **Removal handler** | `src/moss/src/api/v1/services.rs` | `delete_service_v1` / `take_away_offering_v1` — removes an offering from a stone |

### What Does NOT Exist Yet

- No `UnifiedOffering`, `SwEntry`, or `OfferingManifest` structs — use the ones in the table above
- No chirp message enum — the system uses string announcement type constants with different payload structs
- No `observe` command in Rake — needs to be created
- No DNS registration code in Moss — `.dns()` handle exists but has never been called; research `koi_embedded::DnsHandle` API first
- No `replicable` field on offering manifests
- No orchestration fields on `ToolProjection` or `TopologyServiceEntry`
- No `RoleChanged` variant on `OfferingEvent`

## Critical Architectural Principles

1. **DRY/YAGNI/KISS** — Reuse existing infrastructure. Do not create parallel systems.
2. **Common-first** — Shared types go in `garden_common`. Moss and Rake import from there.
3. **No magic strings** — Use constants in `announcement_types.rs` and `event_types.rs`.
4. **Moss orchestrates, Rake is thin** — Rake sends requests to Moss. Moss does the work.
5. **Pull, never push** — Replicas pull from primaries. No coordinator pushes state.
6. **Each Stone owns its own problem** — A Moss instance manages its own sync, elections, and state.
7. **Extend, don't parallel** — The existing `ElectionService` gets a new scoring mode. Do NOT build a second election system.
8. **Existing patterns** — Follow `auto_adoption.rs` for background tasks, `tending.rs` for Rake commands, `bootstrap/run.rs` for service spawning.
9. **Domain NEVER imports infra** — Use traits for abstraction boundaries (per `.agentic/CONTEXT.md`).
10. **Backward compatibility** — All new fields on persisted structs use `Option<>` with `#[serde(default)]`. Existing JSON on disk must deserialize without error.
11. **Event-driven fan-out** — Role changes go through `OfferingEvent`, which already drives chirps, presence, and tools projection. Do NOT manually emit events at each callsite.
12. **Scoring is opaque** — The election system picks the highest score. How a Stone computes its score is a Moss-private implementation detail, not a shared type.
13. **One task per offering lifecycle** — Sync is a behavior within the Dormant state, not a separate background task. One orchestration task manages the full state machine.

## Implementation Plan

Execute the following phases IN ORDER. Each phase must compile with zero errors and zero warnings before proceeding. Run `cargo build --workspace` and `cargo clippy --workspace -- -D warnings` after each phase.

---

### Phase 1: Shared Types & State Machine

**Goal:** Add orchestration role to the runtime offering model and make it visible on chirps.

**Files to modify:**
- `src/common/src/types.rs` — Add `OrchestrationState` to `Offering`, add `role` to `TopologyServiceEntry`
- `src/common/src/manifests/offering.rs` — Add `replicable` and `orchestration_constraints` fields
- `src/common/src/constants/mod.rs` — Add `orchestration` sub-module
- `src/common/src/election.rs` — Add `ScoreMechanism`, `OfferingPrimary` election type, `score` field on candidate
- `src/moss/src/domain/events.rs` — Add `RoleChanged` variant to `OfferingEvent`
- `src/common/src/presence/event_types.rs` — Add orchestration event constants

**Steps:**

1. **Read the existing runtime `Offering` struct** in `src/common/src/types.rs`. Understand fields, `OfferingModeData`, serialization, persistence. Do NOT break backward compatibility.

2. **Add orchestration types to `types.rs`:**

```rust
/// Orchestration role for multi-instance coordination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OfferingRole {
    Primary,
    Dormant,
    Joining,
    Degraded,
}

impl Default for OfferingRole {
    fn default() -> Self { OfferingRole::Primary }
}

/// Orchestration state persisted on each offering instance.
/// Starts minimal; sync fields added in Phase 4 when sync exists.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrchestrationState {
    pub role: OfferingRole,
    pub primary_stone_id: Option<String>,   // Who is the current primary
    pub pinned: bool,
    pub pin_timestamp: Option<String>,      // ISO timestamp, for tiebreaking
}
```

Add to the runtime `Offering` struct:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub orchestration: Option<OrchestrationState>,
```

3. **Add `role` to `TopologyServiceEntry`** — this piggybacks orchestration role on chirps for free (~20 bytes/service, <3% overhead on a 1500–3500 byte chirp):

```rust
pub struct TopologyServiceEntry {
    #[serde(default)]
    pub offering_id: String,
    pub name: String,
    pub offering: String,
    pub category: String,
    pub status: String,
    /// Orchestration role: "primary", "dormant", "joining", "degraded".
    /// None when orchestration is not active for this offering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}
```

Update `TopologyServiceEntry::from_service_info()` to populate `role` from the offering's `OrchestrationState`.

4. **Add `replicable` and typed `orchestration_constraints` to the manifest:**

```rust
/// Whether this offering supports replication. Default: true.
#[serde(default = "default_true")]
pub replicable: Option<bool>,

/// Hardware/capability constraints for election eligibility.
/// Evaluated at election time against the stone's capabilities.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub orchestration_constraints: Option<OrchestrationConstraints>,
```

```rust
/// Typed constraints — validated at manifest load time, not at election time.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrchestrationConstraints {
    /// CPU features required (any-of match). e.g. ["avx2", "sse4.2"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_features: Option<Vec<String>>,
    /// Architectures allowed (any-of match). e.g. ["x86_64", "aarch64"]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architectures: Option<Vec<String>>,
    /// Minimum memory in MB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_memory_mb: Option<u64>,
    /// Minimum free storage in MB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_storage_mb: Option<u64>,
}
```

5. **Add orchestration constants** (new `src/common/src/constants/orchestration.rs`):

```rust
/// Fitness mode: quiet timeout — no new candidate = decision time
pub const FITNESS_QUIET_TIMEOUT_MS: u64 = 1_000;
/// Fitness mode: hard cap — never wait longer than this
pub const FITNESS_HARD_CAP_MS: u64 = 3_000;
/// Degradation: consecutive health failures before role transition
pub const DEGRADATION_CONSECUTIVE_FAILURES: u32 = 3;
/// Degradation: check interval
pub const DEGRADATION_CHECK_INTERVAL_SECS: u64 = 10;
/// Sync: dormant replica poll interval
pub const SYNC_CHECK_INTERVAL_SECS: u64 = 60;
/// Pinned fitness score (outside valid range, always wins)
pub const FITNESS_SCORE_PINNED: i16 = 1001;
/// Fitness score valid range
pub const FITNESS_SCORE_MIN: i16 = -1000;
pub const FITNESS_SCORE_MAX: i16 = 1000;
/// Default resource thresholds for degradation detection
pub const DEFAULT_MEMORY_THRESHOLD_PCT: f64 = 90.0;
pub const DEFAULT_CPU_THRESHOLD_PCT: f64 = 95.0;
pub const DEFAULT_DISK_THRESHOLD_PCT: f64 = 95.0;
```

6. **Extend the election system** in `src/common/src/election.rs`:

Add `OfferingPrimary` to `ElectionType`:

```rust
pub enum ElectionType {
    UpdateSource,
    CeremonyCoordinator,
    ReplicaTarget,
    BackupSource,
    OfferingPrimary,        // NEW — elect primary for a replicated offering
    Custom(String),
}
```

Add `ScoreMechanism`:

```rust
/// How candidates are ranked during an election.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScoreMechanism {
    /// BLAKE3 hash → delay → first respondent wins (existing).
    #[default]
    Blake,
    /// Candidates respond immediately with a fitness score (i16).
    /// Highest score wins. Pinned = 1001.
    /// Ineligible stones don't respond at all.
    Fitness,
}
```

Add `score_mechanism` to `ElectionRequest`:

```rust
#[serde(default)]
pub score_mechanism: ScoreMechanism,
```

Add `score` and `pin_timestamp` to `ElectionCandidate`:

```rust
/// Fitness score [-1000..1000], or 1001 if pinned.
/// Present only in Fitness mode. Absent in Blake mode.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub score: Option<i16>,
/// ISO timestamp of pin — tiebreaker when multiple candidates score 1001.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub pin_timestamp: Option<String>,
```

7. **Add `RoleChanged` to `OfferingEvent`** in `src/moss/src/domain/events.rs`:

```rust
RoleChanged {
    offering_name: String,
    old_role: OfferingRole,
    new_role: OfferingRole,
},
```

Wire `RoleChanged` into the existing event bus subscribers so that:
- The chirp listener picks it up and triggers a chirp (role now on `TopologyServiceEntry`)
- The presence stream emits the corresponding presence event
- The tools projector updates `OrchestrationState` on the `ToolProjection`

This is the **key SoC win**: one event emission, all downstream systems react automatically.

8. **Add presence event constants** in `event_types.rs`:

```rust
pub const OFFERING_ELECTION_STARTED: &str = "offering.election.started";
pub const OFFERING_ROLE_PROMOTED: &str = "offering.role.promoted";
pub const OFFERING_ROLE_DEMOTED: &str = "offering.role.demoted";
pub const OFFERING_SYNC_COMPLETED: &str = "offering.sync.completed";
pub const OFFERING_HEALTH_DEGRADED: &str = "offering.health.degraded";
```

**Checkpoint:** `cargo build --workspace` passes. All existing tests pass. No warnings. Chirp datagrams now carry `role` on `TopologyServiceEntry` (null for non-orchestrated offerings).

---

### Phase 2: Fitness Scoring & Election Extension

**Goal:** Add Fitness scoring mode to the existing election system. Scoring is simple and opaque: each Stone computes a single `i16`, the election system picks the highest.

**Files to modify:**
- `src/moss/src/tasks/election_service.rs` — Fitness collection + tally mode
- `src/moss/src/tasks/state_provider.rs` (or new sibling) — `compute_fitness_score()` function

**Design principles:**

- **Score is `i16` in range [-1000, 1000].** Pinned = 1001 (outside range, always wins).
- **Ineligible stones don't respond.** If a Stone fails manifest constraints (wrong arch, insufficient memory), it simply never sends an `ELECTION_CANDIDATE`. No filtering needed on the collection side. This is how Blake mode already works.
- **Scoring function lives in Moss, not in `garden_common`.** The election protocol only knows "candidates have scores, highest wins." How a Stone computes that number is its own business and can evolve without protocol changes.
- **No weight tables, no trigger profiles, no `FitnessInput` god struct.** Start with a simple best-effort heuristic using data already available in `AppState` and `HardwareCapabilities`.

**Steps:**

1. **Implement `compute_fitness_score()`** in Moss (e.g. `src/moss/src/tasks/state_provider.rs` or a new `src/moss/src/domain/fitness.rs`):

```rust
/// Compute this stone's fitness for hosting an offering.
/// Returns i16 in [-1000, 1000]. Higher is better.
/// Returns None if ineligible (caller should not respond).
pub fn compute_fitness_score(
    state: &AppState,
    offering: &Offering,
    manifest_constraints: Option<&OrchestrationConstraints>,
) -> Option<i16>
```

Implementation guidance (start simple, refine later):

```rust
// 1. Check eligibility — return None if constraints fail
if !meets_constraints(state, manifest_constraints) { return None; }
if !state.network_reachable() { return None; }

// 2. Compute score from locally available metrics
let mut score: f64 = 0.0;

// CPU headroom (0–250 points)
let cpu_free = 100.0 - state.avg_cpu_load_percent();
score += (cpu_free / 100.0) * 250.0;

// Memory headroom (0–250 points)
let mem_free = 100.0 - state.avg_memory_used_percent();
score += (mem_free / 100.0) * 250.0;

// Offering count penalty (fewer = better, 0–250 points)
let offering_count = state.offerings.read().await.len();
let load_score = (1.0 / (1.0 + offering_count as f64)) * 250.0;
score += load_score;

// Health bonus (0 or 250)
if offering.health == ServiceHealthStatus::Healthy {
    score += 250.0;
}

// Clamp to valid range
Some((score as i16).clamp(-1000, 1000))
```

This is intentionally simple. The function can grow (sync freshness in Phase 4, latency, uptime) without changing the election protocol. The score is **opaque** to everyone except this function.

2. **Implement `meets_constraints()`** — checks `OrchestrationConstraints` against `HardwareCapabilities`:

```rust
fn meets_constraints(
    state: &AppState,
    constraints: Option<&OrchestrationConstraints>,
) -> bool {
    let Some(c) = constraints else { return true; };
    let caps = &state.capabilities;

    if let Some(ref archs) = c.architectures {
        if !archs.iter().any(|a| a == &caps.hardware.cpu.architecture) {
            return false;
        }
    }
    if let Some(ref features) = c.cpu_features {
        if !features.iter().any(|f| caps.hardware.cpu.features.contains(f)) {
            return false;
        }
    }
    if let Some(min_mem) = c.min_memory_mb {
        if caps.hardware.memory.total_mb < min_mem { return false; }
    }
    if let Some(min_storage) = c.min_storage_mb {
        if caps.hardware.disk.free_mb() < min_storage { return false; }
    }
    true
}
```

3. **Modify `ElectionService`** in `src/moss/src/tasks/election_service.rs`:

**Responding to requests (`handle_election_request()`):**
- `ScoreMechanism::Blake` → existing behavior (BLAKE3 delay, spawn timer)
- `ScoreMechanism::Fitness` → compute score immediately via `compute_fitness_score()`. If `None` (ineligible), don't respond. If `Some(score)`, send `ELECTION_CANDIDATE` with `score`, no delay.
- If the offering is pinned, set `score = 1001` and `pin_timestamp` from orchestration state.

**Starting elections (`start_election()`):**
- `ScoreMechanism::Blake` → existing behavior (take first respondent)
- `ScoreMechanism::Fitness` → collect candidates until (1s quiet OR 3s hard cap), then pick winner:

```rust
fn resolve_fitness_election(candidates: &[ElectionCandidate]) -> Option<ElectionWinner> {
    // 1. Highest score wins
    // 2. Equal scores: most recent pin_timestamp wins (if present)
    // 3. Still tied: lexicographically higher stone_id wins (deterministic)
    candidates.iter()
        .max_by(|a, b| {
            a.score.cmp(&b.score)
                .then_with(|| a.pin_timestamp.cmp(&b.pin_timestamp))
                .then_with(|| a.stone_id.cmp(&b.stone_id))
        })
        .map(|c| ElectionWinner { /* ... */ })
}
```

The requester also self-bids in Fitness mode (injects its own candidacy into the collection if it holds a replica). In Blake mode the requester excludes itself.

4. **Write unit tests:**
- `resolve_fitness_election()`: no candidates, single, tied scores, dual-pinned different timestamps, stone_id tiebreak
- `compute_fitness_score()`: basic scoring, constraint rejection, pinned override
- Blake mode unchanged (regression)

**Checkpoint:** All tests pass. `cargo clippy --workspace -- -D warnings` clean. Existing Blake-mode callers untouched — they default to `Blake`.

---

### Phase 3: Orchestration Task in Moss

**Goal:** A single background task manages the full offering orchestration lifecycle: role assignment, primary monitoring, dual-primary resolution, elections, and (later) sync.

**Files to create:**
- `src/moss/src/tasks/offering_orchestration.rs` — The orchestration state machine

**Files to modify:**
- `src/moss/src/tasks/mod.rs` — Declare module
- `src/moss/src/bootstrap/run.rs` — Spawn task (follow ElectionService pattern, NOT coordinator)

**Steps:**

1. **Create `offering_orchestration.rs`** following `auto_adoption_task` signature:

```rust
pub async fn offering_orchestration_task(
    state: AppState,
    token: CancellationToken,
) -> Result<()>
```

2. **State machine dispatch.** The task's main loop iterates offerings with orchestration state and dispatches by role:

```rust
match role {
    Primary   => check_own_health(), watch_for_dual_primary()
    Dormant   => watch_primary_heartbeat()  // sync added in Phase 4
    Joining   => (no-op until Phase 4)
    Degraded  => wait_for_election_result()
}
```

3. **Role transitions go through a single function** that emits `OfferingEvent::RoleChanged`:

```rust
async fn transition_role(
    state: &AppState,
    offering_name: &str,
    new_role: OfferingRole,
) -> Result<()> {
    let old_role = /* read current role */;
    /* update orchestration.role on the offering */
    /* persist */
    state.event_bus.emit(OfferingEvent::RoleChanged {
        offering_name: offering_name.to_string(),
        old_role,
        new_role,
    });
    Ok(())
}
```

Because `RoleChanged` flows through the event bus, the chirp listener automatically broadcasts the new role on `TopologyServiceEntry`, the presence stream emits `offering.role.promoted`/`demoted`, and the tools projector updates. Zero manual wiring.

4. **First-deploy-is-primary.** Hook into the offering deployment path. After `install_service_task()` sets status to `Running` (in `job_executors.rs`), check the topology cache for another Stone running the same offering FQN. If none → set role to `Primary`. If found → set role to `Joining`. The FQN is constructed from the offering's `name` field (see `tool_fqid` pattern in `src/moss/src/domain/tools/projector.rs`).

5. **Primary heartbeat monitoring.** For Dormant offerings, the orchestration task watches the topology cache (populated from chirps) for the primary's `TopologyServiceEntry`. If the primary's chirp hasn't been seen for `FITNESS_HARD_CAP_MS * 2` (6 seconds), call `state.election_service.start_election()` with `ElectionType::OfferingPrimary` and `ScoreMechanism::Fitness`.

No custom heartbeat tracking needed — the existing chirp pipeline IS the heartbeat. The topology cache already tracks last-seen timestamps.

6. **Dual-primary resolution.** When receiving a chirp from another Stone listing the same offering FQN with `role: "primary"`, and this Stone is also Primary → **the Stone with the lexicographically lower `stone_id` yields** (deterministic, no flapping). The yielder calls `transition_role(Dormant)`. No fitness comparison needed — this is a conflict breaker, not an election.

7. **Handle election results.** When `ElectionService` announces a Fitness-mode result: if this Stone won → `transition_role(Primary)`. If lost → `transition_role(Dormant)`.

8. **Pin recovery on startup.** On startup, for any offering with `orchestration.pinned == true`, trigger a re-election. Score 1001 guarantees victory.

9. **Startup reconciliation.** When Moss restarts, the world may have changed while it was down. For every offering with orchestration state: emit a chirp immediately but **do not assert Primary until one full election window (3s) passes**. During this window, watch for chirps from other Stones. If another Stone is already Primary for this FQN, yield to it (become Dormant). If no other Primary seen, retain Primary. This prevents a stale Primary from conflicting with a legitimately elected replacement.

10. **Spawn in bootstrap** (`src/moss/src/bootstrap/run.rs`):

```rust
let orch_state = state.clone();
let orch_token = shutdown_token.clone();
tokio::spawn(async move {
    if let Err(e) = offering_orchestration_task(orch_state, orch_token).await {
        tracing::error!(error = ?e, "Offering orchestration task failed");
    }
});
```

**Checkpoint:** Single-stone deployment works (first deploy = Primary). Orchestration task runs without crashing. Chirps carry role.

---

### Phase 4: DNS Publication via Koi

**Goal:** Primary registers DNS, non-primaries don't.

**IMPORTANT:** The `koi_embedded::DnsHandle` API has not been used in Moss before. Read the `koi_embedded` source to verify `DnsEntry` fields and method signatures. Code below is illustrative.

**Files to modify:**
- `src/moss/src/tasks/offering_orchestration.rs` — DNS on role change
- Existing offering lifecycle code (where offerings are started/stopped)

**Steps:**

1. **On `transition_role(Primary)`** — register DNS:

```rust
if let Ok(dns) = state.koi_handle.dns() {
    let dns_name = format!("{}.lan", offering_name_to_dns(fqn));
    dns.add_entry(DnsEntry {
        name: dns_name,
        ip: state.stone_ip.to_string(),
        ttl: None,
    })?;
}
```

2. **On `transition_role(Dormant)` or `transition_role(Degraded)` from Primary** — remove DNS:

```rust
if let Ok(dns) = state.koi_handle.dns() {
    dns.remove_entry(&format!("{}.lan", offering_name_to_dns(fqn)))?;
}
```

3. **FQN to DNS name conversion:**

```rust
fn offering_name_to_dns(fqn: &str) -> String {
    fqn.replace(':', "-")   // "mongodb:analytics" → "mongodb-analytics"
}
```

4. **Idempotency.** Re-registration on restart is fine. Don't error on duplicate entries.

5. **Integration with role transitions.** DNS registration/removal is a side-effect of `transition_role()`, not scattered across callsites.

**Checkpoint:** Deploy on Stone A → `offering.lan` resolves to A. Deploy on B → B dormant, DNS on A. Stop A → election → B promotes → DNS moves to B.

---

### Phase 5: Pull-Based Sync

**Goal:** Dormant replicas sync from primaries.

**Files to modify:**
- `src/common/src/types.rs` — Add sync fields to `OrchestrationState`
- `src/moss/src/tasks/offering_orchestration.rs` — Add sync behavior to Dormant/Joining states
- Moss stone API — Add cursor endpoint

**Steps:**

1. **Extend `OrchestrationState`** with sync fields (backward-compatible via `#[serde(default)]`):

```rust
pub struct OrchestrationState {
    pub role: OfferingRole,
    pub primary_stone_id: Option<String>,
    pub pinned: bool,
    pub pin_timestamp: Option<String>,
    // Added in Phase 5:
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_cursor: Option<String>,        // ISO timestamp or sequence
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_method: Option<String>,        // "snapshot", "capabilities", "seed-bank"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_check: Option<String>,
}
```

2. **Add cursor endpoint to Moss API:**

```
GET /api/v1/stone/offerings/:name/cursor
→ { "cursor": "2026-02-16T14:30:00Z", "method": "snapshot" }
```

3. **Add sync behavior to the orchestration task.** Within the Dormant/Joining state dispatch:

```rust
Dormant | Joining => {
    watch_primary_heartbeat();  // existing

    // Sync: poll primary's cursor periodically
    if time_since_last_sync_check > SYNC_CHECK_INTERVAL {
        let primary_cursor = GET primary/api/v1/stone/offerings/{name}/cursor;
        if local_cursor < primary_cursor {
            pull_sync(offering, primary_endpoint, local_cursor);
            update_local_cursor();
        } else if role == Joining {
            transition_role(Dormant);  // fully caught up
        }
    }
}
```

4. **Sync dispatch by offering mode** — use `OfferingModeData` to determine the sync method:

- **Managed** (containers): Volume snapshot via Moss API, transfer as tarball, apply locally. Use job system for long-running transfers.
- **Adopted** (with capabilities): Use existing capability mirroring (`/api/v1/stone/offerings/:name/capabilities/mirror`).
- **Borrowed**: No sync — borrowed resources can't be replicated. Set `replicable: false` equivalent behavior.

Consider defining a `Syncable` trait dispatched by mode for clean separation, but this is an implementation choice, not a requirement.

5. **Fitness re-election precondition.** For non-emergency rebalance elections (`garden-rake rebalance`), only fire if all known replicas' sync cursors match the primary. Failure elections skip this check.

**Checkpoint:** Second deployment enters Joining, syncs, transitions to Dormant.

---

### Phase 6: Pinning & Lifecycle Edge Cases

**Goal:** Pin/unpin commands, graceful Primary removal, last-copy seed-bank archival.

**Files to create:**
- New Rake command for pin/unpin

**Files to modify:**
- Moss stone API — pin/unpin endpoints
- `src/moss/src/api/v1/services.rs` — modify `delete_service_v1` / `take_away_offering_v1` for graceful removal

**Steps:**

1. **Pin/unpin API:**

```
POST /api/v1/stone/offerings/:name/pin
→ { "pinned": true, "pin_timestamp": "2026-02-16T14:30:00Z" }

DELETE /api/v1/stone/offerings/:name/pin
→ { "pinned": false }
```

2. **Rake commands** (follow existing command patterns):

```bash
garden-rake pin <offering> <stone>     # Pin to named stone
garden-rake unpin <offering>           # Remove pin from tended stone
```

3. **Pin recovery** — already covered in Phase 3 (startup → re-election → 1001 wins).

4. **Graceful Primary removal.** When `take_away_offering_v1` is called on a Primary that has replicas:
   - Trigger a Fitness-mode election (this Stone does NOT self-bid)
   - Wait for `ELECTION_RESULT` naming a new Primary (timeout: `FITNESS_HARD_CAP_MS`)
   - The new Primary transitions (gets DNS, emits events)
   - THEN proceed with removal of this offering

   This prevents unplanned failover when intentionally removing a Primary.

5. **Last-copy seed-bank archival.** When `take_away_offering_v1` is called and this is the **last instance in the garden** (no other Stone lists this FQN in topology):
   - If a seed-bank offering is discovered in the garden's tools cache:
     - Return a response indicating last-copy status and seed-bank availability
     - Rake prompts: *"This is the last instance of `mongodb`. Archive to seed-bank before removal? [Y/n]"*
     - If yes → Moss snapshots the offering's volume/capabilities to the seed-bank (using the same sync mechanism as Phase 5 in reverse), via the job system. Wait for job completion, then remove.
     - If no → remove immediately
   - If no seed-bank exists: remove immediately (warn that data is permanently lost)

   The API endpoint returns metadata; the interactive prompt lives in Rake. Moss never blocks on user input.

**Checkpoint:** Pin offering, stop Stone, verify failover, restart, verify reclaim. Remove Primary gracefully, verify new Primary before removal completes. Remove last instance with seed-bank archival prompt.

---

### Phase 7: Degradation Detection & Graceful Handover

**Goal:** Primary's Moss detects degradation, initiates graceful handover via existing infrastructure.

**Files to modify:**
- `src/moss/src/tasks/offering_orchestration.rs` — Degradation monitoring in Primary state
- `src/moss/src/api/v1/services.rs` — Implement `cordon_service_v1` (already stubbed)

**Steps:**

1. **Degradation detection** in the orchestration task's Primary dispatch. Track:
   - Consecutive health check failures (read from `offering.health` — already updated by `health_monitor_task` every 30s)
   - Sustained resource pressure (memory, CPU, disk above thresholds)

   After `DEGRADATION_CONSECUTIVE_FAILURES` consecutive failures: `transition_role(Degraded)`.

2. **Degradation flows through chirps naturally.** When role becomes Degraded, the `RoleChanged` event triggers a chirp. The chirp carries `role: "degraded"` on `TopologyServiceEntry`. Dormant replicas see the primary's status change to "degraded" in the topology cache and trigger a Fitness-mode election. **No special `DEGRADATION_WARNING` announcement type needed** — the existing chirp pipeline IS the notification.

3. **Implement `cordon_service_v1`** (already stubbed at `POST /api/v1/stone/services/{service}/cordon`):

```rust
// 1. Stop accepting new connections for this offering
// 2. Wait for existing connections to drain (timeout: 30s)
// 3. Return success when drained or timed out
```

4. **Graceful handover sequence** when this Stone is Primary and a Fitness-mode election triggered by degradation elects a new winner:
   - Call `cordon_service_v1` internally (drain)
   - `transition_role(Dormant)` (removes DNS, emits events)
   - New Primary simultaneously: registers DNS, starts serving

5. **Catastrophic failure** — primary just dies (no degradation). Replicas detect absence via heartbeat timeout (Phase 3) and trigger a failure election. No handover — just promote and go.

**Checkpoint:** Artificially degrade a primary. Verify role → Degraded, chirp broadcasts it, replicas trigger election, graceful handover, DNS migration.

---

### Phase 8: Observability

**Goal:** Orchestration state visible in Tools API, Presence stream, and Rake.

**Files to modify:**
- `src/common/src/tools/types.rs` — Add `OrchestrationState` to `ToolProjection`
- `src/moss/src/domain/tools/projector.rs` — Populate from offering state
- Rake — Add orchestration view

**Steps:**

1. **Add `OrchestrationState` directly to `ToolProjection`** — reuse the domain type, don't create a lossy projection struct:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub orchestration: Option<OrchestrationState>,
```

**Beacon size:** `ToolProjection` participates in `ToolsBeacon` (UDP). Add a `stripped_for_beacon()` method (like `TopologyEntry::stripped_for_chirp()`) that drops or minimizes the orchestration field in the UDP payload.

2. **Presence events are automatic.** `OfferingEvent::RoleChanged` already wired in Phase 1 to emit presence events. No additional work.

3. **Observability in Rake.** Instead of a new top-level `observe` command, add an `--orchestration` flag to the existing `garden-rake garden` command, or make it the default when orchestration state is present:

```
$ garden-rake garden --orchestration

  STONE-AMBER-RIDGE    [thriving]
    my-app             primary    synced    dns: my-app.lan
    ollama:adopted     primary    synced    dns: ollama.lan    4 models

  STONE-CORAL-REEF     [thriving]
    my-app             dormant    synced    replica
    ollama:adopted     dormant    synced    replica            4 models (mirrored)

  STONE-BRONZE-CANYON  [resting]
    my-app             joining    syncing   42% complete
```

This extends existing UI rather than creating a new command surface to discover and document.

**Checkpoint:** `garden-rake garden --orchestration` shows roles. Presence stream emits events during elections.

---

### Phase 9: Lantern Relay (Cross-Subnet)

**Goal:** Elections work across subnets via Lantern.

**Key principle:** This is a **transport concern**, not an orchestration concern. The orchestration task and election service never know or care whether messages travel via UDP multicast or Lantern HTTP relay.

**Files to modify:**
- P2P transport layer — Add Lantern relay backend
- Lantern crate — Add relay endpoint

**Steps:**

1. **Lantern relay endpoint:**

```
POST /api/v1/election/relay
```

Lantern receives election messages (`ELECTION_REQUEST`, `ELECTION_CANDIDATE`, `ELECTION_RESULT`) via HTTP and fans out to all registered Stones.

2. **Transport-layer routing.** Modify `p2p::send_announcement()` (or the transport layer it uses) to:
   - Always send via UDP multicast (existing)
   - If `LANTERN_ENDPOINT` is configured, ALSO send via HTTP to Lantern

   The orchestration task and election service call `send_announcement()` exactly as before. The transport layer handles dual-path delivery transparently.

3. **Partition arbitration.** Lantern tracks which Stones are reachable. The side without Lantern connectivity is degraded.

**Checkpoint:** Two Stones on different subnets participate in elections via Lantern relay.

---

## Testing Strategy

### Unit Tests
- `resolve_fitness_election()` — no candidates, single, tied scores, dual-pinned, stone_id tiebreak
- `compute_fitness_score()` — basic scoring, constraint rejection, pinned = 1001
- `meets_constraints()` — all constraint fields, missing constraints, partial matches
- `ScoreMechanism` dispatch — Blake unchanged, Fitness collects and ranks
- State machine: `transition_role()` emits correct events
- FQN to DNS name conversion
- Startup reconciliation logic

### Integration Tests
- Two-stone deployment: primary + dormant
- Primary failure: election and promotion
- Primary removal: graceful handover before delete
- Last-copy removal: seed-bank archival prompt
- Pin: pin, failover, reclaim
- Cold boot: both stones start simultaneously (reconciliation)
- Dual-primary: deterministic resolution (lower stone_id yields)
- Blake mode elections unaffected (regression)

### Manual Testing Checklist
- [ ] Deploy offering once → Primary, DNS registered
- [ ] Deploy same on second Stone → Joining → synced → Dormant
- [ ] Stop first Stone → election → second promotes → DNS moves
- [ ] Restart first Stone → reconciliation → joins as replica
- [ ] Pin offering to Stone A → A always wins (score 1001)
- [ ] Unpin → fitness-based elections resume
- [ ] Two pinned Stones → most recent pin_timestamp wins
- [ ] `garden-rake garden --orchestration` shows roles
- [ ] Presence stream emits election/role events
- [ ] `replicable: false` → no replica behavior
- [ ] Remove Primary gracefully → election first, then removal
- [ ] Remove last instance → seed-bank prompt if available
- [ ] Existing Blake-mode elections still work unchanged
- [ ] Chirps carry `role` on `TopologyServiceEntry`

## Conventions

- Use `tracing` for all logging (`info!`, `warn!`, `error!`, `debug!`)
- Use `thiserror` for error types
- Use `serde` with JSON for API responses, YAML for manifests
- Use `tokio` for async; `tokio::fs` not `std::fs`
- Mandatory error handling on `tokio::spawn` (per `.agentic/CONTEXT.md`)
- Follow existing module organization
- Run `cargo fmt --all` before committing
- Run `cargo clippy --workspace -- -D warnings` before committing
- Run `cargo test --workspace` before committing

## What NOT to Do

- Do NOT create a separate binary for orchestration. It lives in Moss.
- Do NOT build a second election system. Extend `ElectionService` with Fitness scoring.
- Do NOT add external consensus dependencies (no Raft, no etcd). Elections are UDP via p2p.
- Do NOT create a central coordinator. Each Stone is autonomous.
- Do NOT implement ORCH-0002 or ORCH-0003 here. This prompt is ORCH-0001 only.
- Do NOT add new announcement types for elections. Reuse `ELECTION_REQUEST`, `ELECTION_CANDIDATE`, `ELECTION_RESULT`.
- Do NOT add a `DEGRADATION_WARNING` announcement type. Degradation flows through chirps — the role field on `TopologyServiceEntry` carries it naturally.
- Do NOT break backward compatibility. Use `Option<>` and `#[serde(default)]`.
- Do NOT reference `UnifiedOffering`, `SwEntry`, or `OfferingManifest` — they don't exist.
- Do NOT create a chirp message enum — use announcement type string constants.
- Do NOT create a `FitnessInput` god struct or weight tables. Score is `i16`, computation is Moss-private.
- Do NOT create separate `offering_sync.rs` task. Sync is a behavior within the Dormant state of the single orchestration task.
- Do NOT create an `OrchestrationProjection` wrapper. Reuse `OrchestrationState` directly on `ToolProjection`.
- Do NOT scatter DNS/presence/chirp calls at each role transition. Emit `OfferingEvent::RoleChanged` once; event bus subscribers handle the rest.
- Do NOT import infra from domain modules. Use traits for abstraction boundaries.
- Do NOT use `f64::INFINITY` for pinned scores. Use `1001_i16`.
