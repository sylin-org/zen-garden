# Zen Garden - Utilities & Constants Reference

Existing utilities - don't reinvent these.

---

## Formatting (`common/src/utils.rs`)

| Function | Output |
|----------|--------|
| `format_bytes(u64)` | "1.00 GB" |
| `format_uptime(u64)` | "1h 30m" |
| `format_bytes_precision(u64, usize)` | Custom precision |
| `format_bytes_short(u64)` | "1G" |
| `format_memory_mb(u64)` | "1024 MB" |

---

## TUI Primitives (`common/src/ui/rendering.rs`)

| Function | Purpose |
|----------|---------|
| `terminal_dimensions()` | `(cols, rows)` with (80, 24) fallback |
| `visible_length(&str)` | ANSI-aware string length |
| `pad_visible(&str, width)` | ANSI-aware right-pad to width |
| `truncate_visible(&str, max)` | ANSI-aware truncation preserving escape codes |
| `format_separator(label, cols, unicode)` | Horizontal divider `" ──────"` or `" label ──────"` |
| `format_wall_clock()` | Current time as `"HH:MM:SS"` |
| `extract_sse_time(&Value)` | ISO timestamp → `"HH:MM:SS"` from SSE event JSON |
| `format_gauge(label, value, width, color)` | `"CPU [====----] 42%"` bar |
| `format_net_rate(bytes_per_sec)` | `"1.4 MB/s"` |

---

## Paths (`common/src/constants/paths.rs`)

| Function | Linux | Windows |
|----------|-------|---------|
| `data_dir()` | `/var/lib/zen-garden` | `.zen-garden` |
| `shared_data_dir()` | `/var/lib/zen-garden` | `{ProgramData}\zen-garden` |
| `config_dir()` | `/etc/zen-garden` | `.zen-garden` |
| `companions_dir()` | `/usr/local/bin/companions/` | varies |
| `topology_dir()` | shared_data subdir | shared_data subdir |
| `harvest_dir()` | data subdir | data subdir |
| `stored_dir()` | data subdir | data subdir |
| `stone_home()` | `/home/stone` | varies |
| `first_run_flag()` | data subdir | data subdir |

---

## Network Ports (`common/src/constants/mod.rs`)

| Constant | Value | Purpose |
|----------|-------|---------|
| `MOSS_HTTPS` | 7183 | Pond mTLS HTTPS |
| `DISCOVERY_UDP` | 7184 | mDNS/multicast |
| `MOSS_HTTP` | 7185 | Stone daemon |
| `LANTERN_HTTP` | 7186 | Service registry |
| `COMPANION_PORT_BASE` | 7187 | First companion port |
| `COMPANION_PORT_MAX` | 7199 | Last companion port |

---

## Timeouts (`common/src/constants/timeouts.rs`)

| Constant | Value | Purpose |
|----------|-------|---------|
| `DISCOVERY_TIMEOUT_MS` | 3000 | Discovery wait |
| `HTTP_REQUEST_TIMEOUT_MS` | 30000 | API calls |
| `COMPANION_COMMAND_TIMEOUT_MS` | 5000 | Command forwarding |

---

## Limits (`common/src/constants/limits.rs`)

| Constant | Value |
|----------|-------|
| `MAX_OFFERING_NAME_LENGTH` | 64 |
| `MAX_SERVICES_PER_STONE` | 100 |

---

## IDs (`common/src/utils/ids.rs`)

```rust
generate_guidv7()           // "01234567-89ab-7cde-8f01-234567890abc"
generate_id("job")          // "job-01234567"
```

---

## Utils Modules (`common/src/utils/`)

| Module | Key Functions |
|--------|---------------|
| `env.rs` | `EnvConfig` typed accessors for `ZG_*` vars |
| `fs.rs` | `ensure_dir()`, `read_file()`, `write_file()` async |
| `platform.rs` | `PlatformPaths` trait |
| `json.rs` | `parse<T>()`, `stringify<T>()` |
| `strings.rs` | `truncate()`, `to_kebab_case()`, `to_snake_case()`, `shorten_stone_name()` |
| `validation.rs` | `validate_name()`, `validate_port()`, `validate_url()` |

---

## Shared Types

### Companion (`common/src/companion.rs`)
- `CompanionCommandRequest` - args: Vec<String>
- `CompanionCommandResponse` - success, output
- `CompanionManifest` - name, version, description, port

### Command Manifest (`common/src/command_manifest/`)
- `CommandManifest` - Full companion command manifest
- `CommandParameter` - name, type, required, description
- `check_dump_commands()` - Helper for companion binaries

### Offerings (`common/src/offerings.rs`)
- `OfferingFqn` - Typed FQN (see [offering-fqn spec](../../docs/specs/offering-fqn.md))
  - Constructors: `new()`, `with_instance()`, `adopted()`, `image_direct()`, `parse()`
  - Fields: `source`, `offering`, `instance`, `image_ref`
  - `fqn()` / `Display` → canonical string (`ollama::dev`)
  - `encoded_for_container()` → Docker-safe name (`ollama--dev`)
  - Custom serde: serializes as string, deserializes with legacy normalization
- `OfferingSource` - Image, Repo(String), Oci
- `TaxonomyDictionary` - Synonym mapping
- `OfferingSearchRequest/Response/Result`

---

## Environment Variables

**Prefix**: `ZG_` (legacy `GARDEN_*` supported with warnings)

| Category | Variables |
|----------|-----------|
| Paths | `ZG_DATA_DIR`, `ZG_CONFIG_DIR`, `ZG_HARVEST_DIR`, `ZG_STAGING_DIR`, `ZG_STORED_DIR` |
| Stone | `ZG_STONE_NAME`, `ZG_STONE_HOST`, `ZG_STONE_HOME`, `ZG_STONE_USER` |
| Endpoints | `ZG_STONE` (skip discovery), `ZG_LANTERN` |
| Resolution | `ZG_PARTITION`, `ZG_INSTANCE` |
| Flags | `ZG_NO_COLOR`, `ZG_UNICODE`, `ZG_QUIET`, `ZG_CONTAINER` |
| External | `CUDA_PATH`, `INTEL_OPENVINO_DIR` |

---

## Binary Names

```
MOSS_BINARY    = "garden-moss"
RAKE_BINARY    = "garden-rake"
LANTERN_BINARY = "garden-lantern"
MOSS_SERVICE   = "garden-moss.service"
```

---

## Key Infra Modules

| Module | Purpose |
|--------|---------|
| `moss/src/app_state.rs` | Shared state via `Arc<RwLock<T>>` |
| `moss/src/infra/persistence.rs` | `load_json<T>()`, `save_json<T>()` atomic |
| `moss/src/infra/docker.rs` | Bollard wrapper |
| `moss/src/infra/manifests/` | YAML frontmatter loader |
| `common/src/client.rs` | `ApiClient` with 30s timeout |
