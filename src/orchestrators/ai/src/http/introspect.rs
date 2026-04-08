//! `GET /v1/{modality}/{leaf}[/{skill_id}]` — skill and primitive
//! introspection (ORCH-0030 §R2.8.3).
//!
//! Every invocable URL is also a GET introspection URL. The handler
//! returns the full object model the caller needs to build a
//! successful POST to the same path:
//!
//! - **Identity** — primitive and optional skill id.
//! - **Display** — human-facing name/description/tags (from the
//!   `CapabilityAnnouncement` declared by the adapter, if any).
//! - **Routing** — which providers declared this primitive or skill,
//!   which one would run it right now (`will_run_on`), and the
//!   fallback chain. Live answer, not static.
//! - **Invocation** — method, URL, content-type. Self-describing.
//! - **Parameters** — for skills, the declared parameter list with
//!   `effective_default` resolved through the preferences layer
//!   (commit 12 plugs in here without code changes).
//! - **Example** — a minimal body the caller can POST.
//!
//! The handler supports three cases:
//!
//! 1. `GET /v1/{modality}/{leaf}` with a valid primitive → `kind:
//!    "primitive"` response, including a `skills_available` list of
//!    all skills declared for this primitive by any provider.
//! 2. `GET /v1/{modality}/{leaf}/{skill_id}` with a valid primitive
//!    and a skill id declared by at least one provider → `kind:
//!    "skill"` response.
//! 3. Unknown primitive or unknown skill → 404 with error envelope.

#![allow(dead_code)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::capability_announcement::{
    AutoDescriptor, SkillDeclaration, SkillDisplay, SkillParameter,
};
use crate::domain::errors::ErrorCode;
use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;

use super::errors::quick_error_response;

// ── Response shapes ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum IntrospectionResponse {
    Primitive(PrimitiveIntrospection),
    Skill(SkillIntrospection),
}

#[derive(Debug, Clone, Serialize)]
pub struct PrimitiveIntrospection {
    pub primitive: String,
    pub display: IntrospectionDisplay,
    pub routing: RoutingInfo,
    pub invocation: InvocationInfo,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skills_available: Vec<SkillListEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillIntrospection {
    pub primitive: String,
    pub skill_id: String,
    pub display: IntrospectionDisplay,
    pub routing: RoutingInfo,
    pub invocation: InvocationInfo,
    pub parameters: Vec<ParameterView>,
    pub example: ExampleInvocation,
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
    /// All providers that declared this capability/skill and are
    /// currently enabled.
    pub providers: Vec<ProviderName>,
    /// Which provider would actually handle the call right now.
    /// Resolved by preference ranking (commit 12); for now this is
    /// the first provider in the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_run_on: Option<ProviderName>,
    /// Alternate providers in order of preference, used if the
    /// primary provider fails.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback_providers: Vec<ProviderName>,
    /// High-level health indicator. `"healthy"` when at least one
    /// provider is available; `"unavailable"` when none.
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvocationInfo {
    pub method: &'static str,
    pub url: String,
    pub content_type: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillListEntry {
    pub id: String,
    pub provider: ProviderName,
    pub display: IntrospectionDisplay,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterView {
    pub field: String,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_default: Option<Value>,
    /// Where the effective default came from: `"skill"`, `"preferences"`,
    /// or `null` when both are absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_source: Option<&'static str>,
    #[serde(default)]
    pub pinnable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<AutoDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExampleInvocation {
    pub url: String,
    pub body: Value,
}

// ── Handlers ─────────────────────────────────────────────────

/// `GET /v1/{modality}/{leaf}` — describe a bare primitive.
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

    let capability_directory = &state.capability_directory;
    let providers = capability_directory
        .providers_for_primitive(primitive)
        .await;

    // Collect all skills declared for this primitive across every
    // enabled provider.
    let all_providers = capability_directory.providers().await;
    let mut skills_available: Vec<SkillListEntry> = Vec::new();
    for provider_caps in all_providers.values() {
        if !provider_caps.enabled {
            continue;
        }
        for skill in &provider_caps.announcement.skills {
            if skill.primitive == primitive {
                skills_available.push(SkillListEntry {
                    id: skill.id.clone(),
                    provider: provider_caps.provider.clone(),
                    display: display_from_skill_display(&skill.display),
                    url: format!("/v1/{}/{}/{}", modality, leaf, skill.id),
                });
            }
        }
    }

    let routing = routing_from_providers(&providers);
    let response = PrimitiveIntrospection {
        primitive: primitive.dotted().to_string(),
        display: IntrospectionDisplay {
            name: primitive_display_name(primitive),
            description: Some(primitive.summary().to_string()),
            tags: vec![primitive.modality().as_str().to_string()],
            preview_image: None,
        },
        routing,
        invocation: InvocationInfo {
            method: "POST",
            url: format!("/v1/{}/{}", modality, leaf),
            content_type: "application/json",
        },
        skills_available,
    };

    (StatusCode::OK, Json(IntrospectionResponse::Primitive(response))).into_response()
}

/// `GET /v1/{modality}/{leaf}/{skill_id}` — describe a named skill.
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

    let capability_directory = &state.capability_directory;
    let providers = capability_directory
        .providers_for_skill(primitive, &skill_id)
        .await;

    if providers.is_empty() {
        return quick_error_response(
            ErrorCode::NotFound,
            format!(
                "No provider declares skill `{skill_id}` for primitive `{modality}.{leaf}`."
            ),
        );
    }

    // Pick the first provider's skill declaration (they all share
    // the same id; if skill content differs between providers,
    // that's an adapter-level inconsistency the dispatcher handles
    // via preference ranking).
    let primary_provider = &providers[0];
    let skill = match capability_directory
        .skill(primary_provider, &skill_id)
        .await
    {
        Some(s) => s,
        None => {
            return quick_error_response(
                ErrorCode::NotFound,
                format!("Skill `{skill_id}` disappeared between lookup and fetch."),
            );
        }
    };

    let parameters = skill
        .parameters
        .iter()
        .map(|p| parameter_view_from_declaration(p))
        .collect::<Vec<_>>();

    let example = build_example_invocation(&skill, &modality, &leaf, &skill_id);
    let routing = routing_from_providers(&providers);

    let response = SkillIntrospection {
        primitive: primitive.dotted().to_string(),
        skill_id: skill.id.clone(),
        display: display_from_skill_display(&skill.display),
        routing,
        invocation: InvocationInfo {
            method: "POST",
            url: format!("/v1/{}/{}/{}", modality, leaf, skill_id),
            content_type: "application/json",
        },
        parameters,
        example,
    };

    (StatusCode::OK, Json(IntrospectionResponse::Skill(response))).into_response()
}

// ── Helpers ──────────────────────────────────────────────────

fn display_from_skill_display(d: &SkillDisplay) -> IntrospectionDisplay {
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

fn parameter_view_from_declaration(param: &SkillParameter) -> ParameterView {
    // TODO(commit 12): layer preferences over the skill default
    // when the Preferences domain lands. For now the effective
    // default equals the skill default.
    let effective_default = param.default.clone();
    let default_source = match (&param.default, &effective_default) {
        (Some(_), Some(_)) => Some("skill"),
        _ => None,
    };

    ParameterView {
        field: param.field.clone(),
        required: param.required,
        description: param.description.clone(),
        default: param.default.clone(),
        effective_default,
        default_source,
        pinnable: param.pinnable,
        auto: param.auto.clone(),
    }
}

fn build_example_invocation(
    skill: &SkillDeclaration,
    modality: &str,
    leaf: &str,
    skill_id: &str,
) -> ExampleInvocation {
    let mut body = serde_json::Map::new();
    for param in &skill.parameters {
        if param.required {
            body.insert(
                param.field.clone(),
                example_value_for_field(&param.field),
            );
        }
    }
    ExampleInvocation {
        url: format!("/v1/{}/{}/{}", modality, leaf, skill_id),
        body: Value::Object(body),
    }
}

fn example_value_for_field(field: &str) -> Value {
    // Provide sensible example placeholders for common field shapes.
    if field.ends_with(".source") {
        json!("@upload:abc123")
    } else if field.ends_with(".positive") || field.ends_with(".user") {
        json!("example prompt")
    } else if field.ends_with(".steps") {
        json!(28)
    } else if field.ends_with(".guidance") {
        json!(3.5)
    } else {
        Value::Null
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

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::capability_announcement::{Capability, CapabilityAnnouncement};

    fn provider() -> ProviderName {
        ProviderName::new("ollama")
    }

    fn other_provider() -> ProviderName {
        ProviderName::new("anthropic")
    }

    #[test]
    fn routing_from_empty_providers_unavailable() {
        let routing = routing_from_providers(&[]);
        assert_eq!(routing.status, "unavailable");
        assert!(routing.will_run_on.is_none());
        assert!(routing.providers.is_empty());
        assert!(routing.fallback_providers.is_empty());
    }

    #[test]
    fn routing_from_single_provider_healthy() {
        let routing = routing_from_providers(&[provider()]);
        assert_eq!(routing.status, "healthy");
        assert_eq!(routing.will_run_on, Some(provider()));
        assert!(routing.fallback_providers.is_empty());
    }

    #[test]
    fn routing_picks_first_and_fallbacks() {
        let routing = routing_from_providers(&[provider(), other_provider()]);
        assert_eq!(routing.will_run_on, Some(provider()));
        assert_eq!(routing.fallback_providers, vec![other_provider()]);
    }

    #[test]
    fn parameter_view_with_skill_default_reports_skill_source() {
        let param = SkillParameter {
            field: "selectors.model".into(),
            required: false,
            description: None,
            default: Some(json!("recommended:vision")),
            auto: None,
            pinnable: true,
        };
        let view = parameter_view_from_declaration(&param);
        assert_eq!(view.default_source, Some("skill"));
        assert_eq!(view.effective_default, Some(json!("recommended:vision")));
    }

    #[test]
    fn parameter_view_without_default_reports_no_source() {
        let param = SkillParameter {
            field: "image.source".into(),
            required: true,
            description: None,
            default: None,
            auto: None,
            pinnable: false,
        };
        let view = parameter_view_from_declaration(&param);
        assert_eq!(view.default_source, None);
        assert!(view.effective_default.is_none());
    }

    #[test]
    fn example_includes_required_fields_only() {
        let skill = SkillDeclaration {
            id: "test".into(),
            primitive: Primitive::ImageAnalyze,
            display: SkillDisplay::new("Test"),
            parameters: vec![
                SkillParameter {
                    field: "image.source".into(),
                    required: true,
                    description: None,
                    default: None,
                    auto: None,
                    pinnable: false,
                },
                SkillParameter {
                    field: "selectors.model".into(),
                    required: false,
                    description: None,
                    default: Some(json!("recommended:vision")),
                    auto: None,
                    pinnable: true,
                },
            ],
        };
        let example = build_example_invocation(&skill, "image", "analyze", "test");
        let obj = example.body.as_object().unwrap();
        assert!(obj.contains_key("image.source"));
        assert!(!obj.contains_key("selectors.model"));
    }

    #[test]
    fn example_value_for_source_field_is_upload_placeholder() {
        assert_eq!(example_value_for_field("image.source"), json!("@upload:abc123"));
        assert_eq!(example_value_for_field("audio.source"), json!("@upload:abc123"));
    }

    #[test]
    fn example_value_for_prompt_positive_is_string() {
        assert!(example_value_for_field("image.prompt.positive").is_string());
    }

    #[test]
    fn example_value_for_unknown_field_is_null() {
        assert!(example_value_for_field("random.unknown").is_null());
    }

    #[test]
    fn primitive_display_name_covers_all_primitives() {
        for p in Primitive::ALL {
            let name = primitive_display_name(*p);
            assert!(!name.is_empty());
            // Each name is title case: first char is uppercase.
            assert!(name.chars().next().unwrap().is_uppercase());
        }
    }

    #[test]
    fn introspection_response_serializes_with_kind_tag() {
        let response = IntrospectionResponse::Primitive(PrimitiveIntrospection {
            primitive: "text.chat".into(),
            display: IntrospectionDisplay {
                name: "Text Chat".into(),
                description: Some("chat".into()),
                tags: vec!["text".into()],
                preview_image: None,
            },
            routing: routing_from_providers(&[provider()]),
            invocation: InvocationInfo {
                method: "POST",
                url: "/v1/text/chat".into(),
                content_type: "application/json",
            },
            skills_available: vec![],
        });
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"kind\":\"primitive\""));
        assert!(json.contains("\"primitive\":\"text.chat\""));
    }

    #[test]
    fn skill_response_serializes_with_kind_tag() {
        let response = IntrospectionResponse::Skill(SkillIntrospection {
            primitive: "image.analyze".into(),
            skill_id: "image-understanding".into(),
            display: IntrospectionDisplay {
                name: "Image Understanding".into(),
                description: None,
                tags: vec![],
                preview_image: None,
            },
            routing: routing_from_providers(&[provider()]),
            invocation: InvocationInfo {
                method: "POST",
                url: "/v1/image/analyze/image-understanding".into(),
                content_type: "application/json",
            },
            parameters: vec![],
            example: ExampleInvocation {
                url: "/v1/image/analyze/image-understanding".into(),
                body: json!({}),
            },
        });
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"kind\":\"skill\""));
        assert!(json.contains("\"skill_id\":\"image-understanding\""));
    }
}
