# Sub-Capabilities Proposal

**Status:** Draft (Refined)
**Date:** 2026-02-02
**Updated:** 2026-02-02
**First Implementation:** Ollama (models)
**First PR Scope:** List only (add/remove in follow-up)

---

## 1. Concept

**Sub-capabilities** are runtime-discoverable features within offerings:

| Offering | Capability Type | Examples |
|----------|-----------------|----------|
| Ollama | model | llama2, mistral, nomic-embed-text |
| PostgreSQL | extension | pgvector, pg_trgm, postgis |
| Redis | module | ReJSON, RediSearch, RedisBloom |
| ChromaDB | collection | embeddings, documents |

**Core principle:** Each offering knows how to list, add, and remove its own capabilities via CLI commands that output a normalized format.

---

## 2. Architecture Decisions

### 2.1 Chirps Stay Lean

Chirps (UDP topology broadcasts) contain only offering names, NOT capabilities.

**Rationale:**
- Capabilities change frequently (user pulls model)
- Payload size would exceed MTU with many models
- Staleness/consistency issues with broadcast

### 2.2 On-Demand Querying

```
User: rake find ollama[llama2]
  → Rake queries tended Moss
  → Moss scans topology: "Who has ollama?"
  → Moss parallel-queries remote stones: GET /api/v1/stone/offerings/ollama/capabilities
  → Aggregate results, return matches
```

### 2.3 Capability Storage

Each stone **persists** capability state per offering:

- Stored in registry alongside ServiceInfo
- Refreshed on:
  - Moss boot (startup sync)
  - Offering install/wake
  - Explicit refresh request (`rake capabilities --refresh`)
  - Capability mutation (add/remove)
- Stale data acceptable for `rake list` counts (fast path)
- Fresh data required for search/queries (triggers refresh if TTL expired)

---

## 3. Query Syntax

### 3.1 Boolean Expression Grammar

```
query   := offering '[' expr ']'
expr    := term ('|' term)*           # OR
term    := factor (',' factor)*       # AND (binds tighter)
factor  := item | '(' expr ')'        # Grouping
item    := [a-zA-Z0-9_:.-]+
```

### 3.2 Examples

| Query | Meaning |
|-------|---------|
| `ollama[llama2]` | Has llama2 |
| `ollama[llama2,mistral]` | Has llama2 AND mistral |
| `ollama[llama2\|mistral]` | Has llama2 OR mistral |
| `ollama[(llama2,embed)\|mistral]` | (llama2 AND embed) OR mistral |

### 3.3 Type Prefixes

```
model:llama2        # Any offering with model
ext:pgvector        # Any offering with extension
mod:redisjson       # Any offering with module
cap:embedding       # Generic capability search
```

---

## 4. CLI Syntax

### 4.1 View Capabilities

```bash
rake capabilities ollama              # Local stone
rake capabilities ollama@gpu-server   # Specific stone
rake capabilities ollama --garden     # All stones
```

### 4.2 Search

```bash
rake find ollama[llama2]              # With capability filter
rake find model:llama2                # By capability type
```

### 4.3 Request Capabilities

**Decision:** Use `with` keyword (avoids shell escaping issues with brackets)

```bash
offer ollama with llama2              # Local
offer ollama with llama2,mistral      # Multiple
offer ollama with llama2 at gpu-server # Remote
```

### 4.4 Remove Capabilities

```bash
rake lift ollama model:llama2         # Remove specific capability
```

### 4.5 Integration with `rake list`

```
ollama              [thriving]   ai        4 models
postgresql          [thriving]   data      3 extensions
redis               [dormant]    cache     2 modules
```

---

## 5. API Endpoints

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/offerings/:name/capabilities` | List local capabilities |
| POST | `/api/v1/stone/offerings/:name/capabilities` | Add capability |
| DELETE | `/api/v1/stone/offerings/:name/capabilities/:type/:item` | Remove capability |
| GET | `/api/v1/garden/offerings/:name/capabilities` | Garden-wide aggregation |
| GET | `/api/v1/garden/offerings/search?q=ollama[llama2]` | Search by capability |

### 5.1 Response Format

```json
{
  "data": {
    "offering": "ollama",
    "capabilities": [{
      "type": "model",
      "items": [
        {"name": "llama2:7b", "size_bytes": 3826793472, "metadata": {...}},
        {"name": "mistral:7b", "size_bytes": 3800000000}
      ],
      "discovered_at": "2026-02-02T14:30:00Z"
    }]
  }
}
```

---

## 6. Normalized Capability Format

All capability commands output this format for the single parser:

### 6.1 CapabilityItem (Rust)

```rust
pub struct CapabilityItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}
```

### 6.2 Output Contracts

**LIST:** JSON array of CapabilityItem
```json
[{"name": "llama2", "size_bytes": 3826793472}, {"name": "mistral"}]
```

**ADD/REMOVE:** JSON result object
```json
{"success": true}
{"success": false, "error": "Model not found"}
```

**SUMMARY:** Single number (for `rake list` counts)
```
4
```

---

## 7. Manifest Schema

### 7.1 CLI One-Liner Approach (No External Dependencies)

**Key decision:** Commands call Moss helper endpoints for JSON transformation. No jq/awk required.

**Benefits:**
- Zero external dependencies (minimal environment assumption)
- Moss controls transformation logic
- Testable via curl to helper endpoints
- Platform-agnostic (same curl everywhere)

### 7.2 Moss Helper Endpoints

Moss exposes transformation helpers at `/api/v1/helpers/`:

| Endpoint | Purpose |
|----------|---------|
| `POST /api/v1/helpers/json-transform` | Apply JSONPath/transformation to input |
| `POST /api/v1/helpers/json-extract` | Extract fields from JSON |

**Request:**
```json
{
  "input": {"models": [{"name": "llama2", "size": 123}]},
  "transform": {
    "items_path": ".models",
    "fields": {
      "name": ".name",
      "size_bytes": ".size"
    }
  }
}
```

**Response:**
```json
[{"name": "llama2", "size_bytes": 123}]
```

### 7.3 Managed vs Adopted Commands

Manifests support both modes. Moss selects based on offering's current mode.

```yaml
list:
  commands:
    managed:
      linux: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
      windows: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
    adopted:
      linux: "curl -s http://localhost:{{port}}/api/tags"
      windows: "curl -s http://localhost:{{port}}/api/tags"
```

### 7.4 Ollama Manifest

```yaml
# src/moss/embedded/manifests/sw/ai/ollama.capabilities.yaml
version: "1"
offering: ollama

capabilities:
  - type: model
    display:
      singular: model
      plural: models
    mutability: hot

    list:
      # Step 1: Fetch raw data (mode-specific)
      commands:
        managed:
          linux: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
          windows: "docker exec {{container_name}} curl -s http://localhost:11434/api/tags"
        adopted:
          linux: "curl -s http://localhost:{{port}}/api/tags"
          windows: "curl.exe -s http://localhost:{{port}}/api/tags"

      # Step 2: Transform via Moss helper (no jq needed)
      transform:
        items_path: ".models"
        fields:
          name: ".name"
          size_bytes: ".size"
          metadata:
            family: ".details.family"
            quantization: ".details.quantization_level"
            modified: ".modified_at"

      output: json
      timeout_secs: 10

    add:
      commands:
        managed:
          linux: "docker exec {{container_name}} ollama pull {{item}}"
          windows: "docker exec {{container_name}} ollama pull {{item}}"
        adopted:
          linux: "ollama pull {{item}}"
          windows: "ollama.exe pull {{item}}"
      # Result parsing handled by Moss (exit code + output)
      timeout_secs: 7200
      progress:
        pattern: "(\\d+)%"

    remove:
      commands:
        managed:
          linux: "docker exec {{container_name}} ollama rm {{item}}"
          windows: "docker exec {{container_name}} ollama rm {{item}}"
        adopted:
          linux: "ollama rm {{item}}"
          windows: "ollama.exe rm {{item}}"
      timeout_secs: 60

    summary:
      transform:
        count_path: ".models | length"
      format: "{{count}} models"
```

### 7.3 Mutability Modes

| Mode | Meaning | Example |
|------|---------|---------|
| `hot` | Changes immediate | Ollama models, PG extensions |
| `warm` | Requires restart | Some configs |
| `cold` | Requires rebuild | Redis modules |

For `cold` mutability, `add`/`remove` are marked unavailable with guidance:

```yaml
add:
  available: false
  reason: "Redis modules require container rebuild. Use redis-stack image instead."
```

---

## 8. Implementation Order

### PR 1: List Capabilities (Minimal)

| Step | Component | Files |
|------|-----------|-------|
| 1 | CapabilityItem types | `common/src/types.rs` |
| 2 | JSON transform helper | `moss/src/api/v1/helpers.rs` |
| 3 | Helper endpoint route | `moss/src/bootstrap/router.rs` |
| 4 | Manifest schema | `common/src/manifests/capabilities.rs` |
| 5 | Capability executor (list only) | `moss/src/domain/capabilities/executor.rs` |
| 6 | Ollama manifest (list only) | `moss/embedded/manifests/sw/ai/ollama.capabilities.yaml` |
| 7 | List endpoint | `moss/src/api/v1/capabilities.rs` |
| 8 | Persistence in registry | `moss/src/domain/registry.rs` |
| 9 | `rake capabilities` command | `rake/src/commands/capabilities.rs` |
| 10 | Unit tests | `moss/src/domain/capabilities/tests.rs` |

### PR 2: Add/Remove Capabilities

| Step | Component | Files |
|------|-----------|-------|
| 11 | Add endpoint | `moss/src/api/v1/capabilities.rs` |
| 12 | Remove endpoint | Same file |
| 13 | `offer with` syntax | `rake/src/commands/offering/mod.rs` |
| 14 | `rake lift` extension | `rake/src/commands/lift.rs` |
| 15 | Progress streaming | `moss/src/api/v1/capabilities.rs` |
| 16 | Integration tests | `probe/src/tests/capabilities.rs` |

### Phase 2: Search & Garden-Wide

| Step | Component |
|------|-----------|
| 12 | Query parser (boolean expressions) |
| 13 | Garden-wide capability aggregation |
| 14 | `rake find` with capability filter |
| 15 | `rake list` capability counts |

### Phase 3: Additional Offerings

| Offering | Notes |
|----------|-------|
| PostgreSQL | SQL via docker exec, extensions |
| ChromaDB | HTTP API, collections |
| Redis | Cold mutability, modules |

---

## 9. Caveats & Open Issues

### 9.1 Security (Deferred)

Remote capability mutations (`offer ollama with X at remote-stone`) have no authentication in Phase 1.

**Decision:** Defer to Phase 2 when Pond (mTLS) is implemented. For now, trust stones in the garden.

### 9.2 Partial Failures

Garden-wide queries may have partial results (some stones timeout).

**Response must include:**
```json
{
  "stones_queried": 5,
  "stones_responded": 3,
  "unreachable": ["stone-04", "stone-05"]
}
```

### 9.3 Capability Name Validation

Prevent command injection. Valid pattern:
```
^[a-zA-Z0-9_:./-]+$
```
Max length: 128 chars

### 9.4 Timeouts

| Operation | Default | Max |
|-----------|---------|-----|
| List | 10s | 30s |
| Add (models) | 7200s | 14400s |
| Remove | 60s | 300s |
| Garden query (per stone) | 5s | 10s |
| Garden query (total) | 15s | 30s |

### 9.5 Scalability

| Scale | Assessment |
|-------|------------|
| 10 stones | No issues |
| 50 stones | Add bounded parallelism (max 20 concurrent) |
| 100+ stones | Requires Lantern capability indexes (future) |

### 9.6 Progress Reporting

Long operations (model pulls) stream progress. Two modes:

1. **Poll:** Job ID returned, client polls `/api/v1/jobs/:id`
2. **Stream:** SSE endpoint `/api/v1/jobs/:id/stream`

Progress extraction via regex pattern in manifest.

---

## 10. Test Plan

### 10.1 Ollama Lifecycle Test

```
1. LIST    → rake capabilities ollama
2. ADD     → offer ollama with tinyllama
3. VERIFY  → rake capabilities ollama (includes tinyllama)
4. SEARCH  → rake find ollama[tinyllama]
5. REMOVE  → rake lift ollama model:tinyllama
6. VERIFY  → rake capabilities ollama (excludes tinyllama)
```

### 10.2 Edge Cases

- [ ] Capability already exists (ADD should succeed, idempotent)
- [ ] Capability not found (REMOVE should return info, not error)
- [ ] Offering not running (LIST should fail gracefully)
- [ ] Remote stone offline (garden query returns partial)
- [ ] Invalid capability name (rejected with validation error)
- [ ] Concurrent adds (Ollama handles internally)

---

## 11. File Locations

```
src/
├── common/src/
│   ├── types.rs                    # Add CapabilityItem, CapabilityCollection
│   └── manifests/
│       └── capabilities.rs         # NEW: Manifest schema
├── moss/src/
│   ├── domain/
│   │   ├── registry.rs             # MODIFY: Add capabilities to ServiceInfo persistence
│   │   └── capabilities/
│   │       ├── mod.rs              # NEW
│   │       ├── executor.rs         # NEW: Run commands, call transform
│   │       ├── transform.rs        # NEW: JSON transform logic (used by helper endpoint)
│   │       └── parser.rs           # NEW: Query expression parser (PR 2+)
│   ├── api/v1/
│   │   ├── helpers.rs              # NEW: /api/v1/helpers/json-transform endpoint
│   │   └── capabilities.rs         # NEW: Capability endpoints
│   └── bootstrap/router.rs         # Add routes
├── moss/embedded/manifests/sw/
│   └── ai/
│       └── ollama.capabilities.yaml # NEW
├── rake/src/commands/
│   ├── capabilities.rs             # NEW: rake capabilities
│   └── offering/mod.rs             # Extend: offer with syntax (PR 2)
└── probe/src/tests/
    └── capabilities.rs             # NEW: Integration tests
```

---

## 12. Dependencies

**No new crate dependencies required. No external tools required.**

Uses existing:
- `serde_json` for JSON parsing and transformation
- `tokio::process` for command execution
- `reqwest` for HTTP (garden queries)
- `jsonpath_lib` or inline implementation for JSONPath extraction

**Explicitly NOT required:**
- ~~jq~~ (transformation handled by Moss helper endpoints)
- ~~awk/sed~~ (not needed)
- ~~PowerShell JSON cmdlets~~ (curl output piped to Moss)

---

## 13. Related Documents

- [UNIFIED-OFFERING-MODEL.md](UNIFIED-OFFERING-MODEL.md) - Offering structure
- [ADR OFFER-0001](../references/decisions/OFFER-0001-offering-taxonomy-tags.md) - Taxonomy
- [SUB-CAPABILITIES-IMPLEMENTATION-PLAN.md](SUB-CAPABILITIES-IMPLEMENTATION-PLAN.md) - Superseded by this doc

---

## 14. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-02-02 | Chirps stay lean | Payload size, staleness |
| 2026-02-02 | On-demand querying | Fresh data, acceptable latency |
| 2026-02-02 | `with` keyword for offer | Avoids shell escaping |
| 2026-02-02 | AND binds tighter than OR | Mathematical convention |
| 2026-02-02 | CLI one-liners for transform | Testable, flexible, simple |
| 2026-02-02 | Security deferred | Get it working first |
| 2026-02-02 | Ollama first | Simple HTTP, high value |
| 2026-02-02 | No jq dependency | Minimal environment assumption; Moss helper endpoints handle JSON transform |
| 2026-02-02 | Support managed + adopted commands | Different execution contexts per mode |
| 2026-02-02 | Port from ServiceInfo | Must reflect real tracked port |
| 2026-02-02 | Persist capabilities | Store in registry, refresh on boot/events |
| 2026-02-02 | PR 1 = list only | Ship incrementally, validate approach |
