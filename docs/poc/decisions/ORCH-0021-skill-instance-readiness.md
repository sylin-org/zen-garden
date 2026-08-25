---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0021: Skill Instance Readiness — Adapter-Owned Provisioning

**Date**: 2026-04-01
**Status**: Accepted
**Supersedes**: The skill status field on SkillDefinition (ORCH-0018/0019 status model)

---

## Problem

ORCH-0018 stored a `SkillStatus` field on each `SkillDefinition` (Initializing →
Provisioning → Ready). This is wrong for three reasons:

1. **Status is not a property of a skill definition.** A skill definition is static
   metadata (name, schema, diagram, required resources). Whether an instance can
   serve it is a runtime question about that instance, not the definition.

2. **Re-registration overwrites status.** Every discovery cycle re-declares skills
   from the provider, resetting status to Initializing. This caused triple-downloads
   of the same 4.3GB model.

3. **The orchestrator managed provisioning details.** The prep module knew how to
   download models and push them via Moss volume API — ComfyUI-specific logic
   living in the orchestrator layer instead of the adapter.

---

## Decision

### Skill = definition + instance readiness

A skill is a **static definition** (what it does) paired with a **dynamic list of
instance readiness states** (who can serve it).

```
Skill (orchestrator concept)
  ├── definition (static singleton)
  │     name, display_name, capability, description
  │     content_slots, parameter_schema, diagram
  │     required_resources (provider-specific)
  │     provider_kind
  │
  └── instances[] (adapter-managed, computed)
        ├── endpoint
        ├── stone_name
        ├── ready: bool
        └── reason: String
```

**Available** = at least one instance is ready. This is computed, not stored.

### Adapter owns readiness and provisioning

The Provider trait gains two methods:

```rust
trait Provider {
    /// Built-in skill definitions this provider can execute.
    /// Called once at startup. Definitions are static singletons.
    fn builtin_skills(&self) -> Vec<SkillDefinition>;

    /// Check if an instance is ready to serve a skill.
    /// Returns readiness status with reason.
    fn check_skill_readiness(
        &self,
        ctx: &ProviderContext,
        skill: &str,
    ) -> BoxFuture<'_, Result<SkillReadiness>>;

    /// Make an instance ready for a skill.
    /// Downloads models, pushes workflows — adapter-specific.
    fn provision_skill(
        &self,
        ctx: &ProviderContext,
        skill: &str,
        cache_dir: &Path,
        moss_endpoint: &str,
        fqn: &str,
    ) -> BoxFuture<'_, Result<()>>;

    /// Execute a skill on a ready instance.
    fn workflow(
        &self,
        ctx: &ProviderContext,
        req: WorkflowRequest,
    ) -> BoxFuture<'_, Result<WorkflowJob>>;
}

struct SkillReadiness {
    ready: bool,
    reason: String,  // "ready", "models missing", "unreachable"
}
```

**ComfyUI adapter** implements:
- `check_skill_readiness`: queries `/models/upscale_models`, `/models/checkpoints` — checks if required files exist
- `provision_skill`: downloads models to local cache, pushes via Moss volume API

**Ollama adapter** (future) implements:
- `check_skill_readiness`: checks if model is in `models_available`
- `provision_skill`: pulls model via Ollama API

### Orchestrator's role

The orchestrator:
1. **Startup**: collects `builtin_skills()` from all providers → registers singletons
2. **Startup**: scans `{data_dir}/skills/` for imported skills → registers singletons
3. **Discovery**: for each skill, for each instance of the matching provider:
   - Calls `check_skill_readiness()` → records result
   - If not ready, spawns `provision_skill()` in background
4. **API query**: computes `available` = any instance ready. Returns instance list.
5. **Routing**: selects from ready instances only.

The orchestrator never knows HOW to provision. It just asks the adapter.

### SkillDefinition becomes static

```rust
pub struct SkillDefinition {
    pub name: String,
    pub display_name: String,
    pub capability: Capability,
    pub description: String,
    pub vram_mb: u64,
    pub content_slots: Vec<ContentSlot>,
    pub parameter_schema: FormSchema,
    pub diagram: Option<String>,
    pub required_models: Vec<ModelRef>,
    pub provider_kind: OfferingKind,
    pub implementation: serde_json::Value,
    // NO status field — availability is computed from instances
}
```

### SkillsSnapshot includes computed availability

```rust
pub struct SkillsSnapshot {
    pub skills: Arc<Vec<SkillView>>,
    pub workflow_jobs: Arc<HashMap<String, WorkflowJob>>,
}

pub struct SkillView {
    pub definition: SkillDefinition,
    pub available: bool,  // any instance ready
    pub instances: Vec<SkillInstanceView>,
}

pub struct SkillInstanceView {
    pub stone_name: String,
    pub endpoint: String,
    pub ready: bool,
    pub reason: String,
}
```

### Discovery flow

```
for provider in providers:
    for skill in provider.builtin_skills():
        for instance in registry.instances_of(provider.kind()):
            readiness = provider.check_skill_readiness(instance, skill.name)
            record(skill.name, instance.endpoint, readiness)
            if !readiness.ready:
                spawn provider.provision_skill(instance, skill.name, ...)
```

Provisioning is fire-and-forget. The next discovery cycle checks readiness again.
No status tracking on the skill — just check-and-provision each cycle.

Deduplication: if an instance is already being provisioned (tracked by a simple
`HashSet<(skill_name, endpoint)>` in the domain), skip. Cleared when provisioning
completes or fails.

---

## Migration

### Remove
- `SkillStatus` enum from `domain/skill.rs`
- `status` field from `SkillDefinition`
- `update_status()` from `SkillsDomain`
- `skills::prep` module (provisioning logic moves into ComfyUI provider)
- Status-tracking in discovery.rs

### Add
- `provider_kind: OfferingKind` to `SkillDefinition`
- `check_skill_readiness()` and `provision_skill()` to Provider trait
- `SkillView` and `SkillInstanceView` types
- Readiness tracking in `SkillsDomain` (per skill+endpoint)
- Provisioning dedup set in `SkillsDomain`

### Move
- Model download + Moss push logic from `skills::prep` into `providers::comfyui`
- The `recommended_upscale_models()` and `recommended_checkpoint_models()` stay in `skills::builtin` (model metadata is skill-level, not adapter-level)

---

## Consequences

- Skills are true singletons — registered once, never overwritten.
- Availability is always computed from live instance state — never stale.
- Provisioning is adapter-owned — the orchestrator delegates, never implements.
- Adding a new provider with skills (e.g., Ollama vision) only requires implementing
  `check_skill_readiness` and `provision_skill` on the adapter.
- The triple-download bug is structurally impossible — there's nothing to re-register.
- The API returns per-instance readiness — the dashboard shows exactly which stones
  can serve each skill and why others can't.
