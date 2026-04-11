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
use crate::domain::media::MediaDelivery;
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    /// The closed-enum primitive this capability covers.
    pub primitive: Primitive,

    /// Provider priority for this primitive (ORCH-0037). Higher wins
    /// when selecting the default provider for composed introspect.
    /// Local providers default to 0; cloud/external to -10.
    #[serde(default)]
    pub priority: i32,

    /// Media inputs this capability honors. One entry per field
    /// that accepts a media reference (e.g. `image.source` for
    /// `image.analyze`). The [`MediaResolver`] reads this list
    /// instead of the legacy `Registration.media_inputs` and applies
    /// the declared [`MediaDelivery`] mode to each reference.
    ///
    /// Empty for primitives that never accept media (text.chat,
    /// text.embed, text.rerank, text.translate). Non-empty for
    /// `image.analyze`, `image.edit`, `image.upscale`,
    /// `audio.transcribe`, etc.
    ///
    /// See [`CapabilityMediaInput`] for the wire shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_inputs: Vec<CapabilityMediaInput>,

    /// Form-schema parameters for this base primitive. Identical
    /// shape to `SkillDeclaration.parameters`. The catalog renders
    /// these when the user selects the base primitive (e.g.,
    /// `GET /v1/catalog/text.chat` returns the full field list).
    ///
    /// Added in ORCH-0030 R2 commit 6.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SkillParameter>,

    /// Named example scenarios that fill the form (ORCH-0035).
    /// Each example carries a label (card text) and a payload
    /// using canonical vocabulary field paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,

}

impl Capability {
    /// Construct a bare capability with no media inputs.
    pub fn new(primitive: Primitive) -> Self {
        Self {
            primitive,
            priority: 0,
            media_inputs: Vec::new(),
            parameters: Vec::new(),
            examples: Vec::new(),
        }
    }

    /// Add a media input declaration to this capability.
    pub fn with_media_input(mut self, input: CapabilityMediaInput) -> Self {
        self.media_inputs.push(input);
        self
    }

    /// Add a parameter to this capability's form schema.
    pub fn with_parameter(mut self, param: SkillParameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// The dotted registration path for this base primitive.
    /// E.g., `Primitive::TextChat` → `"text.chat"`.
    pub fn registration_path(&self) -> &str {
        self.primitive.dotted()
    }
}

/// Serializable wire shape of a media input declaration attached to
/// a capability. Lives in the capability announcement schema so it
/// can travel on the bus and round-trip through JSON. Replaces the
/// legacy `MediaInputSpec` (deleted with the rest of the legacy
/// `Registration` machinery in ORCH-0030 R2 M3).
///
/// The caller sends a media reference (`{media_id: "..."}`) at
/// `field`; the `MediaResolver` resolves it according to `delivery`
/// and validates the bytes' content type against `accepted_types`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityMediaInput {
    /// Dotted canonical field path where the caller places the
    /// media reference (e.g. `"image.source"`).
    pub field: String,

    /// How the media resolver should deliver the bytes to the
    /// adapter's `onboard` method.
    pub delivery: MediaDelivery,

    /// Accepted MIME types. Empty means any content type is
    /// acceptable (the adapter handles validation itself).
    #[serde(default)]
    pub accepted_types: Vec<String>,

    /// Optional overlay hint for inpaint-style skills that paint on
    /// top of another image. Mirrors the legacy
    /// `MediaInputSpec.overlay`. Rare — only skill workflows use it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
}

impl CapabilityMediaInput {
    pub fn base64(field: impl Into<String>, accepted_types: Vec<String>) -> Self {
        Self {
            field: field.into(),
            delivery: MediaDelivery::Base64,
            accepted_types,
            overlay: None,
        }
    }

    pub fn by_id(field: impl Into<String>, accepted_types: Vec<String>) -> Self {
        Self {
            field: field.into(),
            delivery: MediaDelivery::ById,
            accepted_types,
            overlay: None,
        }
    }

    pub fn transfer(field: impl Into<String>, accepted_types: Vec<String>) -> Self {
        Self {
            field: field.into(),
            delivery: MediaDelivery::Transfer,
            accepted_types,
            overlay: None,
        }
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

    /// Named example scenarios (ORCH-0035).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,
}

impl SkillDeclaration {
    /// The dotted registration path for this skill, suitable for URL
    /// routing and catalog indexing. For example, a skill with
    /// `id = "sample-tron"` and `primitive = ImageGenerate` produces
    /// `"image.generate.sample-tron"`.
    pub fn registration_path(&self) -> String {
        format!("{}.{}", self.primitive.dotted(), self.id)
    }
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

/// A single typed parameter a skill or primitive exposes to callers.
/// Carries enough information to render a complete input form widget
/// and validate before submit — no other data source needed.
///
/// The form-schema fields (`label`, `field_type`, `widget`, `min`,
/// `max`, `step`, `options`, `placeholder`) were added in ORCH-0030
/// R2 (commit 6). All are optional/defaulted so existing
/// announcements and tests continue to deserialize.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
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

    // ── Form-schema fields (ORCH-0030 R2 commit 6) ──────────

    /// Human-readable label shown on the form widget. Falls back to
    /// `description` or the vocabulary entry's label if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Data type for validation. `None` → inferred from vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<ParameterType>,

    /// Rendering hint for the form widget. `None` → the client picks
    /// a default widget based on `field_type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<ParameterWidget>,

    /// Minimum value for `Slider` / `Number` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,

    /// Maximum value for `Slider` / `Number` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,

    /// Step granularity for `Slider` / `Number` widgets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,

    /// Closed set of valid values for `Select` widgets. Each value
    /// is a JSON value (string, number, bool) matching `field_type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<serde_json::Value>>,

    /// Ghost text for empty text inputs and textareas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

/// Data type of a parameter value, for validation and form rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    String,
    Integer,
    Number,
    Boolean,
    /// Alternating user/assistant turns — rendered as a conversation
    /// thread widget.
    Dialogue,
}

/// Rendering hint for a form widget. Clients may ignore this and
/// render a default widget based on `ParameterType`, but dashboards
/// that honor widget hints produce a much better experience.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterWidget {
    /// Multi-line text input (prompts, system prompts).
    Textarea,
    /// Range slider with `min`, `max`, `step`.
    Slider,
    /// Numeric spinner input.
    Number,
    /// Dropdown with `options`.
    Select,
    /// On/off toggle (boolean fields).
    Toggle,
    /// Field exists in the schema but is not shown to the user.
    /// Used for fields the adapter fills internally (e.g., model
    /// selector on ComfyUI skills where the model is baked into
    /// the workflow).
    Hidden,
    /// File upload widget (media inputs).
    File,
    /// Conversation thread — alternating user/assistant messages.
    /// The widget accumulates turns and includes the full history
    /// in every dispatch as the field value.
    Dialogue,
    /// Integer input paired with a "re-roll" dice button that
    /// generates a uniformly random value in `[min, max]`. When
    /// `min`/`max` are omitted, the full `u32` range is used. Used
    /// for generative seeds (image diffusion, text sampling) where
    /// the user wants reproducibility by default but one-click
    /// randomization on demand.
    #[serde(rename = "random_seed")]
    RandomSeed,
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

// ── Examples (ORCH-0035) ─────────────────────────────────────

/// A named scenario that pre-fills the form. Adapters provide
/// examples tailored to their domain — a Rilke poem for translate,
/// a creative prompt for image generation, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Example {
    /// Short label shown on the example card, action-oriented.
    /// E.g. "German poem to English", "Anime portrait".
    pub label: String,

    /// Optional one-liner expanding on what the example does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The payload that fills the form. Uses canonical vocabulary
    /// field paths as keys — identical structure to a dispatch
    /// payload. The dashboard reads each key, matches it to a
    /// catalog field, and populates the corresponding widget.
    pub payload: serde_json::Value,
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

