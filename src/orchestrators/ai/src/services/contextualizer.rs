//! The Contextualizer — request normalization and validation.
//!
//! Eight passes run in order; each is pure and unit-testable with a
//! mocked Directory snapshot:
//!
//! 1. `validate_action` — the action exists.
//! 2. `normalize_payload` — apply aliases, flatten shortcuts.
//! 3. `validate_input` — validate the canonical payload against the
//!    input vocabulary (types, ranges, required fields).
//! 4. `extract_media` — walk the payload, collect every
//!    `{media_id: "..."}` reference.
//! 5. `resolve_model` — translate `recommended:*` to a concrete
//!    model FQN.
//! 6. `resolve_provider` — find the target provider via skill, model,
//!    or primitive-only lookup.
//! 7. `validate_provider_narrowing` — apply the chosen provider's
//!    `honored_fields` constraints.
//! 8. `validate_constraints` — zone constraint compatibility.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::domain::directory::{DirectorySnapshot, ModelView};
use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::field_path::FieldPath;
use crate::domain::ids::{MediaId, ModelFqn, ProviderName};
use crate::domain::keys;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    FieldRange, HonoredField, Provider, ProviderHealth, Registration, RegistrationStrategy,
};
use crate::domain::request::{Action, MediaContext, MediaReference, ModelRef, OrchestratorRequest};
use crate::domain::selectors::ZoneConstraint;
use crate::domain::vocabulary::{
    AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
    VocabularyRegistry,
};

/// Recommended moniker prefix used by the caller to ask for a
/// capability-based model (§ORCH-0011).
pub const RECOMMENDED_PREFIX: &str = "recommended:";

/// The contextualization pipeline.
pub struct Contextualizer {
    vocabularies: VocabularyRegistry,
    recommendation: Option<Arc<dyn RecommendationResolver>>,
}

/// Pluggable recommendation lookup. Implemented by
/// [`crate::services::recommendation::RecommendationEngine`]; a stub
/// implementation is used in unit tests.
///
/// The contextualizer reads three things from the resolver:
/// - The model selected for an explicit `recommended:<capability>`
///   moniker.
/// - The primitive a capability label maps to (so the contextualizer
///   can wire the resolved model to the right dispatch path).
/// - The default capability for a bare primitive (used when a caller
///   omits the model selector entirely — `text.chat` with no model
///   becomes `recommended:chat` under the hood).
pub trait RecommendationResolver: Send + Sync + 'static {
    fn selected_for_capability(&self, capability: &str) -> Option<ModelFqn>;
    fn primitive_for_capability(&self, capability: &str) -> Option<Primitive>;
    fn default_capability_for_primitive(&self, primitive: Primitive) -> Option<String>;
}

impl Contextualizer {
    pub fn new(
        vocabularies: VocabularyRegistry,
        recommendation: Option<Arc<dyn RecommendationResolver>>,
    ) -> Self {
        Self {
            vocabularies,
            recommendation,
        }
    }

    /// Run every pass in order. Returns an enriched request or a
    /// typed error mapping to an HTTP status via
    /// [`OrchestratorError::code`].
    pub async fn resolve(
        &self,
        mut request: OrchestratorRequest,
        snapshot: &DirectorySnapshot,
    ) -> Result<OrchestratorRequest, OrchestratorError> {
        let vocabulary = self
            .vocabularies
            .get(request.action.primitive)
            .clone();

        // Pass 1
        self.validate_action(&request, snapshot)?;

        // Pass 2
        request.payload = self.normalize_payload(&request.action, request.payload, &vocabulary)?;

        // Pass 3
        self.validate_input(&request.payload, &vocabulary)?;

        // Pass 4
        request.media = self.extract_media(&request.payload)?;

        // Pass 5
        self.resolve_model(&mut request, snapshot)?;

        // Pass 6
        self.resolve_provider(&mut request, snapshot)?;

        // Pass 7
        self.validate_provider_narrowing(&request, snapshot)?;

        // Pass 8
        self.validate_constraints(&request, snapshot)?;

        Ok(request)
    }

    // ── Pass 1: validate_action ───────────────────────────────

    fn validate_action(
        &self,
        request: &OrchestratorRequest,
        snapshot: &DirectorySnapshot,
    ) -> Result<(), OrchestratorError> {
        let primitive = request.action.primitive;
        if snapshot.providers_for(primitive).is_empty() {
            return Err(OrchestratorError::new(
                ErrorCode::NoCandidates,
                format!(
                    "No provider is registered for primitive `{}`.",
                    primitive.dotted()
                ),
            )
            .with_details(serde_json::json!({
                "primitive": primitive.dotted(),
            })));
        }
        if let Some(skill) = request.action.skill.as_ref() {
            if snapshot.find_skill(primitive, skill).is_none() {
                return Err(OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!(
                        "Skill `{}` is not registered for primitive `{}`.",
                        skill,
                        primitive.dotted()
                    ),
                )
                .with_details(serde_json::json!({
                    "primitive": primitive.dotted(),
                    "skill": skill.as_str(),
                })));
            }
        }
        Ok(())
    }

    // ── Pass 2: normalize_payload ─────────────────────────────

    fn normalize_payload(
        &self,
        action: &Action,
        payload: Value,
        vocabulary: &Vocabulary,
    ) -> Result<Value, OrchestratorError> {
        let mut root = match payload {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Request body for `{}` must be a JSON object (got {})",
                        action.dotted(),
                        value_kind(&other)
                    ),
                ));
            }
        };

        // Remove top-level routing selectors before applying aliases —
        // they are parsed elsewhere (selectors). The ingress layer is
        // responsible for extracting them; here we simply drop any
        // leftover copies so they don't fail validation.
        for key in &["provider", "model", "skill", "variant", "action"] {
            root.remove(*key);
        }

        // Apply aliases, allowing collisions with the canonical path
        // only when the values are equal.
        for alias in &vocabulary.input.aliases {
            if let Some(value) = root.get(alias.from.as_str()).cloned() {
                if !condition_matches(&alias.condition, &value) {
                    continue;
                }
                match alias.condition {
                    AliasCondition::MessagesDecomposer => {
                        decompose_messages(&mut root, &alias.from, &value)?;
                    }
                    _ => {
                        set_canonical_from_alias(&mut root, &alias.from, &alias.to, value)?;
                    }
                }
            }
        }

        Ok(Value::Object(root))
    }

    // ── Pass 3: validate_input ────────────────────────────────

    fn validate_input(
        &self,
        payload: &Value,
        vocabulary: &Vocabulary,
    ) -> Result<(), OrchestratorError> {
        let root = match payload {
            Value::Object(map) => map,
            _ => {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    "Canonical payload must be a JSON object after normalization.",
                ));
            }
        };

        // Build a flat map of every populated canonical path in the
        // payload, respecting the nested shape on the wire.
        let mut populated = BTreeMap::new();
        flatten_nested("", root, &mut populated);

        // Known paths from the vocabulary.
        let mut known = HashSet::new();
        for spec in vocabulary
            .input
            .required
            .iter()
            .chain(vocabulary.input.optional.iter())
        {
            known.insert(spec.path.as_str().to_string());
        }
        for ns in &vocabulary.input.shared_namespaces {
            known.insert(format!("__shared:{}", ns.as_str()));
        }

        // Reject unknown non-passthrough paths.
        for (path, _) in &populated {
            if FieldPath::parse(path)
                .map(|fp| fp.is_passthrough())
                .unwrap_or(false)
            {
                continue;
            }
            if !is_known_path(path, vocabulary, &known) {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Unknown input field `{}` for primitive `{}`. Prefix with `x_` to bypass validation.",
                        path,
                        vocabulary.primitive.dotted()
                    ),
                )
                .with_details(serde_json::json!({
                    "field": path,
                    "primitive": vocabulary.primitive.dotted(),
                })));
            }
        }

        // Required fields must be present.
        for spec in &vocabulary.input.required {
            if populated.get(spec.path.as_str()).is_none() {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Required field `{}` is missing for primitive `{}`.",
                        spec.path,
                        vocabulary.primitive.dotted()
                    ),
                )
                .with_details(serde_json::json!({
                    "field": spec.path.as_str(),
                    "primitive": vocabulary.primitive.dotted(),
                })));
            }
        }

        // Validate types and ranges for populated fields.
        for spec in vocabulary
            .input
            .required
            .iter()
            .chain(vocabulary.input.optional.iter())
        {
            if let Some(value) = populated.get(spec.path.as_str()) {
                validate_field_type(&spec.path, value, &spec.field_type)?;
            }
        }

        Ok(())
    }

    // ── Pass 4: extract_media ─────────────────────────────────

    fn extract_media(&self, payload: &Value) -> Result<MediaContext, OrchestratorError> {
        let mut ctx = MediaContext::default();
        if let Value::Object(root) = payload {
            walk_for_media("", root, &mut ctx)?;
        }
        Ok(ctx)
    }

    // ── Pass 5: resolve_model ─────────────────────────────────

    fn resolve_model(
        &self,
        request: &mut OrchestratorRequest,
        snapshot: &DirectorySnapshot,
    ) -> Result<(), OrchestratorError> {
        // If caller supplied a model, attempt resolution.
        let model_hint = request.selectors.model.clone();
        if let Some(hint) = model_hint {
            if let Some(fqn) = hint.strip_prefix(RECOMMENDED_PREFIX) {
                self.apply_recommended(request, fqn)?;
            } else {
                let resolved = self.resolve_concrete_model(
                    &hint,
                    request.action.primitive,
                    snapshot,
                )?;
                request.resolved_model = Some(ModelRef::new(
                    resolved.provider.clone(),
                    resolved.short_name.clone(),
                ));
            }
        }
        Ok(())
    }

    /// Short-name or FQN resolution. ADR §Directory disambiguation:
    /// operator pins first, alphabetical tiebreaker second.
    fn resolve_concrete_model<'a>(
        &self,
        hint: &'a str,
        primitive: Primitive,
        snapshot: &'a DirectorySnapshot,
    ) -> Result<&'a ModelView, OrchestratorError> {
        if hint.contains('|') {
            let fqn = ModelFqn::parse(hint).map_err(|_| {
                OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!("Model FQN `{hint}` is malformed."),
                )
            })?;
            return snapshot.model(&fqn).ok_or_else(|| {
                OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!("Model `{hint}` is not registered."),
                )
            });
        }
        let mut matches: Vec<&ModelView> = snapshot.models_by_short_name(hint).collect();
        if matches.is_empty() {
            return Err(OrchestratorError::new(
                ErrorCode::NotFound,
                format!("Model `{hint}` is not registered with any provider."),
            ));
        }
        if matches.len() == 1 {
            return Ok(matches[0]);
        }
        // Layer 1: pin precedence. The default capability for
        // this primitive (e.g. `chat` for TextChat) tells us which
        // capability the operator's pin would apply to. If that
        // pinned model's short name equals the hint, that pin wins
        // regardless of alphabetical ordering.
        if let Some(resolver) = self.recommendation.as_ref() {
            if let Some(default_cap) = resolver.default_capability_for_primitive(primitive) {
                if let Some(pinned_fqn) = resolver.selected_for_capability(&default_cap) {
                    if pinned_fqn.short_name() == hint {
                        if let Some(winner) = matches
                            .iter()
                            .find(|m| m.fqn == pinned_fqn)
                            .copied()
                        {
                            return Ok(winner);
                        }
                    }
                }
            }
        }
        // Layer 2: alphabetical provider tiebreaker.
        matches.sort_by(|a, b| a.provider.cmp(&b.provider));
        Ok(matches[0])
    }

    fn apply_recommended(
        &self,
        request: &mut OrchestratorRequest,
        capability: &str,
    ) -> Result<(), OrchestratorError> {
        let Some(resolver) = self.recommendation.as_ref() else {
            return Err(OrchestratorError::new(
                ErrorCode::NoCandidates,
                format!(
                    "Recommendation engine is not available for capability `{capability}`."
                ),
            )
            .with_details(serde_json::json!({ "capability": capability })));
        };

        // The capability must exist in the registry, AND must
        // match the request's primitive (a caller asking for
        // text.chat with `recommended:vision` is a usage error).
        let Some(target_primitive) = resolver.primitive_for_capability(capability) else {
            return Err(OrchestratorError::new(
                ErrorCode::ValidationFailed,
                format!("Unknown capability `{capability}` in `recommended:*` moniker."),
            )
            .with_details(serde_json::json!({ "capability": capability })));
        };
        if target_primitive != request.action.primitive {
            return Err(OrchestratorError::new(
                ErrorCode::ValidationFailed,
                format!(
                    "Capability `{capability}` serves `{}`, not `{}` as requested.",
                    target_primitive.dotted(),
                    request.action.primitive.dotted()
                ),
            )
            .with_details(serde_json::json!({
                "capability": capability,
                "capability_primitive": target_primitive.dotted(),
                "request_primitive": request.action.primitive.dotted(),
            })));
        }

        if let Some(fqn) = resolver.selected_for_capability(capability) {
            request.resolved_model = Some(ModelRef::new(
                ProviderName::new(fqn.provider()),
                fqn.short_name(),
            ));
            return Ok(());
        }
        Err(OrchestratorError::new(
            ErrorCode::NoCandidates,
            format!(
                "No model is currently registered for capability `{capability}` (primitive `{}`).",
                target_primitive.dotted()
            ),
        )
        .with_details(serde_json::json!({
            "capability": capability,
            "primitive": target_primitive.dotted(),
        })))
    }

    // ── Pass 6: resolve_provider ──────────────────────────────

    fn resolve_provider(
        &self,
        request: &mut OrchestratorRequest,
        snapshot: &DirectorySnapshot,
    ) -> Result<(), OrchestratorError> {
        let primitive = request.action.primitive;

        // Skill path: skill identifies provider uniquely.
        if let Some(skill) = request.action.skill.as_ref() {
            let view = snapshot.find_skill(primitive, skill).ok_or_else(|| {
                OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!("Skill `{}` is not registered for `{}`.", skill, primitive.dotted()),
                )
            })?;
            let provider = view.provider.clone();
            // Caller-supplied provider must agree.
            if let Some(override_provider) = request.selectors.provider.as_ref() {
                if override_provider != &provider {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!(
                            "Skill `{}` is served by `{}`, not `{}` as requested.",
                            skill, provider, override_provider
                        ),
                    ));
                }
            }
            request.resolved_provider = Some(provider);
            return Ok(());
        }

        // Resolved model path: model → provider.
        if let Some(model_ref) = &request.resolved_model {
            if let Some(override_provider) = request.selectors.provider.as_ref() {
                if override_provider != &model_ref.provider {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!(
                            "Model `{}` is served by `{}`, not `{}` as requested.",
                            model_ref.short_name, model_ref.provider, override_provider
                        ),
                    ));
                }
            }
            request.resolved_provider = Some(model_ref.provider.clone());
            return Ok(());
        }

        // Provider-only: the caller named a provider without a model.
        if let Some(provider) = request.selectors.provider.clone() {
            if snapshot
                .find_registration(&provider, primitive, request.action.skill.as_ref())
                .is_none()
            {
                return Err(OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!(
                        "Provider `{}` does not serve primitive `{}`.",
                        provider,
                        primitive.dotted()
                    ),
                ));
            }
            request.resolved_provider = Some(provider);
            return Ok(());
        }

        // Implicit path: ask the recommendation engine for the
        // primitive's default capability (e.g. `chat` for
        // TextChat). If a model is available there, use it. This
        // is what makes a bare `text.chat` request route through
        // the same ranking pipeline as `recommended:chat`.
        if let Some(engine) = self.recommendation.as_ref() {
            if let Some(default_cap) = engine.default_capability_for_primitive(primitive) {
                if let Some(fqn) = engine.selected_for_capability(&default_cap) {
                    let provider_name = ProviderName::new(fqn.provider());
                    request.resolved_model =
                        Some(ModelRef::new(provider_name.clone(), fqn.short_name()));
                    request.resolved_provider = Some(provider_name);
                    return Ok(());
                }
            }
        }
        let first_healthy = snapshot
            .providers_for(primitive)
            .into_iter()
            .find(|v| matches!(v.health, ProviderHealth::Healthy))
            .or_else(|| snapshot.providers_for(primitive).into_iter().next());
        match first_healthy {
            Some(view) => {
                request.resolved_provider = Some(view.name.clone());
                Ok(())
            }
            None => Err(OrchestratorError::new(
                ErrorCode::NoCandidates,
                format!(
                    "No provider is registered for primitive `{}`.",
                    primitive.dotted()
                ),
            )),
        }
    }

    // ── Pass 7: validate_provider_narrowing ───────────────────

    fn validate_provider_narrowing(
        &self,
        request: &OrchestratorRequest,
        snapshot: &DirectorySnapshot,
    ) -> Result<(), OrchestratorError> {
        let Some(provider) = request.resolved_provider.as_ref() else {
            return Ok(());
        };
        let Some(registration) = snapshot.find_registration(
            provider,
            request.action.primitive,
            request.action.skill.as_ref(),
        ) else {
            return Ok(());
        };

        let populated = match &request.payload {
            Value::Object(root) => {
                let mut flat = BTreeMap::new();
                flatten_nested("", root, &mut flat);
                flat
            }
            _ => BTreeMap::new(),
        };

        // Required narrowings.
        for hf in &registration.honored_fields {
            if hf.required && !populated.contains_key(hf.path.as_str()) {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Provider `{}` requires field `{}` for `{}` but it is missing.",
                        provider,
                        hf.path,
                        request.action.dotted()
                    ),
                )
                .with_details(serde_json::json!({
                    "provider": provider.as_str(),
                    "field": hf.path.as_str(),
                    "action": request.action.dotted(),
                })));
            }
        }

        // Range narrowings.
        for hf in &registration.honored_fields {
            if let (Some(value), Some(range)) = (populated.get(hf.path.as_str()), hf.range.as_ref())
            {
                validate_range(provider, &hf.path, value, range)?;
            }
        }

        Ok(())
    }

    // ── Pass 8: validate_constraints ──────────────────────────

    fn validate_constraints(
        &self,
        request: &OrchestratorRequest,
        _snapshot: &DirectorySnapshot,
    ) -> Result<(), OrchestratorError> {
        // Zone constraints are advisory in v1; all providers are
        // considered internal by default. The pass stays in the
        // pipeline so future ADRs can wire it through without
        // reshaping the flow.
        match request.constraints.zone {
            ZoneConstraint::Any | ZoneConstraint::Internal | ZoneConstraint::External => Ok(()),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn condition_matches(cond: &AliasCondition, value: &Value) -> bool {
    match cond {
        AliasCondition::Always => true,
        AliasCondition::WhenString => value.is_string(),
        AliasCondition::WhenObject => value.is_object(),
        AliasCondition::WhenArray => value.is_array(),
        AliasCondition::MessagesDecomposer => value.is_array(),
    }
}

fn set_canonical_from_alias(
    root: &mut Map<String, Value>,
    from: &FieldPath,
    to: &FieldPath,
    value: Value,
) -> Result<(), OrchestratorError> {
    // Remove the alias source.
    root.remove(from.as_str());
    // Walk the canonical path and ensure no colliding scalar value
    // already sits there with different content.
    let segments: Vec<&str> = to.as_str().split('.').collect();
    set_nested(root, &segments, value, to)
}

fn set_nested(
    root: &mut Map<String, Value>,
    segments: &[&str],
    value: Value,
    full: &FieldPath,
) -> Result<(), OrchestratorError> {
    if segments.is_empty() {
        return Ok(());
    }
    let last = segments.len() - 1;
    let mut current: &mut Map<String, Value> = root;
    for (idx, segment) in segments.iter().enumerate() {
        if idx == last {
            // ADR §Acceptance-8: a canonical path already present
            // alongside its alias is a collision, regardless of
            // whether the values happen to match.
            if current.contains_key(*segment) {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!(
                        "Alias collision: `{full}` was set both via its canonical path and an alias. Pick one form."
                    ),
                )
                .with_details(serde_json::json!({ "field": full.as_str() })));
            }
            current.insert((*segment).to_string(), value);
            return Ok(());
        }
        let slot = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            return Err(OrchestratorError::new(
                ErrorCode::ValidationFailed,
                format!("Cannot descend into `{segment}` (not an object)."),
            ));
        }
        current = match current.get_mut(*segment) {
            Some(Value::Object(inner)) => inner,
            _ => unreachable!(),
        };
    }
    Ok(())
}

fn decompose_messages(
    root: &mut Map<String, Value>,
    from: &FieldPath,
    value: &Value,
) -> Result<(), OrchestratorError> {
    let arr = match value {
        Value::Array(a) => a.clone(),
        _ => {
            return Err(OrchestratorError::new(
                ErrorCode::ValidationFailed,
                "`messages` must be an array.",
            ));
        }
    };
    if arr.is_empty() {
        return Err(OrchestratorError::new(
            ErrorCode::ValidationFailed,
            "`messages` array must contain at least one user message.",
        ));
    }

    #[derive(Clone)]
    struct Turn {
        user: Option<String>,
        assistant: Option<String>,
    }

    let mut system: Option<String> = None;
    let mut turns: Vec<Turn> = Vec::new();
    let mut pending_user: Option<String> = None;

    for (idx, msg) in arr.iter().enumerate() {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let content = msg.get("content").cloned().unwrap_or(Value::Null);
        let text = message_text(&content)?;
        match role {
            "system" => {
                if !turns.is_empty() || pending_user.is_some() {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        "`system` message must appear before any user or assistant turn.",
                    ));
                }
                if system.is_some() {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        "Multiple `system` messages are not supported.",
                    ));
                }
                system = Some(text);
            }
            "user" => {
                if pending_user.is_some() {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!("Two consecutive `user` messages at index {idx}."),
                    ));
                }
                pending_user = Some(text);
            }
            "assistant" => {
                let Some(user) = pending_user.take() else {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!("`assistant` message at index {idx} has no preceding `user`."),
                    ));
                };
                turns.push(Turn {
                    user: Some(user),
                    assistant: Some(text),
                });
            }
            other => {
                return Err(OrchestratorError::new(
                    ErrorCode::ValidationFailed,
                    format!("Unknown message role `{other}` at index {idx}."),
                ));
            }
        }
    }

    let Some(final_user) = pending_user else {
        return Err(OrchestratorError::new(
            ErrorCode::ValidationFailed,
            "`messages` array must end with a `user` message (the turn to answer).",
        ));
    };

    // Populate canonical fields.
    root.remove(from.as_str());
    let user_path = keys::text::PROMPT_USER;
    let user_segments: Vec<&str> = user_path.as_str().split('.').collect();
    set_nested(root, &user_segments, Value::String(final_user), &user_path)?;

    if let Some(system) = system {
        let sys_path = keys::text::PROMPT_SYSTEM;
        let sys_segments: Vec<&str> = sys_path.as_str().split('.').collect();
        set_nested(root, &sys_segments, Value::String(system), &sys_path)?;
    }

    if !turns.is_empty() {
        let previous_array: Vec<Value> = turns
            .into_iter()
            .filter_map(|t| match (t.user, t.assistant) {
                (Some(u), Some(a)) => Some(serde_json::json!({
                    "user": u,
                    "assistant": a,
                })),
                _ => None,
            })
            .collect();
        let prev_path = keys::text::PROMPT_PREVIOUS;
        let prev_segments: Vec<&str> = prev_path.as_str().split('.').collect();
        set_nested(root, &prev_segments, Value::Array(previous_array), &prev_path)?;
    }

    Ok(())
}

fn message_text(content: &Value) -> Result<String, OrchestratorError> {
    match content {
        Value::String(s) => Ok(s.clone()),
        Value::Array(parts) => {
            // OpenAI-shape multimodal content: array of {type, text} parts.
            let mut buf = String::new();
            for part in parts {
                if let Some(t) = part.get("type").and_then(|v| v.as_str()) {
                    if t == "text" {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            if !buf.is_empty() {
                                buf.push('\n');
                            }
                            buf.push_str(text);
                        }
                    }
                }
            }
            Ok(buf)
        }
        _ => Err(OrchestratorError::new(
            ErrorCode::ValidationFailed,
            "Message content must be a string or an array of text parts.",
        )),
    }
}

fn flatten_nested(prefix: &str, map: &Map<String, Value>, out: &mut BTreeMap<String, Value>) {
    for (key, value) in map {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Object(inner) if is_flat_candidate(inner) => {
                flatten_nested(&full, inner, out);
            }
            _ => {
                out.insert(full, value.clone());
            }
        }
    }
}

/// Decide whether an object is a flat-candidate or a leaf value.
///
/// Leaves:
/// - Media reference objects (have a `media_id` key).
/// - Empty objects (nothing to descend into).
/// - Objects whose keys don't all look like field path segments
///   (e.g. freeform metadata bags).
///
/// Flat-candidates: objects whose keys are all valid field path
/// segments AND which are not media references.
fn is_flat_candidate(map: &Map<String, Value>) -> bool {
    if map.is_empty() {
        return false;
    }
    if map.contains_key("media_id") {
        return false;
    }
    map.keys().all(|k| FieldPath::validate(k).is_ok())
}

fn is_known_path(path: &str, vocab: &Vocabulary, known: &HashSet<String>) -> bool {
    if known.contains(path) {
        return true;
    }
    // Cover longer paths living under a shared namespace opt-in.
    for ns in &vocab.input.shared_namespaces {
        if path.starts_with(&format!("{}.", ns.as_str())) {
            return true;
        }
    }
    // Output-only shared namespaces for cross-modal primitives:
    // image.analyze's payload may legitimately carry `text.*` even
    // though it's an image primitive.
    if matches!(
        vocab.primitive,
        Primitive::ImageAnalyze
    ) && path.starts_with("text.")
    {
        return true;
    }
    // Output-only shared namespaces similarly for audio.transcribe.
    if matches!(vocab.primitive, Primitive::AudioTranscribe) && path.starts_with("text.") {
        return true;
    }
    false
}

fn validate_field_type(
    path: &FieldPath,
    value: &Value,
    field_type: &FieldType,
) -> Result<(), OrchestratorError> {
    let ok = match field_type {
        FieldType::String => value.is_string(),
        FieldType::Integer { min, max } => match value.as_i64() {
            Some(n) => {
                if let Some(m) = min {
                    if n < *m {
                        return Err(range_error_int(path, n, *min, *max));
                    }
                }
                if let Some(m) = max {
                    if n > *m {
                        return Err(range_error_int(path, n, *min, *max));
                    }
                }
                true
            }
            None => false,
        },
        FieldType::Number { min, max } => match value.as_f64() {
            Some(n) => {
                if let Some(m) = min {
                    if n < *m {
                        return Err(range_error_num(path, n, *min, *max));
                    }
                }
                if let Some(m) = max {
                    if n > *m {
                        return Err(range_error_num(path, n, *min, *max));
                    }
                }
                true
            }
            None => false,
        },
        FieldType::Boolean => value.is_boolean(),
        FieldType::Array => value.is_array(),
        FieldType::Object => value.is_object(),
        FieldType::MediaRef => match value {
            Value::String(_) => true,
            Value::Object(map) => map.contains_key("media_id"),
            _ => false,
        },
        FieldType::MessageHistory => match value {
            Value::Array(arr) => arr.iter().all(|item| {
                item.get("user").and_then(|v| v.as_str()).is_some()
                    && item.get("assistant").and_then(|v| v.as_str()).is_some()
            }),
            _ => false,
        },
    };
    if !ok {
        return Err(OrchestratorError::new(
            ErrorCode::ValidationFailed,
            format!(
                "Field `{path}` has invalid type (expected {}, got {}).",
                describe_type(field_type),
                value_kind(value)
            ),
        )
        .with_details(serde_json::json!({
            "field": path.as_str(),
            "expected": describe_type(field_type),
            "actual": value_kind(value),
        })));
    }
    Ok(())
}

fn describe_type(t: &FieldType) -> &'static str {
    match t {
        FieldType::String => "string",
        FieldType::Integer { .. } => "integer",
        FieldType::Number { .. } => "number",
        FieldType::Boolean => "boolean",
        FieldType::Array => "array",
        FieldType::Object => "object",
        FieldType::MediaRef => "media_ref",
        FieldType::MessageHistory => "message_history",
    }
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn range_error_int(
    path: &FieldPath,
    actual: i64,
    min: Option<i64>,
    max: Option<i64>,
) -> OrchestratorError {
    OrchestratorError::new(
        ErrorCode::ValidationFailed,
        format!(
            "Field `{path}` value {actual} out of range (min={:?}, max={:?}).",
            min, max
        ),
    )
    .with_details(serde_json::json!({
        "field": path.as_str(),
        "value": actual,
        "min": min,
        "max": max,
    }))
}

fn range_error_num(
    path: &FieldPath,
    actual: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> OrchestratorError {
    OrchestratorError::new(
        ErrorCode::ValidationFailed,
        format!(
            "Field `{path}` value {actual} out of range (min={:?}, max={:?}).",
            min, max
        ),
    )
    .with_details(serde_json::json!({
        "field": path.as_str(),
        "value": actual,
        "min": min,
        "max": max,
    }))
}

fn validate_range(
    provider: &ProviderName,
    path: &FieldPath,
    value: &Value,
    range: &FieldRange,
) -> Result<(), OrchestratorError> {
    let out_of_range = match (value, range) {
        (Value::Number(n), FieldRange::Integer { min, max }) => {
            if let Some(i) = n.as_i64() {
                min.map(|m| i < m).unwrap_or(false) || max.map(|m| i > m).unwrap_or(false)
            } else {
                true
            }
        }
        (Value::Number(n), FieldRange::Number { min, max }) => {
            if let Some(f) = n.as_f64() {
                min.map(|m| f < m).unwrap_or(false) || max.map(|m| f > m).unwrap_or(false)
            } else {
                true
            }
        }
        _ => false,
    };
    if out_of_range {
        return Err(OrchestratorError::new(
            ErrorCode::ValidationFailed,
            format!(
                "Field `{path}` is outside the range provider `{provider}` allows for this action."
            ),
        )
        .with_details(serde_json::json!({
            "provider": provider.as_str(),
            "field": path.as_str(),
        })));
    }
    Ok(())
}

fn walk_for_media(
    prefix: &str,
    map: &Map<String, Value>,
    ctx: &mut MediaContext,
) -> Result<(), OrchestratorError> {
    for (key, value) in map {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Object(inner) => {
                if let Some(id_str) = inner.get("media_id").and_then(|v| v.as_str()) {
                    // This is a media reference object. Stop descending
                    // into it.
                    let parsed = FieldPath::parse(&full).map_err(|e| {
                        OrchestratorError::new(
                            ErrorCode::InternalError,
                            format!("invalid field path `{full}`: {e}"),
                        )
                    })?;
                    ctx.referenced.push(MediaReference {
                        id: MediaId::from_string(id_str),
                        field: parsed,
                        content_type: inner
                            .get("content_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        metadata: inner.get("metadata").cloned().unwrap_or(Value::Null),
                    });
                } else {
                    walk_for_media(&full, inner, ctx)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}


/// Suppress unused-import warnings for types only referenced in
/// downstream modules.
#[allow(dead_code)]
fn _unused(_: FieldSpec, _: IoSchema, _: Registration, _: HonoredField, _: Arc<dyn Provider>) {
    let _ = SharedNamespace::Meta;
    let _ = RegistrationStrategy::Bare;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::ProviderName;
    use crate::domain::primitive::Primitive;
    use crate::domain::selectors::{Constraints, Selectors};
    use crate::domain::vocabulary::VocabularyRegistry;

    fn vocab() -> VocabularyRegistry {
        VocabularyRegistry::build()
    }

    fn ctx() -> Contextualizer {
        Contextualizer::new(vocab(), None)
    }

    #[test]
    fn normalize_expands_prompt_alias() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let normalized = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({"prompt": "Hi!"}),
                &vocabulary,
            )
            .unwrap();
        assert_eq!(
            normalized,
            serde_json::json!({"text": {"prompt": {"user": "Hi!"}}})
        );
    }

    #[test]
    fn normalize_expands_temperature_alias() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let normalized = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({"prompt": "Hi!", "temperature": 0.7, "max_tokens": 100}),
                &vocabulary,
            )
            .unwrap();
        assert_eq!(
            normalized,
            serde_json::json!({
                "text": {
                    "prompt": {"user": "Hi!"},
                    "sampling": {"temperature": 0.7},
                    "tokens": {"max": 100}
                }
            })
        );
    }

    #[test]
    fn alias_conflict_is_rejected() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let err = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({
                    "prompt": "alias-value",
                    "text": {"prompt": {"user": "canonical-value"}}
                }),
                &vocabulary,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    #[test]
    fn alias_collision_same_value_is_also_rejected() {
        // ADR §Acceptance-8: even with equal values, sending both the
        // alias form and the canonical form is rejected.
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let err = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({
                    "prompt": "Hi!",
                    "text": {"prompt": {"user": "Hi!"}}
                }),
                &vocabulary,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    /// Per-alias collision matrix for `text.chat` (§Acceptance-8).
    /// For each alias in the vocabulary, sending both the alias form
    /// and its canonical target simultaneously (with equal values)
    /// must fail. The test is also a vocabulary-completeness check:
    /// it asserts that every alias declared in the text.chat vocab
    /// has a corresponding case here, so adding a new alias without
    /// adding a collision test fails the build.
    #[test]
    fn text_chat_every_alias_collides_with_canonical() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();

        // (alias_name, body_with_both_forms) pairs.
        // Order is the same as the vocabulary's alias declarations
        // for easy diffing when a new alias is added.
        let cases = [
            (
                "prompt",
                serde_json::json!({"prompt": "Hi!", "text": {"prompt": {"user": "Hi!"}}}),
            ),
            (
                "system",
                serde_json::json!({
                    "system": "You are helpful",
                    "text": {"prompt": {"user": "x", "system": "You are helpful"}}
                }),
            ),
            (
                "temperature",
                serde_json::json!({
                    "temperature": 0.5,
                    "text": {"prompt": {"user": "x"}, "sampling": {"temperature": 0.5}}
                }),
            ),
            (
                "max_tokens",
                serde_json::json!({
                    "max_tokens": 100,
                    "text": {"prompt": {"user": "x"}, "tokens": {"max": 100}}
                }),
            ),
            (
                "top_p",
                serde_json::json!({
                    "top_p": 0.9,
                    "text": {"prompt": {"user": "x"}, "sampling": {"top_p": 0.9}}
                }),
            ),
            (
                "top_k",
                serde_json::json!({
                    "top_k": 40,
                    "text": {"prompt": {"user": "x"}, "sampling": {"top_k": 40}}
                }),
            ),
            (
                "seed",
                serde_json::json!({
                    "seed": 42,
                    "text": {"prompt": {"user": "x"}, "sampling": {"seed": 42}}
                }),
            ),
            (
                "stop",
                serde_json::json!({
                    "stop": ["END"],
                    "text": {"prompt": {"user": "x"}, "stop": {"sequences": ["END"]}}
                }),
            ),
            (
                "tools",
                serde_json::json!({
                    "tools": [{"name": "search"}],
                    "text": {
                        "prompt": {"user": "x"},
                        "tools": {"definitions": [{"name": "search"}]}
                    }
                }),
            ),
            (
                "stream",
                serde_json::json!({
                    "stream": true,
                    "text": {"prompt": {"user": "x"}, "stream": true}
                }),
            ),
            (
                "messages",
                // The decomposer sets text.prompt.user from the final
                // user message; pre-populating it canonically is the
                // collision.
                serde_json::json!({
                    "messages": [{"role": "user", "content": "Hi!"}],
                    "text": {"prompt": {"user": "Hi!"}}
                }),
            ),
        ];

        // Vocabulary completeness check — every declared alias has a case.
        let declared: std::collections::HashSet<&str> = vocabulary
            .input
            .aliases
            .iter()
            .map(|a| a.from.as_str())
            .collect();
        let covered: std::collections::HashSet<&str> =
            cases.iter().map(|(name, _)| *name).collect();
        let missing: Vec<&&str> = declared.difference(&covered).collect();
        assert!(
            missing.is_empty(),
            "Aliases declared in text.chat vocabulary but missing collision tests: {missing:?}"
        );
        let extra: Vec<&&str> = covered.difference(&declared).collect();
        assert!(
            extra.is_empty(),
            "Collision test cases reference aliases that no longer exist: {extra:?}"
        );

        // Each case must be rejected.
        for (name, body) in cases {
            let err = ctx
                .normalize_payload(
                    &Action::bare(Primitive::TextChat),
                    body,
                    &vocabulary,
                )
                .unwrap_err();
            assert_eq!(
                err.code,
                ErrorCode::ValidationFailed,
                "alias `{name}` collision was not rejected"
            );
        }
    }

    #[test]
    fn messages_decomposer_handles_system_user() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let normalized = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({
                    "messages": [
                        {"role": "system", "content": "You are helpful."},
                        {"role": "user", "content": "Hello"}
                    ]
                }),
                &vocabulary,
            )
            .unwrap();
        assert_eq!(
            normalized,
            serde_json::json!({
                "text": {
                    "prompt": {
                        "user": "Hello",
                        "system": "You are helpful."
                    }
                }
            })
        );
    }

    #[test]
    fn messages_decomposer_builds_previous() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let normalized = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({
                    "messages": [
                        {"role": "user", "content": "Hi"},
                        {"role": "assistant", "content": "Hello"},
                        {"role": "user", "content": "How are you?"}
                    ]
                }),
                &vocabulary,
            )
            .unwrap();
        assert_eq!(
            normalized["text"]["prompt"]["user"],
            serde_json::json!("How are you?")
        );
        assert_eq!(
            normalized["text"]["prompt"]["previous"],
            serde_json::json!([{"user": "Hi", "assistant": "Hello"}])
        );
    }

    #[test]
    fn messages_decomposer_rejects_trailing_assistant() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let err = ctx
            .normalize_payload(
                &Action::bare(Primitive::TextChat),
                serde_json::json!({
                    "messages": [
                        {"role": "user", "content": "Hi"},
                        {"role": "assistant", "content": "Hello"}
                    ]
                }),
                &vocabulary,
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    #[test]
    fn validate_input_rejects_unknown_field() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let payload =
            serde_json::json!({"text": {"prompt": {"user": "Hi"}}, "bogus": "value"});
        let err = ctx.validate_input(&payload, &vocabulary).unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    #[test]
    fn validate_input_accepts_passthrough() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let payload = serde_json::json!({
            "text": {"prompt": {"user": "Hi"}},
            "x_custom": "anything"
        });
        ctx.validate_input(&payload, &vocabulary).unwrap();
    }

    #[test]
    fn validate_input_range_enforced() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let payload = serde_json::json!({
            "text": {
                "prompt": {"user": "Hi"},
                "sampling": {"temperature": 5.0}
            }
        });
        let err = ctx.validate_input(&payload, &vocabulary).unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    #[test]
    fn extract_media_finds_source_media_id() {
        let ctx = ctx();
        let payload = serde_json::json!({
            "image": {"source": {"media_id": "01JA7X-test"}}
        });
        let media = ctx.extract_media(&payload).unwrap();
        assert_eq!(media.referenced.len(), 1);
        assert_eq!(media.referenced[0].id.as_str(), "01JA7X-test");
        assert_eq!(media.referenced[0].field.as_str(), "image.source");
    }

    #[test]
    fn required_field_missing_returns_validation_error() {
        let ctx = ctx();
        let vocabulary = vocab().get(Primitive::TextChat).clone();
        let payload = serde_json::json!({"text": {"sampling": {"temperature": 0.5}}});
        let err = ctx.validate_input(&payload, &vocabulary).unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationFailed);
    }

    // Suppress a minor linting complaint about an unused selector
    // import in the tests.
    fn _use_unused_imports() {
        let _ = Selectors::default();
        let _ = Constraints::default();
        let _ = ProviderName::new("x");
    }

    // ── Pin disambiguation for short-name model resolution ────

    struct StubResolver(Option<ModelFqn>);
    impl RecommendationResolver for StubResolver {
        fn selected_for_capability(&self, _capability: &str) -> Option<ModelFqn> {
            self.0.clone()
        }
        fn primitive_for_capability(&self, _capability: &str) -> Option<Primitive> {
            Some(Primitive::TextChat)
        }
        fn default_capability_for_primitive(&self, _primitive: Primitive) -> Option<String> {
            Some("chat".to_string())
        }
    }

    fn snapshot_with_models(
        models: Vec<(ProviderName, &str)>,
    ) -> DirectorySnapshot {
        use crate::domain::directory::ModelView;
        use crate::domain::ids::RegistrationId;
        use std::collections::HashMap;
        let mut models_map = HashMap::new();
        for (provider, short) in models {
            let fqn = ModelFqn::new(&provider, short);
            models_map.insert(
                fqn.clone(),
                ModelView {
                    fqn,
                    short_name: short.to_string(),
                    provider,
                    registration_id: RegistrationId::from("reg"),
                    primitives: vec![Primitive::TextChat.dotted().to_string()],
                    capability_tags: vec![],
                    size_bytes: None,
                    context_length: None,
                    parameter_count: None,
                },
            );
        }
        DirectorySnapshot {
            version: 1,
            updated_at: chrono::Utc::now(),
            providers: Arc::new(HashMap::new()),
            primitives: Arc::new(HashMap::new()),
            skills: Arc::new(HashMap::new()),
            models: Arc::new(models_map),
        }
    }

    #[test]
    fn pin_wins_over_alphabetical_for_short_name() {
        let ollama = ProviderName::new("ollama");
        let zcloud = ProviderName::new("zcloud");
        let snapshot = snapshot_with_models(vec![
            (ollama.clone(), "llama-3.1"),
            (zcloud.clone(), "llama-3.1"),
        ]);
        // Alphabetically `ollama` would win, but pin points at zcloud.
        let resolver: Arc<dyn RecommendationResolver> =
            Arc::new(StubResolver(Some(ModelFqn::new(&zcloud, "llama-3.1"))));
        let ctx = Contextualizer::new(vocab(), Some(resolver));
        let model = ctx
            .resolve_concrete_model("llama-3.1", Primitive::TextChat, &snapshot)
            .unwrap();
        assert_eq!(model.provider.as_str(), "zcloud");
    }

    #[test]
    fn alphabetical_tiebreaker_without_pin() {
        let ollama = ProviderName::new("ollama");
        let zcloud = ProviderName::new("zcloud");
        let snapshot = snapshot_with_models(vec![
            (ollama, "llama-3.1"),
            (zcloud, "llama-3.1"),
        ]);
        let ctx = Contextualizer::new(vocab(), None);
        let model = ctx
            .resolve_concrete_model("llama-3.1", Primitive::TextChat, &snapshot)
            .unwrap();
        assert_eq!(model.provider.as_str(), "ollama");
    }
}
