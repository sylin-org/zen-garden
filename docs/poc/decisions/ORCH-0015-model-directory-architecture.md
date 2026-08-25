---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-30
---

# ORCH-0015: Model Directory Architecture

**Date**: 2026-03-30
**Status**: Proposed
**Applies to**: `zen-garden-ai-orchestrator`, `garden-common`
**Depends on**: ORCH-0013 (AI Orchestrator), DETECT-0001 (Process Detection)

---

## Context

The AI orchestrator's model tracking is fragmented. Models are stored
in a flat `HashMap<String, ModelInfo>` populated by different paths:
local offerings enumerate via `profile_instance()`, cloud providers
register via `cloud_sync` task, and the dashboard reads from the same
map. This creates inconsistencies:

- Cloud models don't appear because the adapter chain is incomplete
- The same model on different stones has no unified identity
- Benchmark results are keyed by `(stone, model)` strings, not typed
- Routing cross-references `instances` and `models` separately
- Capability status requires a complex `build_capability_statuses()`
  function instead of a simple directory scan

---

## Decision

### Model FQN (MFQN)

Every model instance is identified by a four-part fully qualified name:

```
source|locator|model|parameters
```

| Field | Meaning | Examples |
|-------|---------|---------|
| `source` | Provider type | `ollama`, `anthropic`, `google`, `infinity`, `openedai-speech` |
| `locator` | Where specifically | `stone-azure-pool` (local), `prod` (cloud key name) |
| `model` | Model name | `qwen3.5:9b`, `claude-sonnet-4`, `all-MiniLM-L6-v2` |
| `parameters` | Optional variant | `Q4_K_M`, `F16`, omitted when not applicable |

**Examples:**
```
ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M
ollama|stone-quiet-lens|nomic-embed-text|F16
infinity|stone-azure-pool|all-MiniLM-L6-v2
openedai-speech|stone-azure-pool|tts-1
anthropic|prod|claude-sonnet-4
anthropic|dev|claude-sonnet-4
google|personal|gemini-2.0-flash
openai|work|gpt-4o
```

**Separator**: pipe `|` — does not appear in model names, location
names, or provider names. Unambiguous.

**Parameters optional**: 3 pipes = with parameters, 2 pipes = without.
Most cloud models and non-LLM services don't have meaningful parameters.

**Cloud key naming**: Users can configure multiple API keys per cloud
provider. Each key receives a user-assigned name (e.g., `prod`, `dev`,
`key-1`), normalized to lowercase-hyphenated. The locator field
identifies which key/account is used.

### ModelFqn Type

```rust
pub struct ModelFqn {
    pub source: String,
    pub locator: String,
    pub model: String,
    pub parameters: Option<String>,
}

impl ModelFqn {
    pub fn parse(input: &str) -> Result<Self, ModelFqnError>;
    pub fn fqn(&self) -> String;
    pub fn is_cloud(&self) -> bool;
    pub fn model_identity(&self) -> String; // "model" or "model|params"
    pub fn display_short(&self) -> String;  // "qwen3.5:9b (Q4_K_M)"
}
```

Custom serde: serializes as pipe-delimited string, deserializes via
`parse()`.

### ModelFilter — Partial MFQN for Pins and Queries

Pins and queries use partial FQNs as filters. Missing fields match
everything:

```
"qwen3.5:9b"                                → any source, any locator
"ollama|qwen3.5:9b"                         → this source, any locator
"ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M" → exact instance
```

```rust
pub struct ModelFilter {
    pub source: Option<String>,
    pub locator: Option<String>,
    pub model: Option<String>,
    pub parameters: Option<String>,
}

impl ModelFilter {
    pub fn parse(input: &str) -> Result<Self, ModelFqnError>;
    pub fn matches(&self, fqn: &ModelFqn) -> bool;
}
```

Parse by pipe count:
- 0 pipes → model only
- 1 pipe → source + model
- 2 pipes → source + locator + model
- 3 pipes → full FQN (exact match)

**Pin with filter = load balancing**: pinning `"qwen3.5:9b"` for the
chat capability means "use this model on whichever stone can serve it
best." The router finds all matching FQNs and load-balances among them.
Pinning a full FQN pins to one specific instance — no load balancing.

### Model Directory

The directory is the **single source of truth** for what models exist
and where they can be served. It replaces `AppState.models`.

```rust
pub struct ModelDirectory {
    /// Models keyed by model_identity (model name + optional parameters).
    entries: HashMap<String, ModelEntry>,
}

pub struct ModelEntry {
    /// Model name (e.g., "qwen3.5:9b")
    pub model: String,
    /// Quantization or variant (e.g., "Q4_K_M")
    pub parameters: Option<String>,
    /// Capability tags (chat, embed, vision, tools, think, speak, etc.)
    pub capabilities: Vec<Capability>,
    /// Specialization tags (ocr, reasoning, coding, embedding, etc.)
    pub specializations: Vec<String>,
    /// Model metadata (family, parameter count, context length, etc.)
    pub metadata: ModelMetadata,
    /// All instances that can serve this model.
    pub instances: Vec<ModelFqn>,
}
```

**Contribution flow**: every provider contributes through the same path:

```
Provider comes online
  → enumerate() returns models with capabilities
  → For each model:
      → Build ModelFqn(source, locator, model, parameters)
      → Directory.upsert(model_identity, fqn, capabilities, metadata)
      → If model_identity exists: add instance to entry
      → If new: create entry
```

Same path for Ollama, Infinity, OpenedAI Speech, Anthropic, Google.
No special cases.

**Removal**: when a provider goes offline, its FQNs are removed from
directory entries. Entries with zero instances are either removed or
kept as "known but unavailable" (for UI guidance).

### Benchmark Overlay

Benchmark results are keyed by full MFQN:

```rust
pub struct BenchmarkOverlay {
    results: HashMap<ModelFqn, BenchmarkEntry>,
}

pub struct BenchmarkEntry {
    /// Per-capability benchmark results.
    pub capabilities: HashMap<Capability, BenchmarkResult>,
    /// When this was last benchmarked.
    pub benchmarked_at: DateTime<Utc>,
}

pub struct BenchmarkResult {
    pub verdict: Verdict,           // Fast / Degraded / Vetoed / Blocked
    pub samples: Vec<Sample>,
    pub metrics: BenchmarkMetrics,  // provider-category-specific
}
```

**Provider-category-specific metrics:**

| Category | Metrics |
|----------|---------|
| Local inference (Ollama) | tok/s, cold_start_ms, VRAM usage |
| Local embedding (Infinity, Ollama) | embeddings/sec, dimensions |
| Local TTS (OpenedAI Speech) | TTFB ms, audio quality |
| Local STT (whisper.cpp) | real-time factor |
| Cloud inference (Anthropic, OpenAI, Google) | latency ms, cost per 1K tokens |
| Cloud TTS (ElevenLabs) | latency ms, cost per character |

```rust
pub enum BenchmarkMetrics {
    LocalInference {
        tokens_per_second: f64,
        cold_start_ms: u64,
        vram_bytes: Option<u64>,
    },
    LocalEmbedding {
        embeddings_per_second: f64,
        dimensions: u32,
    },
    CloudInference {
        latency_ms: u64,
        cost_per_1k_input_tokens: f64,
        cost_per_1k_output_tokens: f64,
    },
    // ... other categories
}
```

The `Verdict` is universal — all categories produce Fast/Degraded/Vetoed.
The raw metrics differ by category.

### Capability Status from Directory

Capability status is derived by scanning the directory + provider
registry. No special-case function:

```
GREEN (active):
  directory.models_with_capability("embed").count() > 0

YELLOW (needs_setup):
  directory has no models with "embed"
  BUT a registered provider declares Capability::Embed
  (provider installed, needs models pulled/configured)

GRAY (not_installed):
  no models AND no provider declares this capability
  (nothing installed that could serve this)
```

### Routing

```
Request: POST /api/chat { model: "chat", messages: [...] }

1. "chat" matches a capability name → capability-based routing
2. Look up pin for "chat" capability → "qwen3.5:9b" (ModelFilter)
3. Directory scan: all FQNs matching filter with chat capability
   → ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M
   → ollama|stone-quiet-lens|qwen3.5:9b|Q4_K_M
4. Benchmark overlay: azure-pool=Fast (47 tok/s), quiet-lens=Degraded
5. Priority gate: all local (0), cloud excluded
6. Load balance: azure-pool wins (Fast, loaded, queue=0)
7. Route to ollama adapter at stone-azure-pool

Request: POST /api/chat { model: "qwen3.5:9b", messages: [...] }

1. "qwen3.5:9b" is a model name → direct model routing
2. Directory lookup: "qwen3.5:9b" → find matching entries
3. Resolve best instance (same steps 4-7 above)

Request: POST /api/chat { model: "ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M" }

1. Matches MFQN pattern → exact instance routing
2. Route directly to that instance, no load balancing
```

### Instance Registry

`AppState.instances` remains separate from the directory. Instances
track operational state (health, VRAM, queue depth) that changes
rapidly. The directory tracks model availability that changes when
providers come online/offline. They reference each other:

- Directory FQN → maps to an instance via `source + locator`
- Instance → contributes models to the directory via `enumerate()`

---

## Implementation Plan

### Phase 1: ModelFqn + ModelFilter (`garden-common`)

- `ModelFqn` struct with parse/display/serde
- `ModelFilter` struct with parse/matches
- Unit tests for parsing, display, matching

### Phase 2: ModelDirectory (`ai-orchestrator`)

- `ModelDirectory` struct replacing `AppState.models`
- `upsert()`, `remove_provider()`, `models_with_capability()`, `find()`
- Provider contribution through `enumerate()` → directory upsert

### Phase 3: Benchmark Overlay

- `BenchmarkOverlay` keyed by `ModelFqn`
- Provider-category-specific metrics
- Verdict computation per category

### Phase 4: Routing Integration

- Router reads from directory + overlay
- Capability-based routing (`model: "chat"`)
- ModelFilter-based pin resolution
- Load balancing across matching instances

### Phase 5: Dashboard Integration

- Capability status from directory scan
- Model list from directory (unified, all providers)
- Pin UI stores ModelFilter strings

---

---

## Migration Notes

### What Exists Today (for implementer context)

The AI orchestrator crate is at `src/orchestrators/ai/`. Key files:

| File | Current Role | Changes Needed |
|------|-------------|----------------|
| `src/domain/types.rs` | `ModelInfo`, `ServiceInstance`, `Capability`, `OfferingKind` | Add `ModelFqn`, `ModelFilter`. `ModelInfo` becomes `ModelEntry`. |
| `src/app_state.rs` | `models: HashMap<String, ModelInfo>`, `instances: HashMap<String, ServiceInstance>` | Replace `models` with `ModelDirectory`. Keep `instances`. |
| `src/domain/routing.rs` | `select_instance()` cross-references models + instances + tiers | Rewrite to read from directory. Accept `ModelFilter` for pin resolution. |
| `src/domain/recommendation.rs` | Scores models per capability | Read from directory. Return `ModelFilter` not model name. |
| `src/api/proxy.rs` | Extracts model name, calls `select_instance()` | Handle capability names as model field. Resolve `ModelFilter` pins. |
| `src/api/dashboard.rs` | `ModelStatus` in `/api/status` response | Derive from directory. `CapabilityStatus` from directory scan. |
| `src/tasks/discovery.rs` | `profile_instance()` calls enumerate, registers models | Contribute to directory via `directory.upsert()`. |
| `src/tasks/cloud_sync.rs` | Registers cloud models separately | Same path as local: enumerate → directory.upsert(). |
| `src/offerings/cloud/types.rs` | `CloudProviderStore` with `cached_models` | Support multiple named keys per provider. `locator` = key name. |
| `src/offerings/cloud/openai.rs` | Only handles `OfferingKind::OpenAi` | Used for any OpenAI-compatible provider (Google, Cohere, etc.) |
| `src/domain/fitness.rs` | `GpuMatrix` keyed by `(stone, model)` | Replace with `BenchmarkOverlay` keyed by `ModelFqn`. |

### Files That Don't Change

- `src/catalog/traits.rs` — `Offering` trait, `ServiceModel` type (providers still enumerate through this)
- `src/offerings/ollama/` — Ollama adapter (enumerate returns models, unchanged)
- `src/offerings/infinity/` — Infinity adapter (same)
- `src/offerings/openedai_speech/` — OpenedAI Speech adapter (same)
- `src/infra/` — persistence, events (unchanged)
- `src/tasks/health_check.rs` — probes instances (unchanged, operates on instance registry)
- `src/tasks/gateway_announce.rs` — gateway registration (unchanged)

### Dashboard Impact

The React frontend at `src/orchestrators/ai/dashboard/` needs updates:

| Page | Change |
|------|--------|
| Overview | Capability status from directory scan (simpler logic) |
| CapabilityDetail | Model list from directory. All providers unified. |
| ServiceDetail | Model list filtered by source (provider). |
| CloudList/Detail | Support multiple keys per provider. |
| Settings | Pins store `ModelFilter` strings. |
| TryIt | Model selector from directory. |

### Current Test Coverage

80 domain tests exist (routing, demand, fitness, tiering, placement,
recommendation, reconciliation, lease, metrics, gpu_catalog). These
will need updating as the routing and fitness types change. The
algorithms (demand decay, VRAM tiering, placement bin-packing) are
unaffected — only their input types change.

### Cloud Provider Key Architecture

Currently: one API key per provider kind in `providers.json`.
```json
[{"kind": "google", "name": "google", "api_key": "...", "base_url": "..."}]
```

After: multiple named keys per provider.
```json
[
  {"kind": "google", "name": "personal", "api_key": "...", "base_url": "..."},
  {"kind": "openai", "name": "work", "api_key": "...", "base_url": "..."},
  {"kind": "openai", "name": "personal", "api_key": "...", "base_url": "..."}
]
```

The `name` field becomes the `locator` in MFQN. The `kind` field
determines which adapter handles it. Multiple entries with the same
`kind` but different `name` create separate instances in the
directory, each with their own models and benchmark scores.

The `OfferingRegistry` currently stores one adapter per `OfferingKind`.
With multiple keys, it needs one adapter per `(kind, name)` pair —
or a single adapter per kind that dispatches by locator at proxy time.

### Verification Plan

After implementation, verify against the live garden:

1. **Directory populated**: all Ollama models (22), Infinity (1),
   OpenedAI Speech (2), Google cloud (7) appear in the directory
2. **Capability status**: chat=GREEN, embed=GREEN, speak=GREEN,
   translate=YELLOW (provider exists, no models), imagine=GRAY
3. **Routing**: `model: "chat"` resolves to pinned/recommended model
4. **MFQN parsing**: `"ollama|stone-azure-pool|qwen3.5:9b|Q4_K_M".parse::<ModelFqn>()` works
5. **Pin with filter**: pinning `"qwen3.5:9b"` load-balances across
   3 stones. Pinning full FQN routes to one stone.
6. **Dashboard**: all providers' models visible in capability pages

---

## Consequences

### Positive

- Single source of truth for model availability
- All providers contribute through the same path
- Capability status is a simple directory scan
- MFQN provides typed, unambiguous model identity
- Partial MFQN pins enable load-balanced pinning
- Benchmark results properly keyed by instance
- Cloud and local models in the same catalog

### Negative

- Significant refactor of routing, benchmarking, and state management
- Migration of existing pin format (model name → ModelFilter)
- Two collections to keep in sync (directory + instance registry)

### Neutral

- `AppState.instances` unchanged (operational state)
- Existing proxy endpoints unchanged (model name in request body)
- Dashboard API shapes change but routes are the same
