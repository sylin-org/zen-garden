# Zen Garden - Architecture Reference
**Version**: 0.1.0 | **Updated**: 2026-01-25

> **Read this FIRST. Don't reinvent wheels. Use what exists.**

---

## Core Rules

### IDs
Use GUIDv7 via `garden_common::utils::ids::generate_guidv7()`.

### Paths
Use `garden_common::constants::paths::*` functions. Never hardcode paths or use `#[cfg(target_os)]` conditionals.

### Error Handling
- Domain: `anyhow::Result` with `.context()`
- API: Convert to `(StatusCode, Json<ErrorResponse>)` with codes from `garden_common::constants`
- Never `unwrap()` in production

### Async I/O
- File I/O: `tokio::fs` only, never blocking `std::fs`
- HTTP: `reqwest` with 30s timeout

### Shared Contracts
**Moss and Rake MUST share types via `garden_common`.** No duplicate structs. Example: `garden_common::nourishment::*`

### Orchestration
**Rake does NO orchestration.** Rake talks ONLY to its tended stone.

Moss handles all garden-wide coordination:
- `GET /api/v1/garden/nourishment` → Moss queries ALL stones, aggregates results
- `POST /api/v1/garden/nourishment/execute` → Moss dispatches updates to each affected stone
- `GET /api/v1/garden/observe` → Moss queries ALL stones, aggregates topology

### P2P Transport Singleton (CRITICAL)
**ALL UDP communication MUST go through `infra/communications/p2p.rs`.**

**Rules:**
- ❌ **NEVER** import `tokio::net::UdpSocket` in domain/tasks modules
- ❌ **NEVER** call `UdpSocket::bind()` anywhere except `p2p.rs`
- ✅ **ALWAYS** use `p2p::subscribe_to_events()` for receiving
- ✅ **ALWAYS** use `p2p::send_announcement(type, payload)` for sending

**Pattern:**
```rust
// Receiving (in tasks/handlers)
let mut udp_rx = p2p::subscribe_to_events().await?;
loop {
    match udp_rx.recv().await {
        Ok(UdpEvent::ElectionRequest { request, .. }) => handle(request),
        Ok(UdpEvent::StoneChirp { chirp, .. }) => handle(chirp),
        _ => {}
    }
}

// Sending (from anywhere)
p2p::send_announcement(
    announcement_types::ELECTION_REQUEST,
    &election_request
).await?;
```

**Why:** Prevents port conflicts (7184), enforces SoC/DDD, enables testing.  
**Reference:** [COMM-0001](decisions/COMM-0001-p2p-transport-singleton.md)

---

## Nourishment Endpoints

**Garden endpoints** (Rake → tended Moss, orchestrated):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/garden/nourishment` | Aggregates updates from all stones |
| POST | `/api/v1/garden/nourishment/execute` | Dispatches `{"scope":"offerings"}` to affected stones |

**Stone endpoints** (local or Moss → Moss):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/nourishment` | This stone's pending updates |
| POST | `/api/v1/stone/nourishment/execute` | Execute on this stone |
| GET | `/api/v1/stone/nourishment/stream/:job_id` | SSE status stream |

**Execute payload**:
```json
{"scope": "all"}           // All updates
{"scope": "offerings"}     // Software only
{"scope": "firmware"}      // Firmware only
```

**Flow**:
1. Rake → `GET /api/v1/garden/nourishment` on tended Moss
2. Tended Moss → `GET /api/v1/stone/nourishment` on each stone, aggregates
3. User picks [A], [O], or [F]
4. Rake → `POST /api/v1/garden/nourishment/execute` with scope to tended Moss
5. Tended Moss → `POST /api/v1/stone/nourishment/execute` to each affected stone
6. Each stone interprets scope, applies matching updates

---

## Module Structure

```
src/moss/src/
├── domain/      # Business logic only (no external deps)
├── infra/       # Docker, filesystem, network
├── api/         # HTTP handlers (use garden_common types)
├── tasks/       # Background tasks
└── bootstrap/   # Initialization
```

**Rule**: Domain NEVER imports infra. Use traits.

---

## Existing Utilities

### Formatting → `common/src/utils.rs`
- `format_bytes(u64)` → "1.00 GB"
- `format_uptime(u64)` → "1h 30m"

### Paths → `common/src/constants/paths.rs`
- `data_dir()` → `/var/lib/zen-garden` (Linux) or `.zen-garden` (Windows)
- `config_dir()` → `/etc/zen-garden` (Linux) or `.zen-garden` (Windows)
- `harvest_dir()`, `stored_dir()`, `stone_home()`, `stone_user()`, `first_run_flag()`

### Network → `common/src/constants/mod.rs`
- `DISCOVERY_UDP = 7184`, `MOSS_HTTP = 7185`, `LANTERN_HTTP = 7186`

### Timeouts → `common/src/constants/timeouts.rs`
- `DISCOVERY_TIMEOUT_MS = 3000`, `HTTP_REQUEST_TIMEOUT_MS = 30000`

### Limits → `common/src/constants/limits.rs`
- `MAX_OFFERING_NAME_LENGTH = 64`, `MAX_SERVICES_PER_STONE = 100`

### Phase 1 Utils → `common/src/utils/`
| Module | Functions |
|--------|-----------|
| `formatting.rs` | `format_bytes_precision()`, `format_bytes_short()`, `format_memory_mb()` |
| `env.rs` | `EnvConfig` typed accessors for all `GARDEN_*` vars |
| `fs.rs` | `ensure_dir()`, `read_file()`, `write_file()` with async variants |
| `platform.rs` | `PlatformPaths` trait, `data_dir()`, `config_dir()` |
| `ids.rs` | `generate_guidv7()`, `generate_id(prefix)` |
| `json.rs` | `parse<T>()`, `stringify<T>()`, `stringify_pretty<T>()` |
| `strings.rs` | `truncate()`, `to_kebab_case()`, `to_snake_case()` |
| `validation.rs` | `validate_name()`, `validate_port()`, `validate_url()` |

---

## Environment Variables

**Paths**: `GARDEN_DATA_DIR`, `GARDEN_CONFIG_DIR`, `GARDEN_HARVEST_DIR`, `GARDEN_STAGING_DIR`, `GARDEN_STORED_DIR`

**Stone**: `GARDEN_STONE_NAME`, `GARDEN_STONE_HOST`, `GARDEN_STONE_HOME`, `GARDEN_STONE_USER`, `GARDEN_FIRST_RUN_FLAG`

**Endpoints**: `GARDEN_STONE` (skip discovery), `LANTERN_ENDPOINT`

**Flags**: `NO_COLOR`, `GARDEN_NO_COLOR`, `GARDEN_UNICODE`, `GARDEN_QUIET`, `RUNNING_AS_SERVICE`, `ZEN_GARDEN_CONTAINER`

**External**: `CUDA_PATH`, `INTEL_OPENVINO_DIR`, `SystemRoot`, `PROGRAMDATA`, `HOME`

---

## Binary Names

```rust
MOSS_BINARY = "garden-moss"
RAKE_BINARY = "garden-rake"
LANTERN_BINARY = "garden-lantern"
MOSS_SERVICE = "garden-moss.service"
```

---

## Key Infra Modules

| Module | Purpose |
|--------|---------|
| `moss/src/app_state.rs` | Shared state via `Arc<RwLock<T>>` |
| `moss/src/infra/persistence.rs` | `load_json<T>()`, `save_json<T>()` - atomic writes |
| `moss/src/infra/docker.rs` | Bollard wrapper: containers, images, volumes |
| `moss/src/infra/manifests/` | YAML frontmatter loader for sw/ and hw/ |
| `common/src/client.rs` | `ApiClient` with JSON, 30s timeout |

---

## Operations

### SSH Access to Stones

**Credentials**: `stone` / `stone`

```powershell
# Accept host key (first time)
echo y | plink -ssh "stone@<stone-name>" -pw stone "echo OK"

# Run command
plink -batch -ssh "stone@<stone-name>" -pw stone "<command>"
```

**Common commands**:
```powershell
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "systemctl status garden-moss"
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "sudo journalctl -u garden-moss -n 50"
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "fwupdmgr get-history"
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "fwupdmgr get-updates"
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "docker ps"
plink -batch -ssh "stone@stone-crystal-forest" -pw stone "sudo systemctl restart garden-moss"
```

---

## Quick Reference

**Before writing code**:
1. Check if utility exists above
2. Use `garden_common` types
3. Use path functions for cross-platform
4. Propagate errors with `.context()`
5. Use `tracing::*` for logging

**Key files**:
- `common/src/utils.rs` - Formatting
- `common/src/constants/paths.rs` - Platform paths
- `moss/src/infra/persistence.rs` - JSON I/O
- `moss/src/infra/docker.rs` - Docker ops
