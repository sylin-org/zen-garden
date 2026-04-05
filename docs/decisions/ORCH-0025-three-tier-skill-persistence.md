# ORCH-0025: Three-Tier Skill Persistence

- **Status**: Accepted
- **Date**: 2026-04-05
- **Deciders**: Leo, Claude
- **Supersedes**: None

## Context

The AI orchestrator stores skill definitions (skill.json + workflow templates) and
cached model files in its `/data` directory. When mapped to a Docker named volume,
this data survives container rebuilds but is destroyed by Docker Desktop storage
resets or system wipes. Meanwhile, the actual models are already provisioned on
ComfyUI instances (stone volumes) and survive independently.

A Docker wipe causes:
- All imported skill definitions lost
- Local model cache lost (multi-GB re-downloads)
- No way to recover without re-importing from CivitAI

The models on ComfyUI instances survive, but the orchestrator doesn't know what
skills existed or how to use them.

## Decision

Implement a three-tier persistence model where skill data exists in three
locations with cascading recovery:

### Tier 1: Host Filesystem (Speed)

Map orchestrator data to a host directory instead of a Docker named volume.
Data survives Docker wipes, resets, and reinstalls.

```
-v %LOCALAPPDATA%\zen-garden\ai-orchestrator:/data
```

### Tier 2: Stone Moss Storage (Durability)

Back up skill definitions to the stone's managed storage via the Moss API.
Data survives host machine rebuilds. Garden-wide durability.

- **Backup path**: `{stone}/api/v1/stone/storage/banks/{bank}/` → `zen-garden/ai/skills/`
- **Trigger**: On skill publish, delete, or periodic sync (5 min if dirty)
- **Restore**: On startup if local `skills/` is empty and stone is reachable

### Tier 3: ComfyUI Instance Co-location (Self-Healing)

Store skill definitions alongside models on each ComfyUI instance.
Each instance becomes a self-describing recovery source.

- **Push**: During provisioning, copy skill.json + workflow templates to
  `comfyui-models/zen-garden/skills/{skill-name}/` on the instance
- **Pull**: On startup recovery, scan instances for `zen-garden/skills/` and
  restore missing skill definitions
- **Model recovery**: Pull models from instances back to local cache
  (reverse of the provisioning push)

### Recovery Cascade

On startup, the orchestrator checks each tier in order:

```
1. Local /data/skills/ has content?     → normal boot
2. Empty? Check stone Moss storage      → pull skill definitions
3. Stone empty? Scan ComfyUI instances  → pull skills from zen-garden/skills/
4. Nothing anywhere?                    → fresh start, embedded skills only
```

## What Gets Persisted

| Data | Tier 1 (Host) | Tier 2 (Stone) | Tier 3 (ComfyUI) |
|------|---------------|----------------|-------------------|
| skill.json | yes | yes | yes |
| workflow templates | yes | yes | yes |
| Model cache | yes | no (too large) | already there |
| Config/secrets | yes | no | no |
| Metrics | yes | no | no |

## Sync Events

| Event | Tier 1 | Tier 2 | Tier 3 |
|-------|--------|--------|--------|
| Skill publish | immediate (source) | async push | on next provisioning cycle |
| Skill delete | immediate | async push | async clean |
| Startup (empty) | — | pull | pull if stone empty |
| Recovery mode | — | — | pull models to cache |

## Consequences

**Positive**:
- Data survives Docker wipes (Tier 1)
- Data survives machine rebuilds (Tier 2)
- Self-healing from any surviving ComfyUI instance (Tier 3)
- Models never need re-downloading from internet if any instance has them
- Progressive — each tier is independent, failures don't cascade

**Negative**:
- Slight provisioning overhead (pushing skill.json KB-sized files to instances)
- Stone storage dependency for Tier 2 (graceful degradation if unreachable)
- Skill definitions may temporarily diverge across tiers (eventual consistency)

## Implementation

### Files

| File | Purpose |
|------|---------|
| `start.bat` | Host bind mount (Tier 1) |
| `tasks/backup.rs` | Stone backup/restore + ComfyUI sync (Tier 2 & 3) |
| `skills/queue.rs` | Push skills to instances during provisioning (Tier 3) |
| `skills/recovery.rs` | Recovery cascade on startup (all tiers) |

### Migration

Existing Docker named volume data can be migrated by copying:
```
docker cp zen-garden-ai-orchestrator:/data/. %LOCALAPPDATA%\zen-garden\ai-orchestrator\
```
