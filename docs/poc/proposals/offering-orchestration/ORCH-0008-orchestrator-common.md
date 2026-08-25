# ORCH-0008: Orchestrator Common Crate

**Status:** Draft
**Date:** 2026-02-24
**Authors:** Leo Botinelly, Claude
**Depends On:** ORCH-0002 (Ollama orchestrator as extraction source)
**Required By:** ORCH-0007 (MongoDB orchestrator)

---

## Abstract

The Ollama orchestrator (ORCH-0002) contains substantial infrastructure code — stone discovery, gateway registration, tools stream subscription, HTTP helpers, tending state persistence — that is not Ollama-specific. As we build the MongoDB orchestrator (ORCH-0007) and future orchestrators, duplicating this infrastructure is unacceptable.

This proposal extracts the generic orchestrator infrastructure from the Ollama crate into a shared `orchestrator-common` crate at `src/orchestrators/common/`. The Ollama orchestrator is refactored to depend on this crate, and the MongoDB orchestrator consumes it from the start.

The extraction is mechanical — no new features, no API changes. The goal is to move code, not rewrite it.

---

## Table of Contents

1. [What Moves](#what-moves)
2. [What Stays](#what-stays)
3. [Crate Structure](#crate-structure)
4. [Extraction Inventory](#extraction-inventory)
5. [Gateway Announce Task](#gateway-announce-task)
6. [Tools Stream Generalization](#tools-stream-generalization)
7. [Migration Plan](#migration-plan)
8. [Verification](#verification)

---

## What Moves

Every module in the Ollama orchestrator that does NOT reference Ollama-specific types, APIs, or behavior is a candidate for extraction. The litmus test:

> *Would a PostgreSQL orchestrator need this exact same code?*

If yes, it moves.

### Extraction Candidates (from `src/orchestrators/ollama/src/`)

| Module | Verdict | Reason |
|---|---|---|
| `infra/stone_discovery.rs` | **Move entirely** | `DiscoveredStone`, `discover_stones()`, `subscribe_stones()`, `check_stone_health()`, `check_koi_health()`, `fetch_stone_hw()` — none reference Ollama |
| `infra/gateway.rs` | **Move entirely** | `KoiMdnsClient`, `MossGatewayClient`, `GatewayParams`, `PutGatewayResponse` — generic mDNS + Moss gateway |
| `infra/tools_stream.rs` | **Move SSE parsing, parameterize filter** | SSE frame parsing is generic; only `extract_ollama_tool()` and `is_ollama_fqid()` are Ollama-specific |
| `infra/persistence.rs` | **Move tending state only** | `TendedStone` load/save is generic; config and metrics persistence is Ollama-specific |
| `tasks/gateway_announce.rs` | **Move with parameterization** | Lifecycle is identical for all orchestrators; only `MDNS_NAME`, `OFFERING`, and port differ |
| `infra/events.rs` | **Move `DashboardEvent` type** | Broadcast channel event wrapper is generic |
| `infra/ollama_client.rs` | **Keep in Ollama** | Ollama HTTP API client — 100% Ollama-specific |
| `domain/*` | **Keep in Ollama** | All domain logic is VRAM/model/tier specific |
| `api/*` | **Keep in Ollama** | All API handlers reference Ollama state |
| `tasks/reconciliation.rs` | **Keep in Ollama** | Model drift detection — Ollama-specific |
| `tasks/discovery.rs` | **Keep in Ollama** | Uses Ollama-specific topology query (`query_topology_ollama`) |
| `app_state.rs` | **Keep in Ollama** | Heavily Ollama-specific (instances, models, tiers, leases) |

### Special Case: `query_topology_ollama()`

This function in `stone_discovery.rs` queries the topology and filters for Ollama offerings. The generic part (query topology, parse response) moves to orchestrator-common. The filter (Ollama vs MongoDB vs any offering) is parameterized:

```rust
// In orchestrator-common
pub async fn query_topology_offerings(
    stone_endpoint: &str,
    offering_filter: &str,  // "ollama", "mongodb", etc.
) -> Result<Vec<TopologyOfferingStone>> { ... }
```

The Ollama-specific `TopologyOllamaStone` (with `ollama_endpoint()` and VRAM fields) remains in the Ollama crate, constructed from the generic result.

---

## What Stays

Ollama-specific code that does NOT move:

| Module | Reason |
|---|---|
| `domain/routing.rs` | VRAM-tier routing algorithm |
| `domain/advisor.rs` | Topology advisor (VRAM placement) |
| `domain/placement.rs` | Demand-weighted model distribution |
| `domain/policy.rs` | Auto-pull / sync / delete-on-idle |
| `domain/tiering.rs` | VRAM-based tier computation |
| `domain/reconciliation.rs` | Model drift detection |
| `domain/fitness.rs` | Benchmark verdict computation |
| `domain/lease.rs` | High-tier lease management |
| `domain/metrics.rs` | Request counters per stone/model |
| `domain/types.rs` | `OllamaInstance`, `ModelInfo`, `Tier`, `LoadedModel` |
| `infra/ollama_client.rs` | Ollama HTTP API wrapper |
| `api/proxy.rs` | Ollama-compatible proxy endpoint |
| `api/dashboard.rs` | Ollama dashboard HTML + status |
| `api/extension.rs` | `/v1/models`, `/v1/stones` |
| `api/management.rs` | Model pull/delete management |
| `api/benchmark_api.rs` | Fitness profiler endpoints |
| `tasks/reconciliation.rs` | Model drift polling |
| `tasks/model_sync.rs` | Cross-tier model sync |
| `tasks/placement.rs` | Demand-weighted placement |
| `tasks/benchmark.rs` | Fitness benchmarking |
| `tasks/advisor.rs` | Topology advisor computation |
| `tasks/snapshot_publisher.rs` | Dashboard JSON building |
| `tasks/metrics_processor.rs` | Metric event batching |
| `tasks/metrics_flush.rs` | Metrics persistence |
| `app_state.rs` | Ollama-specific shared state |
| `main.rs` | Ollama-specific bootstrap |

---

## Crate Structure

```
src/orchestrators/common/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── discovery.rs           # DiscoveredStone, discover/subscribe/health check
│   ├── gateway.rs             # KoiMdnsClient, MossGatewayClient, GatewayParams
│   ├── tools_stream.rs        # Generic SSE parsing with offering filter callback
│   ├── topology.rs            # Generic topology query with offering filter
│   ├── http.rs                # check_status() helper, error body truncation
│   ├── persistence.rs         # TendedStone load/save
│   ├── events.rs              # DashboardEvent, broadcast channel types
│   ├── jobs.rs                # OrchestratorJob, JobKind, JobStatus (if generic)
│   └── tasks/
│       ├── mod.rs
│       └── gateway_announce.rs  # Parameterized gateway lifecycle task
```

### Cargo.toml

```toml
[package]
name = "orchestrator-common"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
description = "Shared infrastructure for Zen Garden orchestrators"

[dependencies]
garden-common = { path = "../../common" }

anyhow.workspace = true
tokio = { workspace = true, features = ["net", "fs", "io-util"] }
tracing.workspace = true
reqwest = { workspace = true, features = ["stream"] }
serde.workspace = true
serde_json.workspace = true
futures-util = "0.3"
chrono.workspace = true
tokio-util = { version = "0.7", features = ["rt"] }
```

No `axum`, `clap`, `tower-http`, `base64`, `toml` — those are orchestrator-specific (HTTP server, CLI, dashboard rendering).

---

## Extraction Inventory

### `discovery.rs` — Extracted from `infra/stone_discovery.rs`

**Types moved:**
- `DiscoveredStone` (struct + impl)
- `MdnsFoundPayload`, `MdnsSubscribePayload`, `MdnsService` (internal SSE DTOs)

**Functions moved:**
- `discover_stones(koi_endpoint) → Vec<DiscoveredStone>` — one-shot mDNS browse
- `subscribe_stones(koi_endpoint, on_found, on_removed)` — long-lived SSE stream
- `check_stone_health(endpoint) → bool` — health check
- `check_koi_health(koi_endpoint) → bool` — Koi reachability
- `fetch_stone_hw(stone_ip) → (u64, Option<String>)` — hardware capabilities fetch
- `fetch_service_env(moss_endpoint, service_name) → HashMap` — service env var fetch

**Changes:** None. These functions are already generic.

### `topology.rs` — Extracted from `infra/stone_discovery.rs` (bottom half)

**New generic type:**

```rust
/// A stone discovered via the topology endpoint that runs a specific offering.
pub struct TopologyOfferingStone {
    pub stone_id: String,
    pub stone_name: String,
    pub ip: String,
    pub moss_port: u16,
    pub offering_port: Option<u16>,     // From TopologyServiceEntry
    pub capabilities: Option<HardwareCapabilities>,
}
```

**New generic function:**

```rust
/// Query the topology endpoint and return all stones running the specified offering.
pub async fn query_topology_for_offering(
    stone_endpoint: &str,
    offering_name: &str,  // "ollama", "mongodb", etc.
) -> Result<Vec<TopologyOfferingStone>> { ... }
```

The Ollama crate wraps this:

```rust
// In ollama/src/infra/stone_discovery.rs (what remains)
pub async fn query_topology_ollama(stone_endpoint: &str) -> Result<Vec<TopologyOllamaStone>> {
    let generic = orchestrator_common::topology::query_topology_for_offering(
        stone_endpoint, "ollama"
    ).await?;

    generic.into_iter().map(|g| TopologyOllamaStone {
        stone_id: g.stone_id,
        stone_name: g.stone_name,
        ip: g.ip,
        moss_port: g.moss_port,
        vram_total_bytes: g.capabilities.as_ref()
            .and_then(|c| c.hardware.ai_capabilities.as_ref())
            .map(|ai| ai.total_vram_mb * 1_048_576)
            .unwrap_or(0),
        gpu_name: g.capabilities.as_ref()
            .and_then(|c| c.hardware.gpus.first())
            .map(|gpu| gpu.model.clone()),
    }).collect()
}
```

### `gateway.rs` — Extracted from `infra/gateway.rs`

**Types moved (all):**
- `KoiMdnsClient` (struct + impl)
- `MossGatewayClient` (struct + impl)
- `GatewayParams` (struct)
- `PutGatewayResponse` (struct)
- Internal DTOs: `AnnounceRequest`, `AnnounceResponse`, `RegisteredInfo`, `PutGatewayRequest`, `HostInfoResponse`

**Changes:** None. Already fully generic.

### `tools_stream.rs` — Extracted from `infra/tools_stream.rs`

The SSE frame parsing is generic. The offering filter is Ollama-specific.

**Generic layer (moves to common):**

```rust
/// A tool discovered or removed from the Tools API stream.
pub enum ToolStreamEvent {
    /// An offering instance discovered or updated.
    OfferingDiscovered {
        tool_fqid: String,
        stone_id: String,
        stone_name: String,
        endpoint: String,
    },
    /// An offering instance disappeared.
    OfferingRemoved {
        tool_fqid: String,
        stone_id: String,
        stone_name: String,
    },
    /// Heartbeat from the stream.
    Heartbeat,
}

/// Subscribe to the Tools API SSE stream with a filter predicate.
///
/// `fqid_filter` returns true for tool FQIDs this orchestrator cares about
/// (e.g., `|fqid| fqid.starts_with("offering:ollama")`).
pub async fn subscribe_tools_stream(
    stone_endpoint: &str,
    fqid_filter: impl Fn(&str) -> bool,
    on_event: impl FnMut(ToolStreamEvent),
) -> Result<()> { ... }
```

**Ollama-specific layer (stays in ollama):**

```rust
// In ollama/src/infra/tools_stream.rs (simplified wrapper)
pub async fn subscribe_ollama_tools(
    stone_endpoint: &str,
    on_event: impl FnMut(ToolEvent),
) -> Result<()> {
    orchestrator_common::tools_stream::subscribe_tools_stream(
        stone_endpoint,
        |fqid| fqid.starts_with("offering:ollama"),
        |generic_event| {
            // Map ToolStreamEvent → ToolEvent (Ollama-specific)
            on_event(map_to_ollama_event(generic_event));
        },
    ).await
}
```

### `http.rs` — Extracted from multiple modules

Both `stone_discovery.rs` and `tools_stream.rs` contain identical `check_status()` functions. Extract once:

```rust
/// Maximum bytes of error body to include in diagnostics.
pub const ERROR_BODY_MAX: usize = 512;

/// Check HTTP response status. On error, logs and returns an anyhow error
/// with the truncated response body.
pub async fn check_response(
    resp: reqwest::Response,
    label: &str,
) -> Result<reqwest::Response> { ... }
```

### `persistence.rs` — Extracted from `infra/persistence.rs`

Only the `TendedStone` portion moves:

```rust
/// Cached stone endpoint from a previous orchestrator run.
#[derive(Serialize, Deserialize)]
pub struct TendedStone {
    pub stone_name: String,
    pub endpoint: String,
    pub discovered_at: String,
}

pub async fn load_tending(data_dir: &str) -> Option<TendedStone> { ... }
pub async fn save_tending(data_dir: &str, stone: &TendedStone) -> Result<()> { ... }
```

Config loading, metrics persistence, and benchmark persistence stay in Ollama (they serialize Ollama-specific types).

### `events.rs` — Extracted from `infra/events.rs`

```rust
/// Event sent to dashboard SSE subscribers.
#[derive(Clone, Debug)]
pub struct DashboardEvent {
    pub event_type: String,
    pub data: String,  // JSON payload
}
```

### `tasks/gateway_announce.rs` — Parameterized extraction

The gateway announce task follows an identical lifecycle for all orchestrators. Only the constants differ:

```rust
/// Configuration for the gateway announce task.
pub struct GatewayAnnounceConfig {
    /// mDNS service name (e.g., "ollama-orchestrator", "mongodb-orchestrator").
    pub mdns_name: String,
    /// Offering name for Moss gateway registration (e.g., "ollama", "mongodb").
    pub offering: String,
    /// FQN for gateway registration (e.g., "ollama:orchestrator").
    pub fqn: String,
    /// Port the orchestrator proxy/management listens on.
    pub port: u16,
    /// Koi endpoint URL.
    pub koi_endpoint: String,
    /// Source identifier (offering_name from CLI).
    pub source: String,
}

/// Trait for accessing tended stone endpoint from the orchestrator's state.
pub trait TendedStoneProvider: Send + Sync + 'static {
    /// Return the current tended stone endpoint, if available.
    fn tended_endpoint(&self) -> impl std::future::Future<Output = Option<String>> + Send;
}

/// Run the gateway announcement lifecycle.
pub async fn run(
    config: GatewayAnnounceConfig,
    state: impl TendedStoneProvider,
    shutdown: CancellationToken,
) { ... }
```

The Ollama crate passes:

```rust
orchestrator_common::tasks::gateway_announce::run(
    GatewayAnnounceConfig {
        mdns_name: "ollama-orchestrator".into(),
        offering: "ollama".into(),
        fqn: "ollama:orchestrator".into(),
        port: state.proxy_port,
        koi_endpoint: state.koi_endpoint.clone(),
        source: state.offering_name.clone(),
    },
    state.clone(),  // AppState implements TendedStoneProvider
    shutdown.clone(),
).await
```

---

## Migration Plan

### Step 1: Create the crate

1. Create `src/orchestrators/common/Cargo.toml`
2. Create `src/orchestrators/common/src/lib.rs` with module declarations
3. Add `"src/orchestrators/common"` to workspace members in root `Cargo.toml`
4. Verify: `cargo check -p orchestrator-common`

### Step 2: Move code (one module at a time)

For each module:

1. Copy the source file from Ollama to orchestrator-common
2. Remove Ollama-specific references (replace with generics/parameters)
3. Add `orchestrator-common` as a dependency in Ollama's `Cargo.toml`
4. Replace the Ollama module with a thin wrapper that calls orchestrator-common
5. Verify: `cargo check -p zen-garden-ollama-orchestrator`

Order:
1. `http.rs` (no dependencies)
2. `discovery.rs` (depends on `http.rs`)
3. `topology.rs` (depends on `http.rs`)
4. `gateway.rs` (standalone)
5. `persistence.rs` (standalone)
6. `events.rs` (standalone)
7. `tools_stream.rs` (depends on `http.rs`)
8. `tasks/gateway_announce.rs` (depends on `gateway.rs`, `persistence.rs`)

### Step 3: Verify

```bash
cargo check --all
cargo test --package zen-garden-ollama-orchestrator
cargo clippy -- -D warnings
```

### Step 4: MongoDB orchestrator consumes

The MongoDB orchestrator adds `orchestrator-common` as a dependency and uses the shared modules directly — no duplication.

---

## Verification

### Invariants

After extraction:

1. **Ollama orchestrator compiles and passes all tests** — no behavioral changes
2. **No code duplication** — `check_status()` exists exactly once
3. **No Ollama references in orchestrator-common** — grep for "ollama" returns zero hits
4. **Workspace builds cleanly** — `cargo check --all` succeeds
5. **Clippy clean** — `cargo clippy -- -D warnings` passes

### Risk

The extraction is mechanical. The main risk is breaking Ollama's compilation with incorrect imports. Mitigated by doing one module at a time with `cargo check` after each step.

---

## References

- [ORCH-0002: AI Capability Router](ORCH-0002-ai-capability-router.md) — source codebase for extraction
- [ORCH-0007: MongoDB Orchestrator](ORCH-0007-mongodb-orchestrator.md) — first consumer of orchestrator-common
