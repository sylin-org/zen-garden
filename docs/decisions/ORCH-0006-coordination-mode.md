---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-20
---

# ORCH-0006: Coordination Mode — Opt-In Election for Stateful Offerings

**Date**: 2026-02-20
**Status**: Accepted
**Applies to**: `garden-common`, `moss`
**Supersedes**: The implicit `replicable: bool` field introduced in ORCH-0001

## Context

ORCH-0001 introduced multi-instance coordination with Primary/Dormant roles
and a `replicable: bool` field on offering manifests.  In practice this field
was never set to `false` by any manifest — every offering (stateless or
stateful) silently opted in to election.  This caused stateless services like
Ollama to participate in Primary/Dormant role assignment, which is meaningless
for services that own no persistent state.

### Problems with `replicable: bool`

| Issue | Impact |
|-------|--------|
| **Unsafe default** (`true`) | New manifests auto-enroll in election with no author action |
| **Misleading name** | "Replicable" implies data replication; what it controls is role election |
| **Binary, not extensible** | Cannot express future coordination strategies (consensus, sharded) |
| **Dead code** | `AdoptedFile` parser lacked the field entirely — adopted offerings were hardcoded to `true` |

## Decision

Replace `replicable: bool` with a `CoordinationMode` enum:

```rust
#[derive(Default)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationMode {
    /// Each instance operates independently. No election, no roles.
    /// Safe default for most offerings.
    #[default]
    Independent,
    /// One Primary, rest Dormant. Election determines the active writer.
    /// For stateful services (databases, message brokers, etc.).
    Elected,
}
```

### Key design choices

1. **Safe default**: `Independent`. Adding a new manifest never triggers
   unexpected election behavior.

2. **Declarative opt-in**: Stateful offerings declare `coordination: elected`
   in their manifest (frontmatter JSON, `.manifest.yaml`, or `.adopted.yaml`).

3. **No backward compatibility shims**: This is a breaking change.  All
   manifests and persisted state are regenerated.  The old `replicable` field
   is removed entirely.

4. **Enum, not bool**: Future coordination strategies (consensus quorum,
   sharded partitioning) become new enum variants — no schema change needed.

### What declares `coordination: elected`

Stateful offerings with persistent data that benefits from Primary/Dormant
failover:

- **Databases**: mongodb, postgresql, mariadb, redis, couchbase, elasticsearch,
  opensearch, sqlserver
- **Messaging**: rabbitmq, nats
- **Time series**: influxdb
- **Vector DBs**: milvus, weaviate
- **Secrets**: vault
- **Object storage**: minio

### What stays `independent` (default)

Everything else — inference engines (ollama, ollama-cpu), proxies, search UIs,
dashboards, monitoring tools.

## Consequences

### Positive

- Ollama and other stateless services no longer participate in election
- New manifest additions are safe by default
- The enum is extensible for future coordination strategies
- Manifest authors must make an explicit, conscious choice to opt in

### Negative

- Breaking change: persisted `replicable` fields in JSON become unrecognized
  (offerings persist `orchestration` state separately, so this is cosmetic)
- All consuming code updated in a single pass

## Implementation

### Changed files

| File | Change |
|------|--------|
| `garden_common::types` | Add `CoordinationMode` enum |
| `garden_common::manifests::offering` | Replace `replicable: bool` → `coordination: CoordinationMode` on `Offering`, `ManifestFile`, `AdoptedFile`, `FrontmatterFile` |
| `moss::domain::offerings` | Replace on `CompiledOffering` |
| `moss::domain::placement` | Update remote view default |
| `moss::tasks::offering_orchestration` | Backfill checks `Elected` instead of `replicable` |
| `moss::tasks::job_executors` | Post-deploy role assignment checks `Elected` |
| `moss::infra::embedded` | Update embedded adopted loader |
| `*.frontmatter.json` | Stateful offerings get `"coordination": "elected"` |

## Related

- ORCH-0001: Replant Ceremony (introduced `replicable`)
- ORCH-0005: CPU Inference Tier (stateless Ollama offerings)
