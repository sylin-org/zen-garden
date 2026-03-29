# Design Decisions — Improvements Over Ollama Orchestrator

> Decisions made during Phase 0 review. These deviate from the Ollama
> orchestrator's patterns where the AI orchestrator can do better.

---

## 1. Koi Endpoint Default: `host.docker.internal`

**Ollama**: Defaults to `http://localhost:5641`, requires env var override
for Docker (which is every deployment).

**AI Orchestrator**: Default to `http://host.docker.internal:5641`. The
orchestrator always runs in Docker. Non-Docker dev scenarios override via
`KOI_ENDPOINT` env var.

---

## 2. Docker EXPOSE: Correct Ports

**Ollama**: `EXPOSE 11434 7190` and `garden.ports.proxy="11434"` — both
wrong (actual proxy is 21434).

**AI Orchestrator**: EXPOSE the actual ports listened on. Labels match
reality. Ports that are dynamically activated (per offering discovery)
are documented but not all need to be EXPOSEd.

---

## 3. Proxy Ports: Demand-Driven, Not Configured

**Ollama**: CLI arg `--proxy-port 21434`.

**AI Orchestrator**: No port configuration at all. Ports are deterministic
per offering type (hardcoded in `OfferingKind`). A proxy port only starts
if the corresponding offering is detected in the garden. If no ComfyUI
instances are discovered, port 21435 never binds.

| Offering | Port | Starts When |
|----------|------|-------------|
| Ollama | 21434 | Ollama instance discovered |
| ComfyUI | 21435 | ComfyUI instance discovered |
| whisper.cpp | 21436 | whisper.cpp instance discovered |
| Speaches | 21437 | Speaches instance discovered |
| OpenedAI Speech | 21438 | OpenedAI Speech instance discovered |
| Infinity | 21439 | Infinity instance discovered |
| LibreTranslate | 21440 | LibreTranslate instance discovered |
| Dashboard | 7190 | Always (management UI) |

Operators can enable/disable proxies from the dashboard at runtime.
Zero CLI args for ports. Zero env vars for ports.

---

## 4. Env Var Prefix: `AI_ORCH_*`

**Ollama**: `ROUTER_DATA_DIR`, `ROUTER_PROXY_PORT`, etc.

**AI Orchestrator**: `AI_ORCH_DATA_DIR`, `AI_ORCH_DASHBOARD_PORT`. The
prefix distinguishes this orchestrator from others in the same
deployment.

---

## 5. Offering Name: Hardcoded Constant

**Ollama**: CLI arg `--offering-name zen-garden.ollama.orchestrator`.

**AI Orchestrator**: `const OFFERING_NAME: &str = "zen-garden.ai.orchestrator"`.
Not configurable. This is identity, not a setting.

---

## 6. Request Body: Unbounded Stream-Through

**Ollama**: Buffers entire body to 50MB (`axum::body::to_bytes(body, 50MB)`)
before forwarding.

**AI Orchestrator**: Stream the body through without buffering or size
limits. Use `Body::wrap_stream()` for all proxy paths. For paths that
need to peek at the body (e.g., extract model name from JSON), use a
tee pattern on the first bytes — not full buffering.

---

## 7. Gateway Registration: Single Coordinated Task

**Ollama**: One `gateway_announce` task for one offering.

**AI Orchestrator**: Single `gateway_announce` task that iterates over all
active offerings and registers/heartbeats each. Less task overhead,
coordinated lifecycle.

---

## 8. State: Single AppState + FromRef

**Ollama**: `ProxyState { app, client }` and `ManagementState { app, client }`.

**AI Orchestrator**: Single `AppState` with `FromRef` extractions per
code standard SS6. Offering-specific HTTP clients accessed via
`state.registry.get(OfferingKind)`.

---

## 9. Config File: `config.toml`

**Ollama**: `router-config.toml`.

**AI Orchestrator**: `config.toml`. Generic, correct.

---

## 10. Dashboard Updates: Snapshot on Load + SSE Fanout

**Ollama**: Snapshot publisher task computes full JSON every 2 seconds,
even when no dashboard is connected.

**AI Orchestrator**: Full snapshot loaded on `GET /api/status` (page load).
After that, SSE fanout publishes incremental state changes as they occur.
No periodic polling, no wasted computation when nobody is watching.

---

## 11. Dashboard Assets: Embedded Built Output

**Ollama**: `include_str!("../../assets/dashboard.html")` — single file.

**AI Orchestrator**: React+TypeScript+Tailwind dashboard built separately.
Output directory embedded via `include_dir!` (or `rust-embed`). Compiled
into the binary for single-binary deployment.

---

## Summary

| # | Concern | Ollama | AI Orchestrator |
|---|---------|--------|-----------------|
| 1 | Koi default | `localhost:5641` | `host.docker.internal:5641` |
| 2 | Docker EXPOSE | Stale/wrong | Correct |
| 3 | Proxy ports | CLI arg | Demand-driven, dashboard-controlled |
| 4 | Env prefix | `ROUTER_*` | `AI_ORCH_*` |
| 5 | Offering name | CLI arg | Hardcoded constant |
| 6 | Body buffer | 50MB limit | Unbounded stream-through |
| 7 | Gateway tasks | One per offering | Single coordinated task |
| 8 | State wrappers | Two structs | Single AppState + FromRef |
| 9 | Config file | `router-config.toml` | `config.toml` |
| 10 | Dashboard data | 2s periodic snapshot | Load + SSE incremental |
| 11 | Dashboard assets | `include_str!` | Embedded built directory |
