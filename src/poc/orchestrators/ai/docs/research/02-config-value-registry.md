# Configuration Value Registry

> Research artifact for ORCH-0013. Every configuration value traced to its
> exact source in the Ollama orchestrator. The AI orchestrator MUST read
> these from source — never guess or hardcode from this document.
>
> **Rule**: When implementing, open the file at the listed path, read the
> actual `#[arg]` attribute, and copy the default. Do not transcribe from
> this table.

---

## CLI Arguments

### Ollama Orchestrator Source (for reference only — do NOT copy blindly)

| Parameter | Env Var | Default | Source |
|-----------|---------|---------|--------|
| `koi_endpoint` | `KOI_ENDPOINT` | `http://localhost:5641` | ollama/main.rs:26 |
| `stone` | `GARDEN_STONE` | None (optional) | ollama/main.rs:30 |
| `offering_name` | `GARDEN_OFFERING_NAME` | `zen-garden.ollama.orchestrator` | ollama/main.rs:36 |
| `proxy_port` | `ROUTER_PROXY_PORT` | `21434` | ollama/main.rs:43 |
| `dashboard_port` | `ROUTER_DASHBOARD_PORT` | `7190` | ollama/main.rs:47 |
| `data_dir` | `ROUTER_DATA_DIR` | `/data` | ollama/main.rs:51 |
| `log_level` | `RUST_LOG` | `info` | ollama/main.rs:55 |

### AI Orchestrator (corrected defaults — see 05-design-decisions.md)

| Parameter | Env Var | Default | Rationale |
|-----------|---------|---------|-----------|
| `koi_endpoint` | `KOI_ENDPOINT` | `http://host.docker.internal:5641` | Always runs in Docker; localhost never works |
| `stone` | `GARDEN_STONE` | None (optional) | Same as Ollama — skip discovery |
| `dashboard_port` | `AI_ORCH_DASHBOARD_PORT` | `7190` | Same port, new prefix |
| `data_dir` | `AI_ORCH_DATA_DIR` | `/data` | Same path, new prefix |
| `log_level` | `RUST_LOG` | `info` | Standard |
| ~~offering_name~~ | — | Hardcoded constant | Not configurable |
| ~~proxy_port~~ | — | Demand-driven per offering | Not configurable |

---

## Port Assignments

| Port | Service | Binding | Source |
|------|---------|---------|--------|
| 21434 | Ollama proxy | `0.0.0.0:{proxy_port}` | main.rs:204 |
| 7190 | Dashboard | `0.0.0.0:{dashboard_port}` | main.rs:279 |

### AI Orchestrator Port Assignments (from ORCH-0013 ADR)

| Port | Service | Native Port | Source |
|------|---------|-------------|--------|
| 21434 | Ollama | 11434 | ORCH-0013:697 |
| 21435 | ComfyUI | 8188 | ORCH-0013:698 |
| 21436 | whisper.cpp | 8080 | ORCH-0013:699 |
| 21437 | Speaches | 8000 | ORCH-0013:700 |
| 21438 | OpenedAI Speech | 8001 | ORCH-0013:701 |
| 21439 | Infinity | 7997 | ORCH-0013:702 |
| 21440 | LibreTranslate | 5000 | ORCH-0013:703 |
| 7190 | Dashboard | — | ORCH-0013:733 |

**NOTE**: These ports are NOT yet in `well-known-ports.yaml`. Must be
registered before implementation.

---

## Docker Configuration (from `src/orchestrators/ollama/Dockerfile`)

| Setting | Value | Source |
|---------|-------|--------|
| Builder image | `rust:latest` | Dockerfile:5 |
| Runtime image | `debian:bookworm-slim` | Dockerfile:27 |
| EXPOSE | 11434, 7190 | Dockerfile:64 |
| ENV ROUTER_DATA_DIR | `/data` | Dockerfile:66 |
| Binary path | `/usr/local/bin/zen-garden-ollama-orchestrator` | Dockerfile:57 |
| Non-root user | `router` (UID 1000) | Dockerfile:54 |
| Workdir | `/data` | Dockerfile:61 |
| ENTRYPOINT | `[binary path]` | Dockerfile:68 |
| Build profile | `fast-release` (thin LTO, 4 codegen units, stripped) | Cargo.toml:52-56 |

### Docker Networking (CRITICAL — caused first implementation failure)

The Ollama orchestrator defaults to `http://localhost:5641` which never
works in Docker (localhost = container, not host). It relies on runtime
env var injection to override to `host.docker.internal:5641`.

**AI orchestrator fix**: Default directly to `http://host.docker.internal:5641`.
The orchestrator always runs in Docker. Dev/non-Docker overrides via
`KOI_ENDPOINT` env var. No deployment-time workaround needed.

---

## Koi Service Details

| Setting | Value | Source |
|---------|-------|--------|
| Default port | 5641 | Koi crate config (verify in koi source) |
| Protocol | HTTP | Not HTTPS |
| mDNS service type | `_moss._tcp` | garden_common::constants::MDNS_SERVICE_TYPE |
| Health endpoint | `GET /healthz` | orchestrator-common discovery.rs |
| Browse endpoint | `GET /browse?idle_for=5s` | orchestrator-common discovery.rs |
| Subscribe endpoint | `GET /subscribe` (SSE) | orchestrator-common discovery.rs |

---

## Task Intervals (from `src/orchestrators/ollama/src/tasks/`)

| Task | Interval | Startup Delay | Source |
|------|----------|---------------|--------|
| discovery | Reconnect on failure; topology refresh 30s | 0 | tasks/discovery.rs |
| reconciliation | 30s | 10s | tasks/reconciliation.rs |
| health_check | 15s | 0 | tasks/health_check.rs |
| metrics_flush | 30s | 0 (also flushes on shutdown) | tasks/metrics_flush.rs |
| model_sync | 60s | 30s | tasks/model_sync.rs |
| snapshot_publisher | 2s | 0 | tasks/snapshot_publisher.rs |
| metrics_processor | Event-driven (channel recv) | 0 | tasks/metrics_processor.rs |
| placement | 60s | 60s | tasks/placement.rs |
| gateway_announce | 30s heartbeat | Waits for tended stone | tasks/gateway_announce.rs |
| advisor | 300s (5min); reactive on topology events; 5s debounce | 15s | tasks/advisor.rs |

---

## Channel Configuration (from `src/orchestrators/ollama/src/main.rs`)

| Channel | Type | Init Value | Purpose | Source |
|---------|------|-----------|---------|--------|
| `snapshot` | `watch::channel` | `serde_json::json!({})` | Dashboard snapshot | main.rs:85 |
| `metrics` | `mpsc::unbounded_channel` | — | Proxy → metrics processor | main.rs:86 |
| `dashboard_tx` | `broadcast::channel` | capacity unknown (check app_state.rs) | SSE events | app_state.rs |

---

## Persistence Paths (from `src/orchestrators/ollama/src/infra/persistence.rs`)

| File | Format | Purpose |
|------|--------|---------|
| `{data_dir}/router-config.toml` | TOML | User settings (human-editable) |
| `{data_dir}/metrics/summary.json` | JSON | Global metrics counters |
| `{data_dir}/metrics/stones/{name}.json` | JSON | Per-stone metrics |
| `{data_dir}/.tending` | JSON | Cached tended stone binding |
| `{data_dir}/fitness.json` | JSON | Benchmark results + GPU matrix |

---

## Gateway Registration Constants (from orchestrator-common)

| Constant | Value | Source |
|----------|-------|--------|
| HEARTBEAT_INTERVAL_SECS | 30 | tasks/gateway_announce.rs |
| MDNS_LEASE_SECS | 60 | tasks/gateway_announce.rs |
| Shutdown timeout | 5s | main.rs:303 |
| Max jobs in ring buffer | 20 | app_state.rs (check) |
