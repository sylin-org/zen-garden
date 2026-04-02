//! Skill domain types — declarative mapping-driven skills (ORCH-0018).
//!
//! A skill is a named operation within a capability. Each skill carries:
//! - Content slots: what the user provides (image, text)
//! - Mappings: how user inputs map to workflow template parameters
//! - Workflow: the provider-specific template (e.g., ComfyUI node graph)
//!
//! The execution engine iterates mappings — zero skill-specific branches.
//! Pure domain types — no I/O, no async.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::Capability;

// ── Skill Definition ───────────────────────────────────────────

/// Static skill definition — a singleton registered once at startup.
///
/// Availability is NOT stored here. It's computed from instance readiness
/// at query time (ORCH-0021).
#[derive(Debug, Clone, Serialize)]
pub struct SkillDefinition {
    /// Internal name for API routing: "image.upscale", "image.generate"
    pub name: String,
    /// User-facing display name: "Upscale", "Generate"
    pub display_name: String,
    /// Parent capability for routing.
    pub capability: Capability,
    /// Short description shown as subtitle in the UI.
    pub description: String,
    /// Which provider type handles execution.
    pub provider_kind: super::types::OfferingKind,
    /// Minimum GPU VRAM required to run this skill (MB).
    pub vram_mb: u64,
    /// What inputs the skill requires (image, text).
    pub content_slots: Vec<ContentSlot>,
    /// Declarative mappings: user inputs → workflow template locations.
    pub mappings: Vec<SkillMapping>,
    /// Optional Mermaid diagram of the pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<String>,
    /// Preview image URL (from CivitAI import or user upload).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    /// Models that must be installed for this skill to work.
    pub required_models: Vec<ModelRef>,
    /// Default workflow template name. Overridden by `parameters.workflow` if present.
    pub default_workflow: String,
    /// Named workflow templates. The provider selects one at execution time.
    #[serde(skip)]
    pub workflows: std::collections::HashMap<String, serde_json::Value>,
}

// ── Mappings ──────────────────────────────────────────────────

/// A single mapping from user input to workflow template location.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillMapping {
    /// User content (image/text) → placeholder string in the workflow.
    /// `content_type` determines handling: image = upload first, text = substitute.
    Content {
        role: String,
        content_type: ContentType,
        placeholder: String,
    },
    /// Form parameter → workflow value.
    /// Two targeting methods (mutually exclusive):
    /// - `placeholder`: string substitution throughout the workflow tree
    /// - `node` + `input`: set a specific node's input by node ID
    /// Neither is required (e.g., `field: "workflow"` is consumed by the provider).
    Param {
        field: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        #[serde(flatten)]
        param_type: ParamType,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<serde_json::Value>,
    },
}

/// How a parameter is rendered and serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "param_type", rename_all = "snake_case")]
pub enum ParamType {
    /// Named options with optional display labels.
    /// Simple array: display = wire value. Named: display label differs.
    Options { options: Vec<ParamOption> },
    /// Numeric range with min/max/step.
    Range {
        min: f64,
        max: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// Auto-generated value (e.g., seed). Editable but pre-filled.
    Auto { kind: AutoKind },
    /// Free text input.
    Text,
}

/// A single option in an Options parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamOption {
    /// Wire value sent to the workflow.
    pub value: serde_json::Value,
    /// Display label. None → display the value itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ParamOption {
    /// Simple option: display = wire value.
    pub fn simple(value: impl Into<serde_json::Value>) -> Self {
        Self { value: value.into(), label: None }
    }

    /// Named option: display label differs from wire value.
    pub fn named(value: impl Into<serde_json::Value>, label: impl Into<String>) -> Self {
        Self { value: value.into(), label: Some(label.into()) }
    }
}

/// Auto-generation strategy for a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoKind {
    /// Random u64.
    RandomInt,
}

// ── Readiness (computed, not stored on definition) ─────────────

/// Readiness of a single instance for a specific skill.
#[derive(Debug, Clone, Serialize)]
pub struct SkillReadiness {
    pub ready: bool,
    pub reason: String,
}

/// Skill view for API responses — definition + computed availability.
#[derive(Debug, Clone, Serialize)]
pub struct SkillView {
    /// The static definition.
    #[serde(flatten)]
    pub definition: SkillDefinition,
    /// At least one instance can serve this skill.
    pub available: bool,
    /// Per-instance readiness.
    pub instances: Vec<SkillInstanceView>,
}

/// Per-instance readiness for a skill.
#[derive(Debug, Clone, Serialize)]
pub struct SkillInstanceView {
    pub stone_name: String,
    pub endpoint: String,
    pub ready: bool,
    pub reason: String,
    pub vram_mb: u64,
}

/// Declares a single input slot for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSlot {
    /// Role name: "source", "mask", "prompt", "negative"
    pub role: String,
    /// Expected content type.
    pub content_type: ContentType,
    /// Whether this input must be provided.
    pub required: bool,
    /// If set, render as a paint overlay on the referenced role's image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
    /// Default value for text content slots (e.g., negative prompt default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Content type for input/output blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Image,
    Text,
}

/// A model required by a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Filename as the provider knows it (e.g., "4x-UltraSharp.pth").
    pub filename: String,
    /// Which model directory (e.g., "upscale_models", "checkpoints").
    pub model_type: String,
    /// Download URL. The provider streams from here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Expected file size in bytes (for progress reporting).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Expected SHA-256 checksum ("sha256:{hex}" or just the hex).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// License name (shown in dashboard skill panel).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Workflow Request / Response ─────────────────────────────────

/// A request to execute a skill.
///
/// The `skill` field is populated by the API handler from the URL path
/// (`/v1/{capability}/skill/{moniker}` → `"image.upscale"`).
/// Clients do NOT include it in the request body.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowRequest {
    /// Skill to invoke — filled by the handler, not the client.
    #[serde(default)]
    pub skill: String,
    /// Input content blocks (images, text).
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    /// Tuning parameters (skill-specific).
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// A single input or output content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    /// Content type: "image" or "text".
    #[serde(rename = "type")]
    pub content_type: ContentType,
    /// Role disambiguator: "source", "mask", "prompt", "negative".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Inline base64 data (mutually exclusive with `url`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// URL reference — orchestrator fetches and caches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Output format (set on response content blocks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// A tracked workflow job.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowJob {
    /// Public job identifier (GUIDv7).
    pub id: String,
    /// Skill that was invoked (dotted: "image.upscale").
    pub skill: String,
    /// Current status.
    pub status: WorkflowJobStatus,
    /// Provider-internal job reference (e.g., ComfyUI prompt_id).
    #[serde(skip)]
    pub prompt_id: Option<String>,
    /// Endpoint of the instance that executed this job (for asset proxying).
    #[serde(skip)]
    pub endpoint: Option<String>,
    /// Progress (0.0 to 1.0), if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Result content blocks (populated on completion).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ContentBlock>>,
    /// Error details (populated on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WorkflowError>,
    /// Execution metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<WorkflowUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowUsage {
    pub duration_ms: u64,
}

// ── Skill Form View (API response) ────────────────────────────

/// Combined mappings + diagram for the TryIt UI.
/// Returned by `GET /v1/skills/{skill}/form`.
#[derive(Debug, Clone, Serialize)]
pub struct SkillFormView {
    pub display_name: String,
    pub description: String,
    pub content_slots: Vec<ContentSlot>,
    pub mappings: Vec<SkillMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<String>,
}

impl SkillFormView {
    pub fn from_definition(def: &SkillDefinition) -> Self {
        Self {
            display_name: def.display_name.clone(),
            description: def.description.clone(),
            content_slots: def.content_slots.clone(),
            mappings: def.mappings.clone(),
            diagram: def.diagram.clone(),
        }
    }
}

// ── Skill Registry ─────────────────────────────────────────────

/// Registry of all published skills, keyed by name.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, skill: SkillDefinition) {
        self.skills.insert(skill.name.clone(), skill);
    }

    pub fn remove(&mut self, name: &str) -> Option<SkillDefinition> {
        self.skills.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut SkillDefinition> {
        self.skills.get_mut(name)
    }

    pub fn list(&self) -> Vec<&SkillDefinition> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    pub fn by_capability(&self, capability: Capability) -> Vec<&SkillDefinition> {
        self.skills
            .values()
            .filter(|s| s.capability == capability)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn upscale_skill() -> SkillDefinition {
        let mut workflows = std::collections::HashMap::new();
        workflows.insert("upscale_4x".into(), serde_json::json!({}));

        SkillDefinition {
            name: "image.upscale".into(),
            display_name: "Upscale".into(),
            capability: Capability::Image,
            description: "Enhance image resolution".into(),
            provider_kind: crate::domain::types::OfferingKind::ComfyUi,
            vram_mb: 1024,
            content_slots: vec![ContentSlot {
                role: "source".into(),
                content_type: ContentType::Image,
                required: true,
                overlay: None,
                default: None,
            }],
            mappings: vec![
                SkillMapping::Content {
                    role: "source".into(),
                    content_type: ContentType::Image,
                    placeholder: "PLACEHOLDER_IMAGE".into(),
                },
                SkillMapping::Param {
                    field: "upscale_model".into(),
                    label: "Style".into(),
                    node: None,
                    input: None,
                    placeholder: Some("PLACEHOLDER_MODEL".into()),
                    param_type: ParamType::Options {
                        options: vec![
                            ParamOption::named("RealESRGAN_x4plus.pth", "Realistic"),
                            ParamOption::named("RealESRGAN_x4plus_anime_6B.pth", "Anime"),
                        ],
                    },
                    default: Some(serde_json::json!("RealESRGAN_x4plus.pth")),
                },
            ],
            diagram: Some("graph LR\n  A --> B".into()),
            preview_url: None,
            required_models: vec![ModelRef {
                filename: "RealESRGAN_x4plus.pth".into(),
                model_type: "upscale_models".into(),
                url: None,
                size_bytes: None,
                sha256: None,
                license: None,
                description: Some("4x upscaler".into()),
            }],
            default_workflow: "upscale_4x".into(),
            workflows,
        }
    }

    #[test]
    fn registry_register_and_get() {
        let mut reg = SkillRegistry::new();
        reg.register(upscale_skill());

        assert_eq!(reg.len(), 1);
        assert!(reg.get("image.upscale").is_some());
        assert!(reg.get("image.generate").is_none());
    }

    #[test]
    fn registry_list_sorted() {
        let mut reg = SkillRegistry::new();

        let mut generate = upscale_skill();
        generate.name = "image.generate".into();
        reg.register(generate);
        reg.register(upscale_skill());

        let skills = reg.list();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "image.generate");
        assert_eq!(skills[1].name, "image.upscale");
    }

    #[test]
    fn registry_by_capability() {
        let mut reg = SkillRegistry::new();
        reg.register(upscale_skill());

        assert_eq!(reg.by_capability(Capability::Image).len(), 1);
        assert_eq!(reg.by_capability(Capability::Chat).len(), 0);
    }

    #[test]
    fn registry_remove() {
        let mut reg = SkillRegistry::new();
        reg.register(upscale_skill());
        reg.remove("image.upscale");
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn form_view_from_definition() {
        let skill = upscale_skill();
        let view = SkillFormView::from_definition(&skill);

        assert_eq!(view.display_name, "Upscale");
        assert_eq!(view.content_slots.len(), 1);
        assert_eq!(view.mappings.len(), 2);
        assert!(view.diagram.is_some());
    }

    #[test]
    fn mapping_serde_round_trip() {
        let mapping = SkillMapping::Param {
            field: "seed".into(),
            node: Some("5".into()),
            input: Some("seed".into()),
            placeholder: None,
            label: "Seed".into(),
            param_type: ParamType::Auto { kind: AutoKind::RandomInt },
            default: None,
        };

        let json = serde_json::to_value(&mapping).unwrap();
        assert_eq!(json["type"], "param");
        assert_eq!(json["param_type"], "auto");
        assert_eq!(json["kind"], "random_int");
        assert_eq!(json["field"], "seed");

        let deserialized: SkillMapping = serde_json::from_value(json).unwrap();
        match deserialized {
            SkillMapping::Param { field, param_type: ParamType::Auto { kind }, .. } => {
                assert_eq!(field, "seed");
                assert_eq!(kind, AutoKind::RandomInt);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn param_option_simple() {
        let opt = ParamOption::simple(512);
        assert_eq!(opt.value, serde_json::json!(512));
        assert!(opt.label.is_none());

        let json = serde_json::to_value(&opt).unwrap();
        assert!(json.get("label").is_none());
    }

    #[test]
    fn param_option_named() {
        let opt = ParamOption::named("RealESRGAN_x4plus.pth", "4x");
        assert_eq!(opt.label.as_deref(), Some("4x"));

        let json = serde_json::to_value(&opt).unwrap();
        assert_eq!(json["label"], "4x");
        assert_eq!(json["value"], "RealESRGAN_x4plus.pth");
    }

    #[test]
    fn content_mapping_serde() {
        let mapping = SkillMapping::Content {
            role: "source".into(),
            content_type: ContentType::Image,
            placeholder: "PLACEHOLDER_IMAGE".into(),
        };

        let json = serde_json::to_value(&mapping).unwrap();
        assert_eq!(json["type"], "content");
        assert_eq!(json["content_type"], "image");
        assert_eq!(json["placeholder"], "PLACEHOLDER_IMAGE");
    }

    #[test]
    fn options_param_serde() {
        let mapping = SkillMapping::Param {
            field: "width".into(),
            node: Some("4".into()),
            input: Some("width".into()),
            placeholder: None,
            label: "Width".into(),
            param_type: ParamType::Options {
                options: vec![
                    ParamOption::simple(512),
                    ParamOption::simple(768),
                    ParamOption::simple(1024),
                ],
            },
            default: Some(serde_json::json!(512)),
        };

        let json = serde_json::to_value(&mapping).unwrap();
        assert_eq!(json["param_type"], "options");
        assert_eq!(json["options"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn range_param_serde() {
        let mapping = SkillMapping::Param {
            field: "strength".into(),
            node: Some("6".into()),
            input: Some("denoise".into()),
            placeholder: None,
            label: "Strength".into(),
            param_type: ParamType::Range { min: 0.0, max: 1.0, step: Some(0.05) },
            default: Some(serde_json::json!(0.7)),
        };

        let json = serde_json::to_value(&mapping).unwrap();
        assert_eq!(json["param_type"], "range");
        assert_eq!(json["min"], 0.0);
        assert_eq!(json["max"], 1.0);
        assert_eq!(json["step"], 0.05);
    }

    #[test]
    fn deserialize_workflow_request() {
        let json = serde_json::json!({
            "skill": "image.upscale",
            "content": [
                { "type": "image", "url": "https://example.com/photo.png" }
            ],
            "parameters": { "upscale_model": "RealESRGAN_x4plus.pth" }
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "image.upscale");
        assert_eq!(req.content.len(), 1);
        assert_eq!(req.content[0].content_type, ContentType::Image);
        assert_eq!(req.parameters["upscale_model"], "RealESRGAN_x4plus.pth");
    }

    #[test]
    fn deserialize_minimal_request() {
        let json = serde_json::json!({
            "skill": "image.generate",
            "content": [
                { "type": "text", "role": "prompt", "data": "a cat" }
            ]
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "image.generate");
        assert!(req.parameters.is_null());
    }

    #[test]
    fn serialize_workflow_job_completed() {
        let job = WorkflowJob {
            id: "job-123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Completed,
            prompt_id: Some("comfy-abc".into()),
            endpoint: Some("http://192.168.1.119:8188".into()),
            progress: None,
            content: Some(vec![ContentBlock {
                content_type: ContentType::Image,
                role: None,
                data: None,
                url: Some("/v1/workflows/assets/job-123-result.png".into()),
                format: Some("png".into()),
            }]),
            error: None,
            usage: Some(WorkflowUsage { duration_ms: 3200 }),
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "completed");
        assert_eq!(json["content"][0]["url"], "/v1/workflows/assets/job-123-result.png");
        assert!(json.get("error").is_none());
    }

    #[test]
    fn serialize_workflow_job_completed_skips_internal_fields() {
        let job = WorkflowJob {
            id: "job-123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Completed,
            prompt_id: Some("comfy-abc".into()),
            endpoint: Some("http://192.168.1.119:8188".into()),
            progress: None,
            content: None,
            error: None,
            usage: None,
        };
        let json = serde_json::to_value(&job).unwrap();
        assert!(json.get("prompt_id").is_none(), "prompt_id must not serialize");
        assert!(json.get("endpoint").is_none(), "endpoint must not serialize");
    }

    #[test]
    fn deserialize_request_without_skill() {
        let json = serde_json::json!({
            "content": [{ "type": "image", "role": "source", "data": "base64..." }],
            "parameters": { "zoom": "4x" }
        });
        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "");
        assert_eq!(req.content.len(), 1);
    }

    #[test]
    fn serialize_workflow_job_failed() {
        let job = WorkflowJob {
            id: "job-456".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Failed,
            prompt_id: None,
            endpoint: None,
            progress: None,
            content: None,
            error: Some(WorkflowError {
                code: "model_not_found".into(),
                message: "Upscale model not installed".into(),
            }),
            usage: None,
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error"]["code"], "model_not_found");
        assert!(json.get("content").is_none());
    }
}
