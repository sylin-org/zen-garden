---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0038: Context-Aware Workspace Resolution

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Supersedes**: parts of ORCH-0036 (static Capability.parameters as form source)
**Amends**: ORCH-0037 (composed introspect)

---

## Context

ORCH-0036 made the introspect endpoint return the workspace definition.
ORCH-0037 made the handler compose vocabulary base + provider overlay.
Both treated `Capability.parameters` as a static list — the form surface
was fixed per (primitive, provider) pair.

This misses a critical dimension: **different models within the same
provider may expose different field surfaces**. Anthropic's `claude-4-5`
has "thinking mode" and "effort" controls that `claude-3-haiku` lacks.
Ollama's reasoning models may expose a "reasoning depth" slider that
regular chat models don't. The static overlay can't express this.

The deeper realization: field resolution is a **function of resolved
context**. The inputs are the primitive, an optional model hint, and
an optional provider hint. The output is a workspace description — the
fields, the resolved model, the media inputs, the examples. The adapter
is the only entity that knows how its models differ, so the adapter
must be the one to answer.

---

## Decision

### `describe_workspace` — new Provider trait method

The `Provider` trait gains a method that takes a primitive and an
optional model hint, and returns a fresh workspace description
tailored to that context:

```rust
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    fn name(&self) -> ProviderName;

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderResult, ProviderError>;

    /// Describe the workspace this provider would render for the
    /// given primitive and optional model hint. Returns None if the
    /// provider doesn't serve this primitive, or doesn't have the
    /// requested model.
    ///
    /// The adapter is the single authority on its own field surface.
    /// Static capability parameters in the announcement are an
    /// initial hint; the live answer always comes from this method.
    async fn describe_workspace(
        &self,
        primitive: Primitive,
        model_hint: Option<&str>,
    ) -> Option<WorkspaceDescription>;

    async fn flush_caches(&self) -> Result<FlushReport, ProviderError> {
        Ok(FlushReport::empty())
    }
}
```

### `WorkspaceDescription` — the adapter's answer

```rust
pub struct WorkspaceDescription {
    /// The model the provider would use for this primitive given
    /// the hint. None if the provider has no model concept
    /// (LibreTranslate, ComfyUI workflow-fixed skills).
    pub resolved_model: Option<String>,

    /// Field surface the form should render. This is the live,
    /// context-aware answer — provider-specific AND model-specific
    /// fields all composed here.
    pub fields: Vec<SkillParameter>,

    /// Media inputs accepted by this primitive for this provider.
    pub media_inputs: Vec<CapabilityMediaInput>,

    /// Example scenarios.
    pub examples: Vec<Example>,
}
```

### Adapter responsibilities

Each adapter implements `describe_workspace` with its own logic:

**Simple case (LibreTranslate)** — no model concept, static fields:

```rust
async fn describe_workspace(&self, primitive: Primitive, _: Option<&str>) -> Option<WorkspaceDescription> {
    if primitive != Primitive::TextTranslate { return None; }
    Some(WorkspaceDescription {
        resolved_model: None,
        fields: self.static_params(),
        media_inputs: Vec::new(),
        examples: self.static_examples(),
    })
}
```

**Model-aware case (Ollama)** — resolves model from hint, builds fields
with live model options:

```rust
async fn describe_workspace(&self, primitive: Primitive, hint: Option<&str>) -> Option<WorkspaceDescription> {
    if !self.serves(primitive) { return None; }

    // Resolve model: hint (if we have it) or recommended selection
    let model = match hint {
        Some(m) if self.has_model(m) => m.to_string(),
        Some(_) => return None,  // We don't have this model
        None => self.recommended_for(primitive)?,
    };

    Some(WorkspaceDescription {
        resolved_model: Some(model.clone()),
        fields: self.build_fields_for(primitive, &model),
        media_inputs: self.media_inputs_for(primitive),
        examples: self.examples_for(primitive),
    })
}
```

**Per-model overlay case (hypothetical Anthropic)**:

```rust
async fn describe_workspace(&self, primitive: Primitive, hint: Option<&str>) -> Option<WorkspaceDescription> {
    if primitive != Primitive::TextChat { return None; }

    let model = hint.unwrap_or("claude-4-5-sonnet").to_string();
    if !self.has_model(&model) { return None; }

    let mut fields = base_chat_fields();
    // Claude 4+ supports thinking mode and effort
    if model.starts_with("claude-4") {
        fields.push(thinking_field());
        fields.push(effort_field());
    }
    // Claude 3 Haiku and earlier don't

    Some(WorkspaceDescription {
        resolved_model: Some(model),
        fields,
        media_inputs: Vec::new(),
        examples: anthropic_examples(),
    })
}
```

### Unified resolver in the introspect handler

The introspect handler no longer reads `Capability.parameters`
directly. It walks provider candidates in priority order and asks
each one to describe the workspace for the requested context:

```rust
async fn resolve_workspace(
    primitive: Primitive,
    model_hint: Option<&str>,
    provider_hint: Option<&str>,
) -> Option<(ProviderName, WorkspaceDescription)> {
    // 1. All providers that serve this primitive
    let candidates = capability_directory.providers_for_primitive(primitive).await;

    // 2. Filter by provider hint if specified
    let filtered = candidates
        .into_iter()
        .filter(|p| provider_hint.map_or(true, |h| p.as_str() == h));

    // 3. Sort by priority (highest wins)
    let mut by_priority: Vec<_> = filtered
        .filter_map(|name| {
            let priority = /* look up from capability */;
            Some((name, priority))
        })
        .collect();
    by_priority.sort_by_key(|(_, p)| -p);

    // 4. Ask each candidate to describe the workspace
    //    First Some() wins. The adapter is the authority.
    for (name, _) in by_priority {
        if let Some(provider) = provider_registry.get(&name).await {
            if let Some(desc) = provider.describe_workspace(primitive, model_hint).await {
                return Some((name, desc));
            }
        }
    }
    None
}
```

**Resolution cases:**

| Input | Flow |
|-------|------|
| `GET /v1/text/chat` | No hints. Walk providers by priority. Each returns its default workspace. First Some() wins. |
| `GET /v1/text/chat?model=claude-4-5` | Walk providers. Each is asked "do you serve claude-4-5?". Only Anthropic returns Some. Its describe_workspace tailors fields to claude-4-5 (adds thinking, effort). |
| `GET /v1/text/chat?provider=ollama` | Only consider Ollama. It resolves its recommended model, returns fields. |
| `GET /v1/text/chat?provider=anthropic&model=claude-3-haiku` | Only Anthropic is considered. It returns fields without thinking/effort (haiku doesn't support them). |

### `Capability.parameters` becomes a hint, not a source

The static `parameters` field on `Capability` remains for two reasons:

1. **Catalog summary**: `GET /v1/catalog` lists primitives and skills
   with a lightweight parameter list. Calling `describe_workspace` for
   every primitive + provider just to build the catalog would be
   wasteful. The static list is enough for the summary.

2. **Initial announcement**: The adapter publishes a capability
   announcement on startup. Before any request arrives, the directory
   needs to know what primitives the adapter can serve.

But the **workspace form** (via introspect endpoint) ALWAYS calls
`describe_workspace`. No direct reads of `Capability.parameters` in
the introspect handler.

### Frontend: re-fetch on model change

The Workspace component tracks the selected model. When the user
changes the Model dropdown, the effect triggers a re-fetch of the
introspect endpoint with `?model=` set. If the new model has different
fields, the form updates.

```
User lands on /create/text/chat
  → GET /v1/text/chat
  → winning provider: Ollama (priority 0)
  → Ollama.describe_workspace(TextChat, None)
  → resolved_model = "llama3.2:latest"
  → fields = [user, system, temperature, max_tokens, history, model]

User selects "deepseek-r1:8b" from the Model dropdown
  → GET /v1/text/chat?model=deepseek-r1:8b
  → Ollama.describe_workspace(TextChat, Some("deepseek-r1:8b"))
  → resolved_model = "deepseek-r1:8b"
  → fields = [user, system, temperature, max_tokens, history, model, reasoning_depth]
  → form updates: new slider appears
```

---

## Backend changes

| Change | Where |
|--------|-------|
| Add `WorkspaceDescription` struct | `domain/provider.rs` |
| Add `describe_workspace` trait method | `domain/provider.rs` |
| Implement in all 9 adapters (default + model-aware) | `providers/*.rs` |
| Introspect handler: unified resolver | `http/introspect.rs` |
| Remove direct reads of `Capability.parameters` | `http/introspect.rs` |

## Frontend changes

| Change | Where |
|--------|-------|
| Workspace tracks selectedModel | `Workspace.tsx` |
| Model field change triggers onModelChange callback | `WorkspaceForm.tsx` |
| Introspect URL includes `?model=` when set | `Workspace.tsx` |
| Re-fetch on model change | `Workspace.tsx` |

---

## Consequences

### Positive

- **Adapter owns its field surface.** The Provider trait method is the
  single authority. No leakage of implementation details into the
  domain or the handler.
- **Per-model field overlays work.** A provider can expose different
  fields for different models within its own catalog.
- **Unified resolver.** Model and provider hints compose naturally.
  The resolver walks candidates in priority order; adapters answer
  with their context-aware description.
- **Clean SoC.** Domain defines the contract. Providers implement
  it. The handler orchestrates. No cross-cutting static data
  structures.

### Negative

- **Trait change touches all adapters.** Each needs an implementation,
  even if it just returns static data. Mitigated: the simple case is
  ~15 lines of Rust.
- **Every introspect call invokes the adapter.** Previously static,
  now a method call. Mitigated: the method is async and can cache
  internally; model resolution is already async in adapters that
  need it.

---

## References

- [ORCH-0036](ORCH-0036-introspect-as-workspace-definition.md) — introspect as workspace
- [ORCH-0037](ORCH-0037-composed-introspect-and-provider-overlay.md) — provider overlay
- [ORCH-0028](ORCH-0028-orchestrator-core.md) — Provider trait
