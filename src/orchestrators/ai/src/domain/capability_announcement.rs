//! Capability announcement types — the bottom-up contract between
//! adapters and the Directory (ORCH-0030 §R2.2, §R2.8).
//!
//! An adapter publishes a [`CapabilityAnnouncement`] to the bus under
//! topic `directory.provider.{name}.capabilities` whenever its
//! internal capability set changes. The [`DirectorySubscriber`]
//! consumes these events, validates them, and rebuilds the Directory's
//! view of the provider wholesale on each announcement.
//!
//! # Two independent lists
//!
//! Every announcement carries two parallel lists:
//!
//! - **`capabilities`** — base primitives the adapter can serve
//!   natively through its regular model/instance machinery. The
//!   caller sends the primitive's standard honored fields; the
//!   adapter picks a model (if applicable), picks an instance, and
//!   runs it.
//! - **`skills`** — named, pre-configured invocations of a primitive.
//!   The adapter bakes some fields (system prompts, workflow graphs,
//!   output shaping rules) and exposes a reduced or renamed parameter
//!   surface. Every skill references a primitive that must appear in
//!   `capabilities`.
//!
//! An adapter can publish `0..N` of each, independently. An adapter
//! with zero capabilities is effectively disabled. An adapter with
//! capabilities but zero skills is a bare-primitive provider. An
//! adapter can add or remove skills while its capability set stays
//! stable.
//!
//! # Wire format: atomic full snapshot
//!
//! An announcement is a **full replacement** of the provider's state.
//! No deltas. The `DirectorySubscriber` rebuilds its view of the
//! provider wholesale on each received announcement; deletions are
//! expressed by simply omitting the removed entries from the next
//! snapshot.
//!
//! # Invariants
//!
//! 1. Every skill's `primitive` must appear in `capabilities`. If
//!    not, [`DirectorySubscriber`] rejects the announcement with
//!    [`AnnouncementError::SkillWithoutCapability`]. No implicit
//!    derivation.
//! 2. Skill ids are unique within a single announcement. Duplicates
//!    are rejected with [`AnnouncementError::DuplicateSkillId`].
//! 3. Every capability's `primitive` must be in the locked
//!    [`Primitive`] enum (enforced by deserialization).
//! 4. Preview images are URLs served out-of-band, not inline bytes.
//!    Announcements stay small.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::ids::ProviderName;
use crate::domain::primitive::Primitive;

// ── The announcement ─────────────────────────────────────────

/// The full wire shape of a `directory.provider.{name}.capabilities`
/// event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityAnnouncement {
    /// Which provider this announcement is for. The
    /// `DirectorySubscriber` validates that this matches the topic
    /// suffix to prevent spoofing.
    pub provider: ProviderName,

    /// Whether the provider is currently serving traffic. A provider
    /// with `enabled: false` drops out of all routing decisions
    /// regardless of declared capabilities — useful for operator-
    /// triggered drain/disable without changing the capability list.
    pub enabled: bool,

    /// Base primitives this adapter can serve natively.
    #[serde(default)]
    pub capabilities: Vec<Capability>,

    /// Named, pre-configured invocations layered on top of
    /// capabilities.
    #[serde(default)]
    pub skills: Vec<SkillDeclaration>,
}

impl CapabilityAnnouncement {
    /// Construct an empty announcement representing a disabled
    /// provider with no published capabilities. Useful as a default
    /// for adapters that haven't probed yet.
    pub fn empty(provider: ProviderName) -> Self {
        Self {
            provider,
            enabled: false,
            capabilities: Vec::new(),
            skills: Vec::new(),
        }
    }

    /// Validate structural invariants. Called by
    /// [`DirectorySubscriber`] on every received announcement before
    /// the Directory's view is updated.
    pub fn validate(&self) -> Result<(), AnnouncementError> {
        // Invariant 2: skill ids are unique within the announcement.
        let mut seen_ids: HashSet<&str> = HashSet::new();
        for skill in &self.skills {
            if !seen_ids.insert(skill.id.as_str()) {
                return Err(AnnouncementError::DuplicateSkillId {
                    provider: self.provider.clone(),
                    skill_id: skill.id.clone(),
                });
            }
        }

        // Invariant 1: every skill's primitive must be declared as a
        // capability. We build a set of declared primitives once and
        // membership-test each skill against it.
        let declared: HashSet<Primitive> =
            self.capabilities.iter().map(|c| c.primitive).collect();
        for skill in &self.skills {
            if !declared.contains(&skill.primitive) {
                return Err(AnnouncementError::SkillWithoutCapability {
                    provider: self.provider.clone(),
                    skill_id: skill.id.clone(),
                    primitive: skill.primitive,
                });
            }
        }

        Ok(())
    }

    /// Lookup helper: does this announcement include a capability
    /// for the given primitive?
    pub fn has_capability(&self, primitive: Primitive) -> bool {
        self.capabilities.iter().any(|c| c.primitive == primitive)
    }

    /// Lookup helper: return the skill with the given id, if any.
    pub fn find_skill(&self, skill_id: &str) -> Option<&SkillDeclaration> {
        self.skills.iter().find(|s| s.id == skill_id)
    }
}

// ── Capability ───────────────────────────────────────────────

/// A single base-primitive capability. Adapters publish one entry
/// per primitive they can serve; the Directory joins across providers
/// to answer "who can serve `image.analyze`?"
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// The closed-enum primitive this capability covers.
    pub primitive: Primitive,
}

impl Capability {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }
}

// ── Skill declaration ────────────────────────────────────────

/// A named, pre-configured invocation of a primitive. Layered on top
/// of a capability; the adapter bakes some fields internally and
/// exposes the remaining surface via `parameters`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillDeclaration {
    /// Adapter-owned identifier, unique within the provider's
    /// announcement. Appears as the final segment in the URL:
    /// `/v1/{modality}/{leaf}/{skill_id}`.
    pub id: String,

    /// The base primitive this skill is a specialization of. Must be
    /// declared in the announcement's `capabilities`.
    pub primitive: Primitive,

    /// Human-facing metadata for catalog rendering and browsing.
    pub display: SkillDisplay,

    /// Caller-visible parameter contract. Catalog forms render from
    /// this list; introspection responses enumerate it with
    /// `effective_default` layered over preferences.
    #[serde(default)]
    pub parameters: Vec<SkillParameter>,
}

// ── Display metadata ─────────────────────────────────────────

/// Human-facing labels and assets attached to a skill. Used by the
/// catalog and the `GET` introspection endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDisplay {
    /// Short human name, e.g., "Image Understanding".
    pub name: String,

    /// Optional longer description. Shown on the skill detail page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Free-form tags for browsing and filtering. Lowercased by the
    /// subscriber on ingest.
    #[serde(default)]
    pub tags: Vec<String>,

    /// URL to a preview image the dashboard can fetch separately. The
    /// adapter is responsible for serving the image from some route;
    /// the Directory does not store bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_image: Option<String>,
}

impl SkillDisplay {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            tags: Vec::new(),
            preview_image: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags.into_iter().map(|t| t.to_lowercase()).collect();
        self
    }

    pub fn with_preview_image(mut self, url: impl Into<String>) -> Self {
        self.preview_image = Some(url.into());
        self
    }
}

// ── Parameter contract ───────────────────────────────────────

/// A single typed parameter a skill exposes to callers. Mirrors the
/// shape of a primitive's honored field entry so the catalog can
/// render both primitives and skills through one code path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillParameter {
    /// Dotted field path the caller uses in the request body, e.g.,
    /// `"image.source"` or `"selectors.model"`. The field path must
    /// belong to the vocabulary of the skill's primitive; the
    /// `DirectorySubscriber` does not validate this (the vocabulary
    /// registry is consulted at dispatch time), but adapters are
    /// expected to use canonical keys.
    pub field: String,

    /// Whether the caller must provide this field explicitly. If
    /// `required = false` and no default is declared, the field is
    /// treated as genuinely optional.
    #[serde(default)]
    pub required: bool,

    /// Short human-readable description shown in the catalog / form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The default value the adapter will apply if the caller omits
    /// this field and no preference override is active. May be a
    /// `recommended:*` moniker string, a concrete value, or `null`
    /// for "no default, but optional".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    /// When present, advertises that this field participates in
    /// `recommended:*` auto-resolution. The catalog renders
    /// `auto.description` as helper text; the introspection endpoint
    /// uses this to decide whether the field is pinnable and whether
    /// `default_source` should read from the preferences layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<AutoDescriptor>,

    /// Whether the caller is allowed to send a concrete value here
    /// (pinning), or whether the adapter reserves this field for its
    /// own resolution. Most selector fields are pinnable; inputs like
    /// `image.source` are not (the caller is *required* to provide
    /// them, not optionally pinning).
    #[serde(default)]
    pub pinnable: bool,
}

/// Describes the auto-resolution behavior for a field that participates
/// in `recommended:*` defaulting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoDescriptor {
    /// The default selector string applied when the caller omits the
    /// field, e.g., `"recommended:chat"` or `"recommended:vision"`.
    pub default: String,

    /// Human-readable explanation rendered in the dashboard form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Validation errors ────────────────────────────────────────

/// Errors produced by [`CapabilityAnnouncement::validate`] and by the
/// [`DirectorySubscriber`] when an announcement cannot be accepted.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AnnouncementError {
    #[error(
        "provider `{provider}` declared skill `{skill_id}` for primitive `{primitive}` \
         but did not declare `{primitive}` as a capability"
    )]
    SkillWithoutCapability {
        provider: ProviderName,
        skill_id: String,
        primitive: Primitive,
    },

    #[error("provider `{provider}` declared duplicate skill id `{skill_id}`")]
    DuplicateSkillId {
        provider: ProviderName,
        skill_id: String,
    },

    #[error(
        "announcement topic `{topic}` does not match payload provider `{payload_provider}`"
    )]
    TopicProviderMismatch {
        topic: String,
        payload_provider: ProviderName,
    },
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ProviderName {
        ProviderName::new("ollama")
    }

    fn chat_capability() -> Capability {
        Capability::new(Primitive::TextChat)
    }

    fn vision_capability() -> Capability {
        Capability::new(Primitive::ImageAnalyze)
    }

    fn image_understanding_skill() -> SkillDeclaration {
        SkillDeclaration {
            id: "image-understanding".into(),
            primitive: Primitive::ImageAnalyze,
            display: SkillDisplay::new("Image Understanding")
                .with_description("Extract JSON from an image.")
                .with_tags(vec!["vision".into(), "json".into()]),
            parameters: vec![
                SkillParameter {
                    field: "image.source".into(),
                    required: true,
                    description: Some("The image to analyze.".into()),
                    default: None,
                    auto: None,
                    pinnable: false,
                },
                SkillParameter {
                    field: "selectors.model".into(),
                    required: false,
                    description: Some("Vision-capable model.".into()),
                    default: Some(serde_json::json!("recommended:vision")),
                    auto: Some(AutoDescriptor {
                        default: "recommended:vision".into(),
                        description: Some("Ollama picks a vision-capable model.".into()),
                    }),
                    pinnable: true,
                },
            ],
        }
    }

    #[test]
    fn empty_announcement_validates() {
        let ann = CapabilityAnnouncement::empty(provider());
        assert!(ann.validate().is_ok());
    }

    #[test]
    fn announcement_with_capabilities_only_validates() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![chat_capability(), vision_capability()],
            skills: vec![],
        };
        assert!(ann.validate().is_ok());
    }

    #[test]
    fn skill_with_declared_capability_validates() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![vision_capability()],
            skills: vec![image_understanding_skill()],
        };
        assert!(ann.validate().is_ok());
    }

    #[test]
    fn skill_without_capability_rejected() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![chat_capability()], // no image.analyze
            skills: vec![image_understanding_skill()],
        };
        let err = ann.validate().unwrap_err();
        assert!(matches!(
            err,
            AnnouncementError::SkillWithoutCapability {
                primitive: Primitive::ImageAnalyze,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_skill_id_rejected() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![vision_capability()],
            skills: vec![image_understanding_skill(), image_understanding_skill()],
        };
        let err = ann.validate().unwrap_err();
        assert!(matches!(
            err,
            AnnouncementError::DuplicateSkillId { ref skill_id, .. }
                if skill_id == "image-understanding"
        ));
    }

    #[test]
    fn has_capability_lookup() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![chat_capability(), vision_capability()],
            skills: vec![],
        };
        assert!(ann.has_capability(Primitive::TextChat));
        assert!(ann.has_capability(Primitive::ImageAnalyze));
        assert!(!ann.has_capability(Primitive::TextEmbed));
    }

    #[test]
    fn find_skill_lookup() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![vision_capability()],
            skills: vec![image_understanding_skill()],
        };
        let found = ann.find_skill("image-understanding").unwrap();
        assert_eq!(found.primitive, Primitive::ImageAnalyze);
        assert!(ann.find_skill("nonexistent").is_none());
    }

    #[test]
    fn display_tags_lowercased_on_construction() {
        let display = SkillDisplay::new("Test")
            .with_tags(vec!["Vision".into(), "JSON".into(), "Tagging".into()]);
        assert_eq!(display.tags, vec!["vision", "json", "tagging"]);
    }

    #[test]
    fn announcement_roundtrips_through_json() {
        let ann = CapabilityAnnouncement {
            provider: provider(),
            enabled: true,
            capabilities: vec![chat_capability(), vision_capability()],
            skills: vec![image_understanding_skill()],
        };
        let json = serde_json::to_string(&ann).unwrap();
        let parsed: CapabilityAnnouncement = serde_json::from_str(&json).unwrap();
        assert_eq!(ann, parsed);
    }

    #[test]
    fn unknown_primitive_fails_deserialization() {
        let json = r#"{
            "provider": "ollama",
            "enabled": true,
            "capabilities": [{"primitive": "nonexistent.primitive"}],
            "skills": []
        }"#;
        let result: Result<CapabilityAnnouncement, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn auto_descriptor_default_required_field() {
        let json = r#"{
            "field": "selectors.model",
            "auto": {"default": "recommended:chat"}
        }"#;
        let param: SkillParameter = serde_json::from_str(json).unwrap();
        let auto = param.auto.unwrap();
        assert_eq!(auto.default, "recommended:chat");
        assert!(auto.description.is_none());
    }

    #[test]
    fn skill_parameter_default_can_be_any_json() {
        let json = r#"{"field": "image.sampling.steps", "default": 28}"#;
        let param: SkillParameter = serde_json::from_str(json).unwrap();
        assert_eq!(param.default.unwrap(), serde_json::json!(28));
    }

    #[test]
    fn topic_provider_mismatch_variant() {
        let err = AnnouncementError::TopicProviderMismatch {
            topic: "directory.provider.ollama.capabilities".into(),
            payload_provider: ProviderName::new("comfyui"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("ollama"));
        assert!(msg.contains("comfyui"));
    }
}
