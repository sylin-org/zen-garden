//! Skill domain types — dynamic capabilities published by providers (ORCH-0018).
//!
//! A skill is a named operation within a capability. Providers publish skills
//! based on what's installed (models, workflow templates). The orchestrator
//! routes skill requests to capable instances.
//!
//! Example: ComfyUI with upscale models installed publishes `image.upscale`.
//! The same instance without checkpoints does NOT publish `image.generate`.
//!
//! Pure domain types — no I/O, no async.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::Capability;
use crate::catalog::traits::FormSchema;

// ── Skill Definition ───────────────────────────────────────────

/// A named operation that a provider publishes.
///
/// Skills bridge the orchestrator's capability model to a provider's
/// concrete implementation. ComfyUI implements skills as workflow
/// templates; other providers implement them as direct API calls.
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
    /// Current lifecycle status.
    pub status: SkillStatus,
    /// Minimum GPU VRAM required to run this skill (MB).
    /// Instances below this threshold won't be provisioned.
    pub vram_mb: u64,
    /// What inputs the skill requires.
    pub content_slots: Vec<ContentSlot>,
    /// Tuning parameters (JSON Schema + RJSF UI Schema).
    pub parameter_schema: FormSchema,
    /// Optional Mermaid diagram of the pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<String>,
    /// Models that must be installed for this skill to work.
    pub required_models: Vec<ModelRef>,
    /// Provider-specific implementation data (e.g., ComfyUI workflow template JSON).
    #[serde(skip)]
    pub implementation: serde_json::Value,
}

/// Lifecycle status of a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    /// Skill registered, models being downloaded to orchestrator cache.
    Initializing,
    /// Models cached, being pushed to instances.
    Provisioning,
    /// At least one instance fully provisioned — skill accepts requests.
    Ready,
    /// Some instances ready, others still provisioning.
    Degraded,
    /// Provisioning failed.
    Failed,
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
    /// Filename as ComfyUI knows it (e.g., "4x-UltraSharp.pth").
    pub filename: String,
    /// Which model directory (e.g., "upscale_models", "checkpoints").
    pub model_type: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Workflow Request / Response ─────────────────────────────────

/// A request to execute a skill.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowRequest {
    /// Skill to invoke: "image.upscale", "image.generate", etc.
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
    /// Unique job identifier.
    pub id: String,
    /// Skill that was invoked.
    pub skill: String,
    /// Current status.
    pub status: WorkflowJobStatus,
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

// ── Skill Registry ─────────────────────────────────────────────

/// Registry of all published skills, keyed by name.
///
/// Populated at discovery time when providers report their installed
/// models and available operations.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or update a skill.
    pub fn register(&mut self, skill: SkillDefinition) {
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Remove a skill by name.
    pub fn remove(&mut self, name: &str) -> Option<SkillDefinition> {
        self.skills.remove(name)
    }

    /// Look up a skill by name.
    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// Look up a skill mutably (for status updates).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut SkillDefinition> {
        self.skills.get_mut(name)
    }

    /// List all registered skills.
    pub fn list(&self) -> Vec<&SkillDefinition> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    /// List skills for a specific capability.
    pub fn by_capability(&self, capability: Capability) -> Vec<&SkillDefinition> {
        self.skills
            .values()
            .filter(|s| s.capability == capability)
            .collect()
    }

    /// Check if any skills are registered.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }
}

// ── Presentation ───────────────────────────────────────────────

/// Combined schema + diagram for the TryIt UI.
#[derive(Debug, Clone, Serialize)]
pub struct SkillPresentation {
    /// User-facing display name.
    pub display_name: String,
    /// Short description.
    pub description: String,
    /// JSON Schema + UI Schema for parameter form.
    pub schema: serde_json::Value,
    pub ui_schema: serde_json::Value,
    /// What content inputs the skill expects.
    pub content: Vec<ContentSlot>,
    /// Optional Mermaid diagram of the pipeline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<String>,
}

impl SkillPresentation {
    pub fn from_definition(def: &SkillDefinition) -> Self {
        Self {
            display_name: def.display_name.clone(),
            description: def.description.clone(),
            schema: def.parameter_schema.schema.clone(),
            ui_schema: def.parameter_schema.ui_schema.clone(),
            content: def.content_slots.clone(),
            diagram: def.diagram.clone(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn upscale_skill() -> SkillDefinition {
        SkillDefinition {
            name: "image.upscale".into(),
            display_name: "Upscale".into(),
            capability: Capability::Image,
            description: "Enhance image resolution using AI super-resolution".into(),
            status: SkillStatus::Ready,
            vram_mb: 1024,
            content_slots: vec![ContentSlot {
                role: "source".into(),
                content_type: ContentType::Image,
                required: true,
            }],
            parameter_schema: FormSchema {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "scale": { "type": "integer", "enum": [2, 4], "default": 4 },
                        "upscale_model": { "type": "string" }
                    }
                }),
                ui_schema: serde_json::json!({}),
            },
            diagram: Some("graph LR\n    A[Load Image] --> C[Upscale]\n    B[Load Model] --> C\n    C --> D[Save Image]".into()),
            required_models: vec![ModelRef {
                filename: "4x-UltraSharp.pth".into(),
                model_type: "upscale_models".into(),
                description: Some("General-purpose 4x upscaler".into()),
            }],
            implementation: serde_json::json!({}),
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
        assert_eq!(reg.len(), 1);

        reg.remove("image.upscale");
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn presentation_from_definition() {
        let skill = upscale_skill();
        let pres = SkillPresentation::from_definition(&skill);

        assert!(pres.diagram.is_some());
        assert_eq!(pres.content.len(), 1);
        assert_eq!(pres.content[0].role, "source");
    }

    #[test]
    fn deserialize_workflow_request() {
        let json = serde_json::json!({
            "skill": "image.upscale",
            "content": [
                { "type": "image", "url": "https://example.com/photo.png" }
            ],
            "parameters": { "scale": 4 }
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "image.upscale");
        assert_eq!(req.content.len(), 1);
        assert_eq!(req.content[0].content_type, ContentType::Image);
        assert_eq!(req.content[0].url.as_deref(), Some("https://example.com/photo.png"));
        assert_eq!(req.parameters["scale"], 4);
    }

    #[test]
    fn deserialize_minimal_request() {
        let json = serde_json::json!({
            "skill": "image.remove_bg",
            "content": [
                { "type": "image", "data": "base64..." }
            ]
        });

        let req: WorkflowRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.skill, "image.remove_bg");
        assert!(req.parameters.is_null());
    }

    #[test]
    fn serialize_workflow_job_completed() {
        let job = WorkflowJob {
            id: "job-123".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Completed,
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
        assert!(json.get("error").is_none()); // skipped when None
    }

    #[test]
    fn serialize_workflow_job_failed() {
        let job = WorkflowJob {
            id: "job-456".into(),
            skill: "image.upscale".into(),
            status: WorkflowJobStatus::Failed,
            progress: None,
            content: None,
            error: Some(WorkflowError {
                code: "model_not_found".into(),
                message: "Upscale model '4x-UltraSharp.pth' not installed".into(),
            }),
            usage: None,
        };

        let json = serde_json::to_value(&job).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error"]["code"], "model_not_found");
        assert!(json.get("content").is_none()); // skipped when None
    }
}
