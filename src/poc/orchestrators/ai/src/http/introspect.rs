//! `GET /v1/{modality}/{leaf}[/{skill_id}]` — workspace definition
//! (ORCH-0036, ORCH-0037, ORCH-0038).
//!
//! Returns a composed workspace spec. The winning provider is asked
//! (via [`Provider::describe_workspace`]) to describe the workspace
//! for the requested context (primitive + optional model hint).
//! Provider candidates are walked in priority order; the first one
//! whose `describe_workspace` returns `Some` wins. The handler no
//! longer reads `Capability.parameters` directly — that field is a
//! startup-time hint, not the live source of truth.

#![allow(dead_code)]

use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::app_state::AppState;
use crate::domain::capability_announcement::{
    AutoDescriptor, Example, SkillDisplay, SkillParameter,
};
use crate::domain::errors::ErrorCode;
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;
use crate::domain::provider::WorkspaceDescription;
use crate::domain::vocabulary::{FieldType, Vocabulary};

use super::errors::quick_error_response;

// ── Query parameters ─────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
pub struct IntrospectQuery {
    pub provider: Option<String>,
    /// ORCH-0038: optional model hint. The winning provider uses
    /// this to tailor the returned field surface (e.g. a reasoning
    /// model exposes extra controls that a plain chat model lacks).
    pub model: Option<String>,
}

// ── Response shapes ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceResponse {
    pub kind: &'static str,
    pub primitive: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    pub display: IntrospectionDisplay,
    pub routing: RoutingInfo,
    pub invocation: InvocationInfo,
    pub payload: Value,
    pub fields: BTreeMap<String, FieldDescriptor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills_available: Vec<SkillListEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub media_inputs: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct IntrospectionDisplay {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_image: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutingInfo {
    pub providers: Vec<ProviderName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_run_on: Option<ProviderName>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvocationInfo {
    pub method: &'static str,
    pub url: String,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldDescriptor {
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub widget: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<AutoDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// "vocabulary" or the provider name — where this field came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillListEntry {
    pub id: String,
    pub provider: ProviderName,
    pub display: IntrospectionDisplay,
    pub url: String,
}

// ── Handlers ─────────────────────────────────────────────────

/// `GET /v1/{modality}/{leaf}` — composed workspace for a primitive.
pub async fn get_primitive(
    State(state): State<AppState>,
    Path((modality, leaf)): Path<(String, String)>,
    Query(query): Query<IntrospectQuery>,
) -> Response {
    let primitive = match Primitive::from_segments(&modality, &leaf) {
        Ok(p) => p,
        Err(e) => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                format!("Unknown primitive `{modality}.{leaf}`: {e}"),
            );
        }
    };

    let directory = &state.capability_directory;
    let all_providers_for = directory.providers_for_primitive(primitive).await;

    if all_providers_for.is_empty() {
        return quick_error_response(
            ErrorCode::NotFound,
            format!("No provider serves `{modality}.{leaf}`."),
        );
    }

    // ORCH-0038 unified resolver: walk providers in priority order,
    // ask each one to describe the workspace for the given (primitive,
    // model_hint). First Some() wins — the adapter is the single
    // authority on its own field surface.
    let model_hint = query.model.as_deref();
    let (winner, description) = match resolve_workspace(
        query.provider.as_deref(),
        model_hint,
        &all_providers_for,
        primitive,
        &state,
    )
    .await
    {
        Some(result) => result,
        None => {
            let detail = match (query.provider.as_deref(), model_hint) {
                (Some(p), Some(m)) => format!(
                    "Provider `{p}` does not serve `{modality}.{leaf}` with model `{m}`."
                ),
                (Some(p), None) => {
                    format!("Provider `{p}` does not serve `{modality}.{leaf}`.")
                }
                (None, Some(m)) => format!(
                    "No provider serves `{modality}.{leaf}` with model `{m}`."
                ),
                (None, None) => {
                    format!("No provider could describe `{modality}.{leaf}`.")
                }
            };
            return quick_error_response(ErrorCode::NotFound, detail);
        }
    };

    // Layer 1: vocabulary base fields (used only if adapter returned
    // an empty overlay — the bare-primitive fallback).
    let vocabulary = state.vocabularies.get(primitive);

    // Layer 2: provider overlay from the live workspace description.
    let overlay_params = &description.fields;

    // Compose
    let preferences = state.preferences.get_all().await;
    let (mut payload, fields) =
        compose_payload_and_fields(vocabulary, overlay_params, &winner, &preferences);

    // Inject the resolved model into payload so the client sees
    // what the provider actually picked. The client can pass this
    // back on the next call via `?model=`.
    if let Some(resolved) = description.resolved_model.as_deref() {
        inject_resolved_model(&mut payload, resolved);
    }

    // Media inputs + examples from the live description.
    let media_inputs_json: Vec<Value> = description
        .media_inputs
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or_default())
        .collect();

    let examples = if !description.examples.is_empty() {
        description.examples.clone()
    } else {
        collect_first_examples(&all_providers_for, primitive, directory).await
    };

    // Skills available for this primitive
    let all_providers = directory.providers().await;
    let mut skills_available: Vec<SkillListEntry> = Vec::new();
    for pc in all_providers.values() {
        if !pc.enabled {
            continue;
        }
        for skill in &pc.announcement.skills {
            if skill.primitive == primitive {
                skills_available.push(SkillListEntry {
                    id: skill.id.clone(),
                    provider: pc.provider.clone(),
                    display: display_from_skill(&skill.display),
                    url: format!("/v1/{}/{}/{}", modality, leaf, skill.id),
                });
            }
        }
    }

    let response = WorkspaceResponse {
        kind: "primitive",
        primitive: primitive.dotted().to_string(),
        skill_id: None,
        display: IntrospectionDisplay {
            name: primitive_display_name(primitive),
            description: Some(primitive.summary().to_string()),
            tags: vec![primitive.modality().as_str().to_string()],
            preview_image: None,
        },
        routing: RoutingInfo {
            providers: all_providers_for.clone(),
            will_run_on: Some(winner),
            status: "healthy".into(),
        },
        invocation: InvocationInfo {
            method: "POST",
            url: format!("/v1/{}/{}", modality, leaf),
            content_type: "application/json",
        },
        payload,
        fields,
        examples,
        skills_available,
        media_inputs: media_inputs_json,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// `GET /v1/{modality}/{leaf}/{skill_id}` — workspace for a skill.
pub async fn get_skill(
    State(state): State<AppState>,
    Path((modality, leaf, skill_id)): Path<(String, String, String)>,
    Query(query): Query<IntrospectQuery>,
) -> Response {
    let primitive = match Primitive::from_segments(&modality, &leaf) {
        Ok(p) => p,
        Err(e) => {
            return quick_error_response(
                ErrorCode::ValidationFailed,
                format!("Unknown primitive `{modality}.{leaf}`: {e}"),
            );
        }
    };

    let directory = &state.capability_directory;
    let providers = directory.providers_for_skill(primitive, &skill_id).await;

    if providers.is_empty() {
        return quick_error_response(
            ErrorCode::NotFound,
            format!("No provider declares skill `{skill_id}` for `{modality}.{leaf}`."),
        );
    }

    let primary_provider = match &query.provider {
        Some(name) => {
            let pn = ProviderName::new(name);
            if providers.contains(&pn) {
                pn
            } else {
                return quick_error_response(
                    ErrorCode::NotFound,
                    format!("Provider `{name}` does not declare skill `{skill_id}`."),
                );
            }
        }
        None => providers[0].clone(),
    };

    let skill = match directory.skill(&primary_provider, &skill_id).await {
        Some(s) => s,
        None => {
            return quick_error_response(
                ErrorCode::NotFound,
                format!("Skill `{skill_id}` disappeared between lookup and fetch."),
            );
        }
    };

    // For skills, parameters come from the skill declaration (already
    // includes overlay). Vocabulary base is still composed underneath.
    let vocabulary = state.vocabularies.get(primitive);
    let preferences = state.preferences.get_all().await;
    let (payload, fields) = compose_payload_and_fields(
        vocabulary,
        &skill.parameters,
        &primary_provider,
        &preferences,
    );

    // Media inputs from the parent capability
    let mut media_inputs_json: Vec<Value> = Vec::new();
    if let Some(cap) = directory.capability(&primary_provider, primitive).await {
        media_inputs_json = cap
            .media_inputs
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or_default())
            .collect();
    }

    let examples = if !skill.examples.is_empty() {
        skill.examples.clone()
    } else if let Some(cap) = directory.capability(&primary_provider, primitive).await {
        cap.examples.clone()
    } else {
        Vec::new()
    };

    let response = WorkspaceResponse {
        kind: "skill",
        primitive: primitive.dotted().to_string(),
        skill_id: Some(skill.id.clone()),
        display: display_from_skill(&skill.display),
        routing: RoutingInfo {
            providers: providers.clone(),
            will_run_on: Some(primary_provider),
            status: "healthy".into(),
        },
        invocation: InvocationInfo {
            method: "POST",
            url: format!("/v1/{}/{}/{}", modality, leaf, skill_id),
            content_type: "application/json",
        },
        payload,
        fields,
        examples,
        skills_available: Vec::new(),
        media_inputs: media_inputs_json,
    };

    (StatusCode::OK, Json(response)).into_response()
}

// ── Unified resolver (ORCH-0038) ─────────────────────────────

/// Walk provider candidates in priority order and ask each one to
/// describe the workspace for the given (primitive, model_hint). The
/// first `Some` result wins — the adapter is the single authority on
/// its own field surface.
///
/// When `?provider=` is set, only that provider is asked. When
/// `?model=` is set, the adapter may return `None` if it doesn't have
/// the requested model, which lets the resolver fall through to the
/// next candidate.
async fn resolve_workspace(
    provider_hint: Option<&str>,
    model_hint: Option<&str>,
    available: &[ProviderName],
    primitive: Primitive,
    state: &AppState,
) -> Option<(ProviderName, WorkspaceDescription)> {
    let directory = &state.capability_directory;

    // Filter + prioritize candidates. Each candidate's priority comes
    // from its startup capability announcement in the directory.
    let mut ranked: Vec<(ProviderName, i32)> = Vec::new();
    for name in available {
        if let Some(hint) = provider_hint {
            if name.as_str() != hint {
                continue;
            }
        }
        let priority = directory
            .capability(name, primitive)
            .await
            .map(|c| c.priority)
            .unwrap_or(i32::MIN);
        ranked.push((name.clone(), priority));
    }
    // Highest priority first; stable by name on tie.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));

    for (name, _) in ranked {
        let provider = match state.provider_registry.get(&name).await {
            Some(p) => p,
            None => continue,
        };
        if let Some(desc) = provider.describe_workspace(primitive, model_hint).await {
            return Some((name, desc));
        }
    }
    None
}

/// Inject `resolved_model` into the payload template at
/// `selectors.model` so the client round-trips it on the next call.
/// The adapter's `selectors.model` field is surfaced to the UI
/// without the `selectors.` prefix (see `compose_payload_and_fields`),
/// so we write to the stripped key `model` as well.
fn inject_resolved_model(payload: &mut Value, resolved: &str) {
    let Some(obj) = payload.as_object_mut() else {
        return;
    };
    obj.insert("model".to_string(), Value::String(resolved.to_string()));
}

/// Collect first non-empty examples across providers for a primitive.
async fn collect_first_examples(
    providers: &[ProviderName],
    primitive: Primitive,
    directory: &crate::services::directory_subscriber::CapabilityDirectory,
) -> Vec<Example> {
    for provider in providers {
        if let Some(cap) = directory.capability(provider, primitive).await {
            if !cap.examples.is_empty() {
                return cap.examples.clone();
            }
        }
    }
    Vec::new()
}

// ── Composition engine ───────────────────────────────────────

/// Compose vocabulary base fields + provider overlay into a payload
/// template and a fields map.
///
/// When the provider declares overlay parameters, ONLY those fields
/// appear in the form — the provider controls what's visible. The
/// vocabulary enriches overlay fields with type information and
/// validation constraints, but vocabulary-only fields don't leak
/// into the UI.
///
/// When the provider declares NO parameters (overlay is empty),
/// vocabulary fields are used as the fallback — this is the
/// bare-primitive case where no provider has claimed form ownership.
fn compose_payload_and_fields(
    vocabulary: &Vocabulary,
    overlay: &[SkillParameter],
    provider: &ProviderName,
    preferences: &std::collections::HashMap<String, Value>,
) -> (Value, BTreeMap<String, FieldDescriptor>) {
    let mut payload = Map::new();
    let mut fields = BTreeMap::new();

    let has_overlay = !overlay.is_empty();

    // Layer 1: vocabulary base fields.
    // Only shown when the provider has NO overlay (bare primitive
    // fallback). When the provider declares parameters, it controls
    // the form surface — vocabulary fields that the overlay doesn't
    // mention are NOT rendered (ORCH-0037).
    if !has_overlay {
        for spec in vocabulary
            .input
            .required
            .iter()
            .chain(vocabulary.input.optional.iter())
        {
            let path = spec.path.as_str();
            let is_required = vocabulary.input.required.iter().any(|r| r.path == spec.path);

            let (type_str, min, max) = field_type_to_strings(&spec.field_type);
            let descriptor = FieldDescriptor {
                label: derive_label(path),
                field_type: type_str.to_string(),
                widget: infer_widget_from_type(&spec.field_type, min.is_some()),
                required: is_required,
                placeholder: None,
                min,
                max,
                step: None,
                options: None,
                auto: None,
                description: Some(spec.description.to_string()),
                source: Some("vocabulary".to_string()),
            };
            fields.insert(path.to_string(), descriptor);

            let pref_value = preferences.get(path).cloned();
            let default = pref_value.or_else(|| default_for_field_type(&spec.field_type));
            set_nested_value(&mut payload, path, default, is_required);
        }
    }

    // Layer 2: provider overlay fields
    for param in overlay {
        let path = &param.field;

        // Strip selectors. prefix for payload/field key
        let payload_path = if path.starts_with("selectors.") {
            path.strip_prefix("selectors.").unwrap().to_string()
        } else {
            path.clone()
        };

        let widget_str = param
            .widget
            .map(|w| {
                serde_json::to_value(w)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("textarea")
                    .to_string()
            })
            .unwrap_or_else(|| infer_widget_from_param(param));

        let pref_value = preferences.get(path.as_str()).cloned();
        let effective_default = pref_value
            .or_else(|| param.default.clone())
            .or_else(|| param.auto.as_ref().map(|a| Value::String(a.default.clone())));

        // Dialogue fields get empty array default
        let default_for_type = match param.widget {
            Some(crate::domain::capability_announcement::ParameterWidget::Dialogue) => {
                Some(Value::Array(Vec::new()))
            }
            _ => None,
        };

        if payload_path.contains('.') {
            set_nested_value(
                &mut payload,
                &payload_path,
                effective_default.clone().or(default_for_type),
                param.required,
            );
        } else {
            if let Some(val) = &effective_default {
                payload.insert(payload_path.clone(), val.clone());
            } else if param.required {
                payload.insert(payload_path.clone(), Value::String(String::new()));
            }
        }

        let field_type = param
            .field_type
            .map(|t| {
                serde_json::to_value(t)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("string")
                    .to_string()
            })
            .unwrap_or_else(|| "string".to_string());

        let field_key = if path.starts_with("selectors.") {
            path.strip_prefix("selectors.").unwrap().to_string()
        } else {
            path.clone()
        };

        let descriptor = FieldDescriptor {
            label: param.label.clone().unwrap_or_else(|| derive_label(&field_key)),
            field_type,
            widget: widget_str,
            required: param.required,
            placeholder: param.placeholder.clone(),
            min: param.min,
            max: param.max,
            step: param.step,
            options: param.options.clone(),
            auto: param.auto.clone(),
            description: param.description.clone(),
            source: Some(provider.as_str().to_string()),
        };
        fields.insert(field_key, descriptor);
    }

    (Value::Object(payload), fields)
}

// ── Helpers ──────────────────────────────────────────────────

fn set_nested_value(root: &mut Map<String, Value>, dotted: &str, value: Option<Value>, required: bool) {
    let parts: Vec<&str> = dotted.split('.').collect();
    if parts.is_empty() {
        return;
    }
    let mut current = root;
    for &segment in &parts[..parts.len() - 1] {
        if !current.contains_key(segment) {
            current.insert(segment.to_string(), Value::Object(Map::new()));
        }
        match current.get_mut(segment) {
            Some(Value::Object(obj)) => current = obj,
            _ => return,
        }
    }
    let leaf = parts[parts.len() - 1];
    match &value {
        Some(v) => { current.insert(leaf.to_string(), v.clone()); }
        None => {
            if required {
                current.insert(leaf.to_string(), Value::String(String::new()));
            }
        }
    }
}

fn field_type_to_strings(ft: &FieldType) -> (&'static str, Option<f64>, Option<f64>) {
    match ft {
        FieldType::String => ("string", None, None),
        FieldType::Integer { min, max } => ("integer", min.map(|v| v as f64), max.map(|v| v as f64)),
        FieldType::Number { min, max } => ("number", *min, *max),
        FieldType::Boolean => ("boolean", None, None),
        FieldType::Array => ("array", None, None),
        FieldType::Object => ("object", None, None),
        FieldType::MediaRef => ("media_ref", None, None),
        FieldType::Dialogue => ("dialogue", None, None),
    }
}

fn default_for_field_type(ft: &FieldType) -> Option<Value> {
    match ft {
        FieldType::Dialogue => Some(Value::Array(Vec::new())),
        _ => None,
    }
}

fn infer_widget_from_type(ft: &FieldType, has_range: bool) -> String {
    match ft {
        FieldType::String => "textarea",
        FieldType::Integer { .. } | FieldType::Number { .. } => {
            if has_range { "slider" } else { "number" }
        }
        FieldType::Boolean => "toggle",
        FieldType::Array => "textarea",
        FieldType::Object => "textarea",
        FieldType::MediaRef => "file",
        FieldType::Dialogue => "dialogue",
    }
    .to_string()
}

fn infer_widget_from_param(param: &SkillParameter) -> String {
    if param.options.is_some() {
        return "select".to_string();
    }
    match param.field_type {
        Some(crate::domain::capability_announcement::ParameterType::Boolean) => "toggle",
        Some(crate::domain::capability_announcement::ParameterType::Number)
        | Some(crate::domain::capability_announcement::ParameterType::Integer) => {
            if param.min.is_some() && param.max.is_some() { "slider" } else { "number" }
        }
        Some(crate::domain::capability_announcement::ParameterType::Dialogue) => "dialogue",
        _ => "textarea",
    }
    .to_string()
}

fn derive_label(path: &str) -> String {
    let leaf = path.split('.').last().unwrap_or(path);
    let mut label = String::new();
    for (i, c) in leaf.chars().enumerate() {
        if i == 0 {
            label.push(c.to_uppercase().next().unwrap_or(c));
        } else if c == '_' {
            label.push(' ');
        } else {
            label.push(c);
        }
    }
    label
}

fn display_from_skill(d: &SkillDisplay) -> IntrospectionDisplay {
    IntrospectionDisplay {
        name: d.name.clone(),
        description: d.description.clone(),
        tags: d.tags.clone(),
        preview_image: d.preview_image.clone(),
    }
}

fn primitive_display_name(primitive: Primitive) -> String {
    match primitive {
        Primitive::TextChat => "Text Chat",
        Primitive::TextTranslate => "Text Translate",
        Primitive::TextEmbed => "Text Embed",
        Primitive::TextRerank => "Text Rerank",
        Primitive::ImageGenerate => "Image Generate",
        Primitive::ImageEdit => "Image Edit",
        Primitive::ImageUpscale => "Image Upscale",
        Primitive::ImageAnalyze => "Image Analyze",
        Primitive::AudioGenerate => "Audio Generate",
        Primitive::AudioTranscribe => "Audio Transcribe",
    }
    .to_string()
}
