//! The Contextualizer — request normalization and validation.
//!
//! Six passes run in order; each is pure and unit-testable with a
//! mocked `CapabilityDirectory`:
//!
//! 1. `validate_action` — at least one provider serves the action's
//!    primitive (and skill, if any).
//! 2. `normalize_payload` — apply aliases, decompose `messages`,
//!    flatten shortcuts.
//! 3. `validate_input` — validate the canonical payload against the
//!    input vocabulary (types, ranges, required fields).
//! 4. `extract_media` — walk the payload, collect every
//!    `{media_id: "..."}` reference.
//! 5. `resolve_provider` — pick the target provider via skill,
//!    explicit `selectors.provider`, or primitive-only lookup. Sets
//!    `request.resolved_provider`.
//! 6. `validate_constraints` — zone constraint compatibility (advisory
//!    in v1).
//!
//! # ORCH-0030 R2 M3 changes
//!
//! Two passes from the legacy contextualizer were deleted:
//!
//! - **`resolve_model`** — model resolution is now adapter-local.
//!   Each adapter reads `request.selectors.model` inside its own
//!   `onboard` and applies its own resolution policy (Ollama's
//!   capability matrix; static cloud lists via
//!   [`crate::providers::cloud_common::resolve_cloud_model`]).
//! - **`validate_provider_narrowing`** — provider-side narrowings
//!   (`HonoredField::required` / `range` / `constraint`) were
//!   deleted with the legacy `Registration` type. Vocabulary
//!   validation in pass 3 covers the field-level checks; skill
//!   bindings narrow via `Binding.narrow` and are validated by
//!   the dispatching adapter when a skill is invoked.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::domain::errors::{ErrorCode, OrchestratorError};
use crate::domain::field_path::FieldPath;
use crate::domain::ids::MediaId;
use crate::domain::keys;
use crate::domain::primitive::Primitive;
use crate::domain::request::{Action, MediaContext, MediaReference, OrchestratorRequest};
use crate::domain::selectors::ZoneConstraint;
use crate::domain::vocabulary::{
    AliasCondition, FieldType, Vocabulary, VocabularyRegistry,
};
use crate::services::directory_subscriber::CapabilityDirectory;

/// The contextualization pipeline.
pub struct Contextualizer {
    vocabularies: VocabularyRegistry,
}

impl Contextualizer {
    pub fn new(vocabularies: VocabularyRegistry) -> Self {
        Self { vocabularies }
    }

    /// Run every pass in order. Returns an enriched request or a
    /// typed error mapping to an HTTP status via
    /// [`OrchestratorError::code`].
    pub async fn resolve(
        &self,
        mut request: OrchestratorRequest,
        directory: &Arc<CapabilityDirectory>,
    ) -> Result<OrchestratorRequest, OrchestratorError> {
        let vocabulary = self.vocabularies.get(request.action.primitive).clone();

        // Pass 1
        self.validate_action(&request, directory).await?;

        // Pass 2
        request.payload = self.normalize_payload(&request.action, request.payload, &vocabulary)?;

        // Pass 3
        self.validate_input(&request.payload, &vocabulary)?;

        // Pass 4
        request.media = self.extract_media(&request.payload)?;

        // Pass 5 (was Pass 6 in the legacy contextualizer)
        self.resolve_provider(&mut request, directory).await?;

        // Pass 6 (was Pass 8)
        self.validate_constraints(&request)?;

        Ok(request)
    }

    // ── Pass 1: validate_action ───────────────────────────────

    async fn validate_action(
        &self,
        request: &OrchestratorRequest,
        directory: &Arc<CapabilityDirectory>,
    ) -> Result<(), OrchestratorError> {
        let primitive = request.action.primitive;
        if let Some(skill) = request.action.skill.as_ref() {
            let providers = directory
                .providers_for_skill(primitive, skill.as_str())
                .await;
            if providers.is_empty() {
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
            return Ok(());
        }
        if directory.providers_for_primitive(primitive).await.is_empty() {
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

    // ── Pass 5: resolve_provider ──────────────────────────────

    async fn resolve_provider(
        &self,
        request: &mut OrchestratorRequest,
        directory: &Arc<CapabilityDirectory>,
    ) -> Result<(), OrchestratorError> {
        let primitive = request.action.primitive;

        // Skill path: the skill+primitive pair narrows the
        // candidate set, then `selectors.provider` (if present)
        // tightens the choice.
        if let Some(skill) = request.action.skill.as_ref() {
            let candidates = directory
                .providers_for_skill(primitive, skill.as_str())
                .await;
            if candidates.is_empty() {
                return Err(OrchestratorError::new(
                    ErrorCode::NotFound,
                    format!(
                        "Skill `{}` is not registered for `{}`.",
                        skill,
                        primitive.dotted()
                    ),
                ));
            }
            if let Some(override_provider) = request.selectors.provider.as_ref() {
                if !candidates.iter().any(|p| p == override_provider) {
                    return Err(OrchestratorError::new(
                        ErrorCode::ValidationFailed,
                        format!(
                            "Skill `{}` is not served by `{}`.",
                            skill, override_provider
                        ),
                    ));
                }
                request.resolved_provider = Some(override_provider.clone());
            } else {
                request.resolved_provider = Some(candidates.into_iter().next().unwrap());
            }
            return Ok(());
        }

        // Provider-only: the caller named a provider without a skill.
        if let Some(provider) = request.selectors.provider.clone() {
            if directory.capability(&provider, primitive).await.is_none() {
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

        // Implicit path: pick the first provider that serves the
        // primitive. M1 has no preferences/locality routing — that
        // is R2.5 commit 12, deferred per the M0 plan.
        let candidates = directory.providers_for_primitive(primitive).await;
        match candidates.into_iter().next() {
            Some(provider) => {
                request.resolved_provider = Some(provider);
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

    // ── Pass 6: validate_constraints ──────────────────────────

    fn validate_constraints(
        &self,
        request: &OrchestratorRequest,
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
    if matches!(vocab.primitive, Primitive::ImageAnalyze) && path.starts_with("text.") {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::primitive::Primitive;
    use crate::domain::vocabulary::VocabularyRegistry;

    fn vocab() -> VocabularyRegistry {
        VocabularyRegistry::build()
    }

    fn ctx() -> Contextualizer {
        Contextualizer::new(vocab())
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
                .normalize_payload(&Action::bare(Primitive::TextChat), body, &vocabulary)
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
}
