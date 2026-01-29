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
- Background tasks: `tokio::spawn` with **mandatory error handling**

### Background Task Error Handling (CRITICAL)
**NEVER allow silent failures in spawned tasks.** Every `tokio::spawn` must:

```rust
// CORRECT - Full error handling with visibility
tokio::spawn(async move {
    if let Err(e) = do_background_work().await {
        // 1. Log at ERROR level with full context
        tracing::error!(
            job_id = %job_id,
            context = %relevant_context,
            error = %e,
            error_chain = ?e,  // Full anyhow chain
            "OPERATION_NAME FAILED"
        );
        
        // 2. Emit event for SSE subscribers (if applicable)
        let failure_event = MossEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: "error".to_string(),
            message: format!("[CATEGORY] FAILED: {} - {}", context, e),
            job_id: Some(job_id.clone()),
        };
        let _ = event_tx.send(failure_event);
    }
});

// WRONG - Silent failure
tokio::spawn(async move {
    let _ = do_background_work().await;  // ❌ Error silently ignored
});
```

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

### Adapter Framework (CRITICAL)
**Moss manages adapters via port ledger and command routing.**

**Port Assignment:**
- Base port: 7187 (ASCII sum "moss adapter" = 1187 + 6000)
- Range: 7187-7199 (13 adapters max)
- Ledger: `{data_dir}/adapter-ports.json` - persistent HashMap<adapter_id, port>
- Assignment: Incremental (7187, 7188, 7189...) via `PortLedger::get_or_assign()`

**Adapter Discovery Protocol:**
1. Moss scans `{data_dir}/adapters/` directory for executables
2. For each adapter: `{executable} --dump-commands --port {assigned_port}`
3. Adapter outputs JSON CommandManifest to stdout
4. Moss caches manifest and starts adapter with `--stone {endpoint} --port {assigned_port}`

**Command Routing:**
```
Rake → POST /api/v1/stone/adapters/{id}/command
  → Moss forwards to http://127.0.0.1:{assigned_port}/command
  → Adapter executes and returns result (5s timeout)
```

**Required Adapter Endpoints:**
| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/command` | Execute commands from Moss |
| `POST` | `/shutdown` | Graceful shutdown (called before upgrades) |
| `GET` | `/health` | Health check |

**Rules:**
- ❌ **NEVER** hardcode adapter ports (use ledger)
- ❌ **NEVER** adopt non-adapter ports (ledger is source of truth)
- ✅ **ALWAYS** pass `--port` during `--dump-commands` and startup
- ✅ **ALWAYS** route commands through Moss (never direct to adapters)
- ✅ **ALWAYS** implement `/shutdown` for graceful upgrade support

**Adapter SDK (garden-adapter-sdk):**
For Rust adapters, use the SDK which provides all standard infrastructure:
- `AdapterConfig` - CLI parsing (`--stone`, `--port`, `--dump-commands`)
- `CommandHandler` trait - Implement to handle commands
- `AdapterRuntime` - HTTP server, shutdown coordination, signals
- `SseClient` - Optional presence event subscription

**Pattern:**
```rust
// Adapter registration (infra/adapters.rs)
let port = port_ledger.get_or_assign(&adapter_id).await?;
invoke_dump_commands(&executable, port).await?;

// Adapter startup
spawn_adapter(&executable, &stone_endpoint, port).await?;

// Command forwarding (api/v1/adapters.rs)
let url = format!("http://127.0.0.1:{}/command", adapter.port);
let response = client.post(&url)
    .json(&command_request)
    .timeout(Duration::from_millis(ADAPTER_COMMAND_TIMEOUT_MS))
    .send().await?;

// Graceful shutdown before upgrade (infra/adapters.rs)
adapter_registry.stop_all().await;  // Calls /shutdown on each adapter
```

**Why:** Centralizes port management, prevents port conflicts, enables adapter hot-reload.  
**Reference:** [ADAPTER-COMMAND-PROTOCOL](specs/ADAPTER-COMMAND-PROTOCOL.md), [ADAPTER-SERVICE-REGISTRY](specs/ADAPTER-SERVICE-REGISTRY.md), [adapter-development.md](guides/adapter-development.md)

### Discovery Transport (Multicast-First)
**UDP discovery uses multicast-first strategy to solve multi-homed system failures.**

**Default Configuration:**
- **Multicast group**: `239.255.42.99` (organization-local scope)
- **Port**: `7184`
- **TTL**: `1` (LAN-only, doesn't route beyond gateway)
- **Directed broadcast fallback**: Enabled by default
- **Limited broadcast (255.255.255.255)**: Disabled by default

**Environment Variables:**
- `DISCOVERY_PORT`: UDP port (default: 7184)
- `DISCOVERY_MCAST_GROUP`: Multicast group IP (default: 239.255.42.99)
- `DISCOVERY_ENABLE_BCAST_FALLBACK`: Enable directed broadcast (default: true)
- `DISCOVERY_ENABLE_LIMITED_BCAST`: Enable 255.255.255.255 fallback (default: false)

**How It Works:**

1. **Sender** (per-interface sockets):
   - Binds socket to each physical interface IP (not `0.0.0.0`)
   - Sends to multicast group `239.255.42.99:7184`
   - Falls back to directed broadcast (computed from IP + netmask)
   - Example: `192.168.32.10/20` → broadcast to `192.168.47.255`
   - Skips virtual adapters (VMware, Hyper-V, VirtualBox, Docker, WSL)

2. **Receiver** (single socket, multiple joins):
   - Binds to `0.0.0.0:7184`
   - Joins multicast group on each physical interface
   - Receives both multicast and broadcast packets

**Why Multicast?**

Limited broadcast (`255.255.255.255`) fails on multi-homed Windows 11 systems (WSL/Hyper-V adapters). The OS routes broadcast packets through the default interface, which may be a virtual adapter instead of the physical NIC. Multicast join operations explicitly specify which interface to listen on, and per-interface sender binding ensures packets egress the correct NIC.

**Virtual Adapter Detection:**

Skips interfaces matching:
- Name patterns: `veth`, `virbr`, `docker`, `br-`, `vmnet`, `vboxnet`, `hyperv`, `wsl`
- Docker bridge network: `172.17.x.x`
- Link-local: `169.254.x.x`
- Loopback: `127.x.x.x`

**Reference:** [discovery-transport.md](discovery-transport.md)

### Container Naming Convention (CRITICAL)
**Managed offerings MUST use `zen-offering-{name}` container naming.**

**Rules:**
- ❌ **NEVER** adopt containers with other names (e.g., `my-mongo`, `user-redis`)
- ❌ **NEVER** adopt native services as managed containers
- ✅ **ALWAYS** check for `zen-offering-{name}` before deploying
- ✅ **ALWAYS** deploy new managed offerings as `zen-offering-{name}`
- ✅ **ALWAYS** adopt orphaned `zen-offering-*` containers (self-heal)

**Pattern:**
```rust
// Before deploying managed offering
let container_name = format!("zen-offering-{}", offering);

// Check if it already exists (self-heal scenario)
if state.docker.container_exists(&container_name).await? {
    // Adopt existing container instead of deploying new one
    adopt_offering_container(&state.docker, &state.manifests, offering).await?;
    return Ok(());
}

// Deploy new container with correct name
state.docker.create_container(
    &container_name,  // zen-offering-mongodb
    &image,
    &ports,
    &volumes,
).await?;
```

**Why:** Prevents namespace collisions (user's containers, native services), enables safe self-heal, clear ownership boundary.  
**Reference:** [OFFER-0002](decisions/OFFER-0002-container-namespace-collision.md)

---

## Adapter Endpoints

**Stone adapter endpoints:**
| Method | Endpoint | Purpose |
|--------|----------|---------|  
| GET | `/api/v1/stone/adapters` | List registered adapters with status/PIDs |
| GET | `/api/v1/stone/adapters/:id` | Get specific adapter details and manifest |
| POST | `/api/v1/stone/adapters/:id/command` | Forward command to adapter (5s timeout) |
| POST | `/api/v1/stone/adapters/:id/up` | Start adapter process |
| POST | `/api/v1/stone/adapters/:id/down` | Stop adapter process |
| POST | `/api/v1/stone/adapters/refresh` | Rescan adapters directory and reload manifests |

**Command forwarding flow:**
1. Rake → `POST /api/v1/stone/adapters/cricket/command` with `{"args": ["play", "stone-online"]}`
2. Moss looks up Cricket's assigned port (7187) from ledger
3. Moss forwards to `http://127.0.0.1:7187/command` with timeout
4. Cricket executes command, returns JSON response
5. Moss proxies response back to Rake

**Adapters (current):**
- **Cricket** (7187): Audio adapter with 4-channel mixer, 180 CC0 samples, tune system
- **Firefly** (planned): LED control adapter for visual presence indicators
- **OLED** (planned): Display adapter for Stone status screens

---

## Offerings Endpoints

**Stone endpoints** (Rake → Moss):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/offerings` | List all offerings (installed + available) |
| GET | `/api/v1/offerings/search?q={query}` | Search offerings with taxonomy normalization |
| GET | `/api/v1/offerings/:name` | Get offering details |
| POST | `/api/v1/offerings` | Plant (install) an offering |
| DELETE | `/api/v1/offerings/:name` | Take away (uninstall) an offering |
| POST | `/api/v1/offerings/refresh` | Refresh offerings catalog from disk |
| POST | `/api/v1/offerings/heal` | Self-heal by adopting orphaned containers |

**Search query parameters**:
- `q` - Free-form query (e.g., "nosql database", "vector store")
- `prefer` - Hardware preferences, comma-separated (e.g., "ssd,nvme")
- `limit` - Max results (default: 5, max: 50)

**Search architecture**: All taxonomy/scoring logic runs server-side in Moss. Rake is a thin client that calls the search API and displays results.

---

## Resolution Endpoints

**Protocol vs Offering distinction:**
- **Protocol** = Wire format (s3, mongodb, redis, storage)
- **Offering** = Software (minio, mongodb, redis)

**Connection string format:**
```
zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>]
```

**Stone endpoints** (Rake → Moss):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/resolve` | Resolve protocol/offering to endpoint |

**Query parameters**:
- `offering` - Offering name (mongodb, minio)
- `protocol` - Protocol (s3, mongodb, redis, storage)
- `instance` - Instance name for multi-instance offerings

**Resolution priority**:
1. Offerings take precedence over seed-bank gateways
2. Instance-specific matches over default instances
3. Health status: healthy > degraded
4. Priority from mDNS TXT records

---

## Seed Bank Endpoints

**Stone endpoints** (Rake → Moss):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/seed-banks` | List configured seed banks |
| POST | `/api/v1/stone/seed-banks` | Add seed bank |
| DELETE | `/api/v1/stone/seed-banks/:name` | Remove seed bank |

**Storage gateway endpoints** (S3-compatible):
| Method | Endpoint | Purpose |
|--------|----------|---------|
| PUT | `/api/v1/storage/{path}` | Put object |
| GET | `/api/v1/storage/{path}` | Get object |
| HEAD | `/api/v1/storage/{path}` | Head object (metadata) |
| DELETE | `/api/v1/storage/{path}` | Delete object |
| GET | `/api/v1/storage/` | List objects |

**Required headers**:
- `X-App-Name` - Application namespace for isolation

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
- `adapters_dir()` → `{data_dir}/adapters/` (adapter executables)
- `harvest_dir()`, `stored_dir()`, `stone_home()`, `stone_user()`, `first_run_flag()`

### Network → `common/src/constants/mod.rs`
- `DISCOVERY_UDP = 7184`, `MOSS_HTTP = 7185`, `LANTERN_HTTP = 7186`

### Timeouts → `common/src/constants/timeouts.rs`
- `DISCOVERY_TIMEOUT_MS = 3000`, `HTTP_REQUEST_TIMEOUT_MS = 30000`
- `ADAPTER_COMMAND_TIMEOUT_MS = 5000` (command forwarding timeout)

### Limits → `common/src/constants/limits.rs`
- `MAX_OFFERING_NAME_LENGTH = 64`, `MAX_SERVICES_PER_STONE = 100`

### Adapter Types → `common/src/adapter.rs`
- `AdapterCommandRequest` - Command forwarding request (args: Vec<String>)
- `AdapterCommandResponse` - Command result (success: bool, output: String)
- `AdapterManifest` - Adapter metadata (name, version, description, port)

### Command Manifest → `common/src/command_manifest/`
- `CommandManifest` - Full adapter command manifest
- `CommandParameter` - Parameter definition (name, type, required, description)
- `CommandExample` - Usage example (command, description, expected output)
- Helper: `check_dump_commands()` - Outputs manifest and exits (for adapters)

### Offering Search → `common/src/offerings.rs`
- `TaxonomyDictionary` - Synonym mapping for search normalization (nosql → mongodb)
- `OfferingSearchRequest` - Query, prefer, limit parameters
- `OfferingSearchResponse` - Query tokens and ranked results
- `OfferingSearchResult` - Single offering with score and compatibility

### Phase 1 Utils → `common/src/utils/`
| Module | Functions |
|--------|-----------|
| `formatting.rs` | `format_bytes_precision()`, `format_bytes_short()`, `format_memory_mb()` |
| `env.rs` | `EnvConfig` typed accessors for all `ZG_*` vars |
| `fs.rs` | `ensure_dir()`, `read_file()`, `write_file()` with async variants |
| `platform.rs` | `PlatformPaths` trait, `data_dir()`, `config_dir()` |
| `ids.rs` | `generate_guidv7()`, `generate_id(prefix)` |
| `json.rs` | `parse<T>()`, `stringify<T>()`, `stringify_pretty<T>()` |
| `strings.rs` | `truncate()`, `to_kebab_case()`, `to_snake_case()` |
| `validation.rs` | `validate_name()`, `validate_port()`, `validate_url()` |

---

## Environment Variables

Environment variables use the `ZG_` prefix for consistency.

**Paths**: `ZG_DATA_DIR`, `ZG_CONFIG_DIR`, `ZG_HARVEST_DIR`, `ZG_STAGING_DIR`, `ZG_STORED_DIR`

**Stone**: `ZG_STONE_NAME`, `ZG_STONE_HOST`, `ZG_STONE_HOME`, `ZG_STONE_USER`, `ZG_FIRST_RUN_FLAG`

**Endpoints**: `ZG_STONE` (skip discovery), `ZG_LANTERN`

**Resolution**: `ZG_PARTITION` (default partition), `ZG_INSTANCE` (default instance)

**Flags**: `NO_COLOR`, `ZG_NO_COLOR`, `ZG_UNICODE`, `ZG_QUIET`, `RUNNING_AS_SERVICE`, `ZG_CONTAINER`

**External**: `CUDA_PATH`, `INTEL_OPENVINO_DIR`, `SystemRoot`, `PROGRAMDATA`, `HOME`

**Legacy support**: Old `GARDEN_*` and `ZEN_GARDEN_*` prefixes are still supported with deprecation warnings.

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
