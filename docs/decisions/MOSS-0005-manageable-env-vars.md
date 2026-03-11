# MOSS-0005: Manifest-Declared Manageable Environment Variables

## Status
Proposed

## Context
Orchestrators need to read and write environment variables on services running
on stones.  The Ollama orchestrator, for example, discovers `OLLAMA_NUM_PARALLEL`
during profiling and would benefit from being able to _set_ it to tune
parallelism per stone.

Today Moss can read env vars for **managed** (Docker) services via container
inspection (`GET /api/v1/stone/services/{service}/env`), but:

1. **Adopted bare-metal services return empty** — Moss has no mechanism to read
   host-level env vars for native installations.
2. **No write path exists** — there is no API to update env vars on any service
   type.
3. **No safety boundary** — without a declaration of which vars are tunable, an
   unrestricted write API risks breaking services (e.g., overwriting
   `NVIDIA_VISIBLE_DEVICES` or `PATH`).

The adopted Ollama manifest already contains ad-hoc env var manipulation in its
`connectivity.ensure` commands (e.g., `setx /M OLLAMA_HOST` on Windows,
systemd overrides on Linux).  This ADR formalises that pattern into a
general-purpose, manifest-declared mechanism.

## Decision

### 1. Frontmatter declares manageable vars

The `manageable_env` section lives in the offering's `.frontmatter.json` file.
Frontmatter is the offering-level identity file — shared across all modes
(managed, adopted, borrowed).  This is the correct location because the same
env vars are meaningful regardless of how the service is deployed.

```json
{
  "name": "ollama",
  "description": "Ollama local LLM runtime (MIT licensed)",
  "category": "ai",
  "tags": ["ai", "llm", "inference"],
  "port": 11434,
  "manageable_env": {
    "restart_required": true,
    "vars": [
      "OLLAMA_NUM_PARALLEL",
      "OLLAMA_MAX_LOADED_MODELS",
      "OLLAMA_FLASH_ATTENTION"
    ]
  }
}
```

- `vars` is an allowlist.  Any variable not listed is rejected on write.
- `restart_required` indicates whether changes take effect only after a service
  restart.  Defaults to `true` if omitted (safe default).
- The frontmatter is **platform-agnostic** — it declares _what_ is manageable,
  not _how_ it is applied.

#### Rust representation

`FrontmatterFile` (in `common/src/manifests/offering.rs`) gains an optional
field.  The parsed result flows into `Offering` as a cross-mode field alongside
`metadata`, `compatibility`, `connection`, and `coordination`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManageableEnv {
    #[serde(default = "default_true")]
    pub restart_required: bool,
    pub vars: Vec<String>,
}
```

### 2. Moss resolves the "how" per platform and service mode

| Mode | Platform | Read | Write |
|------|----------|------|-------|
| **Managed** (container) | any | Container inspect | Recreate container with updated env |
| **Adopted** (bare metal) | Linux | Parse `/etc/default/{service}` or systemd env override | Write to env file + `systemctl daemon-reload` |
| **Adopted** (bare metal) | Windows | Read machine-scoped env var (registry) | `setx /M VAR VALUE` (registry-backed) |
| **Adopted** (bare metal) | macOS | `launchctl getenv` / process env | `launchctl setenv` |

The platform-specific logic lives in Moss's infra layer, not in the manifest.

#### Restart policy

Moss **never forces** a restart after writing env vars.  Instead:

- The write response includes `"restart_required": true` when the manifest
  declares it, signalling the caller that the change will not take effect
  until the service is restarted.
- The caller (orchestrator, CLI, dashboard) decides when and whether to
  restart, allowing it to coordinate with traffic draining, lease expiry,
  or user confirmation.
- For adopted services, Moss detects the restart mechanism per platform
  (Windows Service, systemd unit, launchd) and exposes it through the
  existing `POST .../restart` endpoint.

### 3. API surface

#### Read (enhanced existing endpoint)
`GET /api/v1/stone/services/{service}/env`

Currently returns the full env map for managed containers and empty `{}` for
adopted services.  Enhanced behaviour:

- **Managed**: unchanged (returns all container env vars).
- **Adopted**: reads only the vars listed in `manageable_env` using the
  platform-specific mechanism.  Returns:
  ```json
  {
    "data": {"OLLAMA_NUM_PARALLEL": "4"},
    "manageable": ["OLLAMA_NUM_PARALLEL", "OLLAMA_MAX_LOADED_MODELS", "OLLAMA_FLASH_ATTENTION"]
  }
  ```
  The `manageable` field tells the caller which vars are writable, enabling
  UI generation without needing a separate manifest query.

#### Write (new endpoint)
`PATCH /api/v1/stone/services/{service}/env`

```json
{"OLLAMA_NUM_PARALLEL": "4", "OLLAMA_FLASH_ATTENTION": "1"}
```

Behaviour:
1. Locate the offering manifest for the service.
2. Validate every key in the request against `manageable_env.vars`.
   Reject the entire request if any key is not in the allowlist (400).
3. Apply the changes using the platform-appropriate mechanism.
4. Return the result:
   ```json
   {
     "applied": {"OLLAMA_NUM_PARALLEL": "4", "OLLAMA_FLASH_ATTENTION": "1"},
     "restart_required": true
   }
   ```

A value of `null` removes the variable (reverts to default).

### 4. Orchestrator workflow

The Ollama orchestrator already calls `GET .../env` during discovery.
With this ADR it can also:

1. Read `OLLAMA_NUM_PARALLEL` from each stone at discovery time (already works
   for managed; now also works for adopted).
2. Decide the optimal parallelism based on VRAM, model sizes, and workload.
3. Call `PATCH .../env` to apply the setting.
4. Inspect `restart_required` in the response and issue
   `POST .../restart` when appropriate (e.g., after draining traffic).

## Consequences

### Positive
- **Safe by default** — only manifest-declared vars are writable.
- **Platform-agnostic manifests** — offering authors declare intent in
  frontmatter, Moss handles platform differences in its infra layer.
- **Generalises existing ad-hoc patterns** — the connectivity `ensure` commands
  that already manipulate `OLLAMA_HOST` via `setx` / systemd overrides become
  a special case of this mechanism.
- **Orchestrator-friendly** — any orchestrator can tune its offering's runtime
  parameters through a single, uniform API.
- **No forced restarts** — callers retain control over service lifecycle,
  enabling coordinated restarts during maintenance windows.

### Negative
- **Adopted bare-metal read requires platform code** — reading env vars from
  systemd units, Windows registry, or launchd is non-trivial and
  platform-specific.  Each platform needs a dedicated infra module.
- **Windows service detection** — Ollama on Windows can run as a Windows
  Service or as a user-mode process.  Moss must detect which and handle both
  for restart signalling.

### Neutral
- Existing `GET .../env` for managed containers is unchanged.
- Manifests without `manageable_env` are unaffected — the write endpoint
  returns 400 ("no manageable variables declared").
- The `manageable` field in the GET response enables future dashboard UI
  for env var editing without additional manifest queries.
