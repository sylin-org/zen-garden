//! Skill data model — disk schema v3 + in-memory shapes (ORCH-0029).
//!
//! Two layers live here:
//!
//! 1. **On-disk schema** (`RawSkillV3`, `RawBindingV3`, …) — plain
//!    serde structs that the loader deserializes from `skill.json`.
//!    The loader also understands legacy v1/v2 files via the
//!    translation table in `loader.rs`.
//! 2. **In-memory typed model** (`SkillDefinition`, `Binding`,
//!    `BindingTarget`, `ModelSelector`, `Variant`, …) — what the
//!    ComfyUI adapter consumes during `onboard`, and what the
//!    registry publishes to the Directory after splitting into the
//!    public Registration half and the adapter-private execution
//!    half.
//!
//! See ORCH-0029 §Disk schema (v3) for the wire contract.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::domain::field_path::FieldPath;
use crate::domain::media::MediaDelivery;
use crate::domain::moniker::Moniker;
use crate::domain::primitive::Primitive;

// ── Constraint types (canonical home; ORCH-0030 R2 M3) ──────
//
// These types describe how a skill narrows a vocabulary field.
// They live in this module — not in `domain/provider.rs` — because
// they are skill-schema types, not provider-trait types. The disk
// schema (v3) depends on them; the canonical lean Provider trait
// does not.

/// Narrows a vocabulary `FieldType` for a specific skill (ORCH-0029).
///
/// Skills declare these on their bindings. The dashboard reads the
/// vocabulary's base `FieldType` and applies this overlay to pick
/// the right widget (slider, dropdown, autofill). The contextualizer
/// validates incoming values against the constraint after passing
/// the vocabulary's broader type check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldConstraint {
    /// Restrict to a finite set of values. Compatible with
    /// vocabulary types `String`, `Integer`, `Number`.
    Options { options: Vec<ParamOption> },
    /// Tighten a numeric range. Compatible with `Integer`, `Number`.
    /// `min`/`max` MUST be inside the vocabulary's declared range.
    Range {
        min: f64,
        max: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    /// Auto-generated value (e.g., random seed). The dispatcher
    /// fills the field if the caller omits it. The dashboard
    /// renders a "regenerate" button.
    Auto {
        #[serde(rename = "auto")]
        kind_inner: AutoKind,
    },
}

impl Eq for FieldConstraint {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamOption {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Eq for ParamOption {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoKind {
    /// Random unsigned 64-bit integer per request (seeds).
    RandomInt,
}

// ── In-memory typed model ─────────────────────────────────────

/// A loaded skill definition — the output of [`loader::load_skills`].
///
/// Carries every piece of information the adapter needs to build
/// both the public Registration (honored fields, media inputs, skill
/// metadata) and the adapter-private `LoadedSkill` (workflow files,
/// binding plan, model resolver).
#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub moniker: Moniker,
    pub display_name: String,
    pub primitive: Primitive,
    pub description: String,
    pub vram_mb: u64,
    pub default_workflow: String,
    pub workflows: HashMap<String, serde_json::Value>,
    pub bindings: Vec<Binding>,
    pub model_selector: Option<ModelSelector>,
    pub variants: Option<Vec<Variant>>,
    pub required_models: Vec<ModelRef>,
    pub source: Option<ImportSource>,
    pub preview_url: Option<String>,
    pub output_node: Option<String>,
}

/// A typed binding from a vocabulary field to a workflow address.
#[derive(Debug, Clone, Serialize)]
pub struct Binding {
    /// Canonical vocabulary field path, or `x_*` for provider-specific
    /// extensions that the vocabulary does not catalogue.
    pub field: FieldPath,
    /// Where the user's value lands in the workflow.
    pub target: BindingTarget,
    /// Skill-specific default (pre-fills the dashboard form and feeds
    /// the dispatcher when the caller omits the field).
    pub default: Option<serde_json::Value>,
    /// Skill-specific narrowing of the vocabulary's `FieldType`.
    pub narrow: Option<FieldConstraint>,
    /// Dashboard label override (falls back to the vocabulary's
    /// description).
    pub label: Option<String>,
    /// Whether the skill marks this field as required.
    pub required: bool,
    /// Media delivery mode (set only on bindings whose field is a
    /// `MediaRef`-typed vocabulary entry — image/audio source slots).
    pub delivery: Option<MediaDelivery>,
    /// Media content-type allowlist (only meaningful for media bindings).
    pub accepted_types: Vec<String>,
    /// Paint-overlay hint for inpaint masks: when set, the dashboard
    /// renders this slot as an overlay on the named role.
    pub overlay: Option<String>,
    /// Self-described type for `x_*` fields. `None` for canonical
    /// fields (the vocabulary provides the type).
    pub self_described_type: Option<SelfDescribedType>,
}

/// Where a binding's value lands in the workflow template.
///
/// Serde uses an "internally-tagged adjacent" pattern: the variant
/// discriminator is `target_kind` and the payload is `target_value`
/// (string for `Placeholder`, `{node, input}` object for
/// `NodeInput`). Serde doesn't support internally-tagged newtype
/// variants carrying primitive types, so we encode manually.
#[derive(Debug, Clone)]
pub enum BindingTarget {
    /// String substitution throughout the workflow tree (replaces
    /// every occurrence of the placeholder string with the value).
    Placeholder(String),
    /// Direct addressing: `workflow[node]["inputs"][input] = value`.
    NodeInput { node: String, input: String },
}

impl Serialize for BindingTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            BindingTarget::Placeholder(s) => {
                let mut st = serializer.serialize_struct("BindingTarget", 2)?;
                st.serialize_field("kind", "placeholder")?;
                st.serialize_field("placeholder", s)?;
                st.end()
            }
            BindingTarget::NodeInput { node, input } => {
                let mut st = serializer.serialize_struct("BindingTarget", 3)?;
                st.serialize_field("kind", "node_input")?;
                st.serialize_field("node", node)?;
                st.serialize_field("input", input)?;
                st.end()
            }
        }
    }
}

/// Multi-workflow selector. When a skill has multiple workflow files
/// and lets the caller pick which one to run, the loader hoists the
/// selector into this typed field and the adapter reads
/// `selectors.variant` at dispatch time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Model selector — narrows `selectors.model` for this skill.
///
/// The `placeholder` is substituted in the workflow with the chosen
/// model filename. The dashboard surfaces `options` as a dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelector {
    pub placeholder: String,
    pub default: String,
    pub options: Vec<ParamOption>,
}

/// A model dependency declared by a skill. The provisioning cache
/// reads these entries to plan downloads and push cached files to
/// remote ComfyUI instances via the Moss volume API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    pub filename: String,
    pub model_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// Self-described type for `x_*` fields that aren't in the canonical
/// vocabulary. The dashboard renders these the same way as canonical
/// fields whose vocabulary `FieldType` resolves to the same shape.
#[derive(Debug, Clone, Serialize)]
pub struct SelfDescribedType {
    #[serde(flatten)]
    pub kind: SelfDescribedKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelfDescribedKind {
    String,
    Integer {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Boolean,
}

// ── On-disk schema — v3 ───────────────────────────────────────

/// v3 on-disk shape. The loader deserializes this directly from
/// `skill.json` files with `version: 3`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSkillV3 {
    pub version: u64,
    #[serde(default)]
    pub draft: bool,
    pub name: String,
    pub display_name: String,
    /// Dotted primitive identifier — `"image.generate"`, `"image.upscale"`, …
    pub primitive: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub vram_mb: u64,
    pub default_workflow: String,
    pub bindings: Vec<RawBindingV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_selector: Option<RawModelSelectorV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variants: Option<Vec<Variant>>,
    #[serde(default)]
    pub required_models: Vec<ModelRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ImportSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_node: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawBindingV3 {
    pub field: String,
    // Exactly one of `placeholder` or (`node` + `input`) is expected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrow: Option<FieldConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,

    // Media binding extensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<MediaDelivery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_types: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawModelSelectorV3 {
    pub placeholder: String,
    pub default: String,
    pub options: Vec<ParamOption>,
}

// ── On-disk schema — legacy v1/v2 ─────────────────────────────
//
// The loader translates these to v3 on read via the table in
// `loader::legacy::translate`. The raw files are never modified.

#[derive(Debug, Clone, Deserialize)]
pub struct RawSkillLegacy {
    pub version: u64,
    #[serde(default)]
    pub draft: bool,
    pub name: String,
    pub display_name: String,
    pub capability: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub provider_kind: String,
    #[serde(default)]
    pub vram_mb: u64,
    pub default_workflow: String,
    #[serde(default)]
    pub content_slots: Vec<RawContentSlotLegacy>,
    pub mappings: Vec<serde_json::Value>,
    #[serde(default)]
    pub required_models: Vec<ModelRef>,
    #[serde(default)]
    pub source: Option<ImportSource>,
    #[serde(default)]
    pub preview_url: Option<String>,
    #[serde(default)]
    pub diagram: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawContentSlotLegacy {
    pub role: String,
    pub content_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub overlay: Option<String>,
    #[serde(default)]
    pub default: Option<String>,
}

/// The legacy `mappings[*]` entries have a `type` discriminator
/// (`"content"` or `"param"`). We parse them into this enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RawMappingLegacy {
    Content {
        role: String,
        content_type: String,
        placeholder: String,
    },
    Param {
        field: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        node: Option<String>,
        #[serde(default)]
        input: Option<String>,
        #[serde(default)]
        placeholder: Option<String>,
        param_type: String,
        // Fields whose presence depends on `param_type`:
        #[serde(default)]
        options: Option<Vec<ParamOption>>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        step: Option<f64>,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        default: Option<serde_json::Value>,
    },
}
