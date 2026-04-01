---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0019: Skill Provisioning and Offering Volume API

**Date**: 2026-04-01
**Status**: Accepted
**Depends on**: ORCH-0018 (Skills and Workflow API)

---

## Problem

The AI orchestrator can discover ComfyUI instances and identify what skills are
possible (from installed node types), but the required model files may not be
present. A fresh ComfyUI instance has zero models — the `image.upscale` skill
needs at least one upscale model (e.g., `RealESRGAN_x4plus.pth`, ~64MB).

There is no mechanism for the orchestrator to place files in an offering's
bind-mounted volumes on a remote stone. Moss manages these volumes but has no
HTTP endpoint for writing to them.

---

## Decision

### 1. Offering Volume API (Moss)

New Moss endpoints for reading and writing files in offering volumes:

```
PUT  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path
GET  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path
HEAD /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path
```

**PUT**: Accepts binary body, writes to `{volumes_dir}/{fqn_encoded}/{volume}/{path}`.
Creates intermediate directories. Returns `201 Created` or `204 No Content`.

**GET**: Returns the file binary with appropriate `Content-Type`. Returns `404`
if the file doesn't exist.

**HEAD**: Returns `200` with `Content-Length` if the file exists, `404` otherwise.
Used for existence checks without transferring the full file.

Path traversal is validated — `..` segments are rejected.

**Security**: These endpoints are local to the stone (port 7185). The offering
FQN is validated against the manifest registry. Only volumes declared in the
offering's manifest can be written to.

### 2. Skill Lifecycle State Machine

Each skill goes through a lifecycle managed by the orchestrator:

```
DISCOVERED → INITIALIZING → PROVISIONING → LIVE
                                          → DEGRADED
```

**DISCOVERED**: Skill definition parsed from built-in template or imported
workflow. Required models identified. Not yet usable.

**INITIALIZING**: Orchestrator downloads required model files from upstream
sources (GitHub releases, HuggingFace, etc.) to its local cache at
`{orchestrator_data_dir}/skill-cache/{model_type}/{filename}`. One-time per
model file, shared across all stones.

**PROVISIONING**: For each ComfyUI instance:
1. Check if model exists via `HEAD /api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}`
2. If missing, transfer via `PUT` with the cached file bytes

**LIVE**: At least one instance has all required models. Skill appears in:
- `GET /v1/skills` listing
- `GET /v1/skills/{skill}/form` for TryIt UI
- `POST /v1/workflows/run` accepts requests
- Routing selects only provisioned instances

**DEGRADED**: Some instances provisioned, others not. Skill is live but routes
only to ready instances. New instances are provisioned in the background.

### 3. Per-Instance Readiness

The orchestrator tracks provisioning status per instance:

```rust
struct InstanceSkillState {
    models_present: HashSet<String>,  // confirmed via HEAD
    models_required: HashSet<String>, // from skill definition
    ready: bool,                      // all required models present
}
```

On each discovery cycle:
1. Enumerate skills from built-in definitions
2. For each skill, check if locally cached (INITIALIZING → done?)
3. For each ComfyUI instance, check per-model readiness (HEAD)
4. Push missing models (PUT)
5. Update routing to include/exclude instances based on readiness

### 4. Orchestrator Cache Layout

```
{orchestrator_data_dir}/
  skill-cache/
    upscale_models/
      RealESRGAN_x4plus.pth          (64MB, downloaded once)
      RealESRGAN_x4plus_anime_6B.pth (17MB, downloaded once)
    checkpoints/
      ...future...
```

Models are downloaded once to the orchestrator's data directory and pushed to
all ComfyUI instances. When a new stone joins, the orchestrator already has the
bits — no re-download from upstream.

---

## Consequences

- Skills are fully automated: orchestrator handles model download, distribution,
  and readiness tracking without manual intervention.
- The Moss volume API is generic — usable for any offering's volumes, not just
  ComfyUI. Future offerings can use it for config files, model weights, etc.
- The orchestrator's local cache means model files are downloaded once from
  upstream, then distributed locally. Network-efficient for multi-stone gardens.
- HEAD checks are lightweight — no file transfer for readiness verification.
- Path traversal protection prevents writes outside the volume boundary.
- The lifecycle state machine prevents exposing skills before they're operational.
