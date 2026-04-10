//! `GET /v1/{modality}/{leaf}[/{skill_id}]` — workspace definition
//! (ORCH-0036).
//!
//! Every invocable URL is also a GET introspection URL. The handler
//! returns everything the dashboard needs to render a workspace form
//! and dispatch:
//!
//! - **Display** — human-facing name, description, tags.
//! - **Routing** — providers, health, who would handle the call.
//! - **Invocation** — method, URL, content-type.
//! - **Payload** — pre-assembled dispatch body with defaults and
//!   empty required fields. POST-able as-is.
//! - **Fields** — map of dotted-path → widget descriptor. Rendering
//!   instructions only; values live in the payload.
//! - **Examples** — named scenarios that fill the form.
//!
//! This endpoint supersedes `GET /v1/catalog/{mod}/{leaf}[/{skill}]`
//! (ORCH-0036).

#![allow(dead_code)]

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::app_state::AppState;
use crate::domain::capability_announcement::{
    AutoDescriptor, Example, SkillDeclaration, SkillDisplay, SkillParameter,
};
use crate::domain::errors::ErrorCode;
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;

use super::errors::quick_error_response;

// ── Response shapes ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceResponse {
    pub kind: &'static str, // "primitive" or "skill"
    pub primitive: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    pub display: IntrospectionDisplay,
    pub routing: RoutingInfo,
    pub invocation: InvocationInfo,
    /// Pre-assembled dispatch body with defaults applied.
    pub payload: Value,
    /// Dotted-path → widget descriptor. Rendering instructions only.
    pub fields: BTreeMap<String, FieldDescriptor>,
    /// Named example scenarios.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,
    /// Skills available under this primitive (only for bare primitives).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills_available: Vec<SkillListEntry>,
    /// Media inputs declared by the capability.
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_providers: Vec<ProviderName>,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillListEntry {
    pub id: String,
    pub provider: ProviderName,
    pub display: IntrospectionDisplay,
    pub url: String,
}

// ── Handlers ─────────────────────────────────────────────────

/// `GET /v1/{modality}/{leaf}` — workspace definition for a primitive.
pub async fn get_primitive(
    State(state): State<AppState>,
    Path((modality, leaf)): Path<(String, String)>,
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
    let providers = directory.providers_for_primitive(primitive).await;

    // Collect parameters, media_inputs, and examples from providers
    // (first non-empty wins for each).
    let mut parameters: Vec<SkillParameter> = Vec::new();
    let mut media_inputs_json: Vec<Value> = Vec::new();
    let mut examples: Vec<Example> = Vec::new();

    for provider in &providers {
        if let Some(cap) = directory.capability(provider, primitive).await {
            if parameters.is_empty() && !cap.parameters.is_empty() {
                parameters = cap.parameters.clone();
            }
            if media_inputs_json.is_empty() && !cap.media_inputs.is_empty() {
                media_inputs_json = cap
                    .media_inputs
                    .iter()
                    .map(|m| serde_json::to_value(m).unwrap_or_default())
                    .collect();
            }
            if examples.is_empty() && !cap.examples.is_empty() {
                examples = cap.examples.clone();
            }
        }
    }

    // Collect skills available for this primitive
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

    // Build payload template and fields map
    let (payload, fields) = build_payload_and_fields(&parameters, &state).await;

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
        routing: routing_from_providers(&providers),
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

/// `GET /v1/{modality}/{leaf}/{skill_id}` — workspace definition for a skill.
pub async fn get_skill(
    State(state): State<AppState>,
    Path((modality, leaf, skill_id)): Path<(String, String, String)>,
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

    let primary_provider = &providers[0];
    let skill = match directory.skill(primary_provider, &skill_id).await {
        Some(s) => s,
        None => {
            return quick_error_response(
                ErrorCode::NotFound,
                format!("Skill `{skill_id}` disappeared between lookup and fetch."),
            );
        }
    };

    // Media inputs from the parent capability
    let mut media_inputs_json: Vec<Value> = Vec::new();
    if let Some(cap) = directory.capability(primary_provider, primitive).await {
        if !cap.media_inputs.is_empty() {
            media_inputs_json = cap
                .media_inputs
                .iter()
                .map(|m| serde_json::to_value(m).unwrap_or_default())
                .collect();
        }
    }

    // Use skill examples, fall back to capability examples
    let examples = if !skill.examples.is_empty() {
        skill.examples.clone()
    } else if let Some(cap) = directory.capability(primary_provider, primitive).await {
        cap.examples.clone()
    } else {
        Vec::new()
    };

    let (payload, fields) = build_payload_and_fields(&skill.parameters, &state).await;

    let response = WorkspaceResponse {
        kind: "skill",
        primitive: primitive.dotted().to_string(),
        skill_id: Some(skill.id.clone()),
        display: display_from_skill(&skill.display),
        routing: routing_from_providers(&providers),
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

// ── Payload + fields builder ─────────────────────────────────

/// Build the pre-assembled payload template and the fields map from
/// a parameter list. The payload contains defaults and empty required
/// fields. The fields map contains widget descriptors keyed by path.
async fn build_payload_and_fields(
    parameters: &[SkillParameter],
    state: &AppState,
) -> (Value, BTreeMap<String, FieldDescriptor>) {
    let preferences = state.preferences.get_all().await;
    let mut payload = Map::new();
    let mut fields = BTreeMap::new();

    for param in parameters {
        // Skip hidden fields that have no user-facing widget
        let widget_str = param
            .widget
            .map(|w| format!("{}", serde_json::to_value(w).unwrap_or_default()).trim_matches('"').to_string())
            .unwrap_or_else(|| infer_widget(param));

        // Determine effective default: preference > skill default > auto default
        let pref_value = preferences.get(param.field.as_str()).cloned();
        let effective_default = pref_value
            .or_else(|| param.default.clone())
            .or_else(|| param.auto.as_ref().map(|a| Value::String(a.default.clone())));

        // Set value in the payload template.
        // Fields with "selectors." prefix are routing directives —
        // strip the prefix and place at the root level (ORCH-0036).
        let payload_path = if param.field.starts_with("selectors.") {
            param.field.strip_prefix("selectors.").unwrap().to_string()
        } else {
            param.field.clone()
        };

        if payload_path.contains('.') {
            // Nested path (vocabulary field): e.g. "text.prompt.user"
            let default_for_type = match param.widget {
                Some(crate::domain::capability_announcement::ParameterWidget::Dialogue) => {
                    Some(Value::Array(Vec::new()))
                }
                _ => None,
            };
            set_nested_value(
                &mut payload,
                &payload_path,
                effective_default.clone().or(default_for_type),
                param.required,
            );
        } else {
            // Top-level key (routing directive): e.g. "model"
            if let Some(val) = &effective_default {
                payload.insert(payload_path.clone(), val.clone());
            } else if param.required {
                payload.insert(payload_path.clone(), Value::String(String::new()));
            }
        }

        // Build field descriptor
        let field_type = param
            .field_type
            .map(|t| format!("{}", serde_json::to_value(t).unwrap_or_default()).trim_matches('"').to_string())
            .unwrap_or_else(|| "string".to_string());

        let descriptor = FieldDescriptor {
            label: param.label.clone().unwrap_or_else(|| {
                // Derive label from field path: "text.prompt.user" → "User"
                let f = &param.field;
                f.split('.').last().unwrap_or(f).to_string()
            }),
            field_type,
            widget: widget_str.clone(),
            required: param.required,
            placeholder: param.placeholder.clone(),
            min: param.min,
            max: param.max,
            step: param.step,
            options: param.options.clone(),
            auto: param.auto.clone(),
            description: param.description.clone(),
        };

        // Use the payload path as the field key so lookups match.
        let field_key = if param.field.starts_with("selectors.") {
            param.field.strip_prefix("selectors.").unwrap().to_string()
        } else {
            param.field.clone()
        };
        fields.insert(field_key, descriptor);
    }

    (Value::Object(payload), fields)
}

/// Set a value at a dotted path in a JSON object, creating
/// intermediate objects as needed.
fn set_nested_value(
    root: &mut Map<String, Value>,
    dotted: &str,
    value: Option<Value>,
    required: bool,
) {
    let parts: Vec<&str> = dotted.split('.').collect();
    if parts.is_empty() {
        return;
    }

    // Navigate to the parent, creating intermediate objects
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
        Some(v) => {
            current.insert(leaf.to_string(), v.clone());
        }
        None => {
            if required {
                // Required field with no default: empty string
                current.insert(leaf.to_string(), Value::String(String::new()));
            }
            // Optional with no default: not included in payload
        }
    }
}

fn infer_widget(param: &SkillParameter) -> String {
    if param.options.is_some() {
        return "select".to_string();
    }
    match param.field_type {
        Some(crate::domain::capability_announcement::ParameterType::Boolean) => "toggle",
        Some(crate::domain::capability_announcement::ParameterType::Number)
        | Some(crate::domain::capability_announcement::ParameterType::Integer) => {
            if param.min.is_some() && param.max.is_some() {
                "slider"
            } else {
                "number"
            }
        }
        Some(crate::domain::capability_announcement::ParameterType::Dialogue) => "dialogue",
        _ => "textarea",
    }
    .to_string()
}

// ── Helpers ──────────────────────────────────────────────────

fn display_from_skill(d: &SkillDisplay) -> IntrospectionDisplay {
    IntrospectionDisplay {
        name: d.name.clone(),
        description: d.description.clone(),
        tags: d.tags.clone(),
        preview_image: d.preview_image.clone(),
    }
}

fn routing_from_providers(providers: &[ProviderName]) -> RoutingInfo {
    if providers.is_empty() {
        RoutingInfo {
            providers: Vec::new(),
            will_run_on: None,
            fallback_providers: Vec::new(),
            status: "unavailable".into(),
        }
    } else {
        let will_run_on = Some(providers[0].clone());
        let fallback_providers = providers.iter().skip(1).cloned().collect();
        RoutingInfo {
            providers: providers.to_vec(),
            will_run_on,
            fallback_providers,
            status: "healthy".into(),
        }
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
