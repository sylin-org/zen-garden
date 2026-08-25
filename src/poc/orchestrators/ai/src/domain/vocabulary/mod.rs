//! Per-primitive vocabulary registry — the public contract the
//! orchestrator serves and the target providers narrow.
//!
//! Every primitive has a vocabulary that declares:
//!
//! - **Input fields** — required and optional field specs.
//! - **Aliases** — caller-friendly shortcuts (e.g. `"prompt"` →
//!   `"text.prompt.user"`), including the special
//!   `MessagesDecomposer` transformer used to accept OpenAI-shape
//!   `messages: [...]` arrays.
//! - **Output fields** — optional keys providers may return.
//! - **Shared namespaces** — `usage.*`, `timing.*`, `meta.*`, `job.*`,
//!   `stream.*` opted into by the primitive.
//! - **Examples** — a minimal and a full example used by the
//!   `GET /v1/do` action index.
//! - **Summary** — human-readable one-liner.
//!
//! The registry is built once at startup into a [`VocabularyRegistry`]
//! held inside the [`crate::app_state::AppState`]. Lookups are O(1) by
//! primitive.

pub mod audio_generate;
pub mod audio_transcribe;
pub mod image_analyze;
pub mod image_edit;
pub mod image_generate;
pub mod image_upscale;
pub mod text_chat;
pub mod text_embed;
pub mod text_rerank;
pub mod text_translate;

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::domain::field_path::FieldPath;
use crate::domain::primitive::Primitive;

// ── Schema types ──────────────────────────────────────────────

/// A vocabulary bundles input + output schemas for a single primitive.
#[derive(Debug, Clone)]
pub struct Vocabulary {
    pub primitive: Primitive,
    pub summary: &'static str,
    pub input: IoSchema,
    pub output: IoSchema,
    /// Minimal request body (aliased form) used in `/v1/do` examples.
    pub example_minimal: Value,
    /// Full request body (canonical form) used in `/v1/do` examples.
    pub example_full: Value,
}

#[derive(Debug, Clone, Default)]
pub struct IoSchema {
    pub required: Vec<FieldSpec>,
    pub optional: Vec<FieldSpec>,
    pub aliases: Vec<Alias>,
    pub shared_namespaces: Vec<SharedNamespace>,
}

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub path: FieldPath,
    pub field_type: FieldType,
    pub description: &'static str,
}

/// Field type constraints enforced by the contextualizer's
/// `validate_input` pass.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldType {
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
    /// A homogeneous array of values. The element type is left open in
    /// v1 — individual providers are responsible for validating element
    /// shape when it matters (e.g., `text.stop.sequences` must be an
    /// array of strings; `text.documents` is an array of strings;
    /// `text.tools.definitions` is an array of objects).
    Array,
    Object,
    /// A `{media_id: "..."}` reference. Accepted as a bare string
    /// (`media_id`) or an object with a `media_id` field.
    MediaRef,
    /// A dialogue: alternating user/assistant turns. Rendered as a
    /// conversation thread widget. Replaces the prior `MessageHistory`
    /// name for clearer semantics.
    Dialogue,
}

/// Input-field alias rewrites.
///
/// `from` is a caller-supplied path (typically at the top level —
/// `prompt`, `temperature`, `max_tokens`). The contextualizer rewrites
/// it to the canonical `to` path unless the canonical path is already
/// present with a different value (collision → `validation_failed`).
#[derive(Debug, Clone)]
pub struct Alias {
    pub from: FieldPath,
    pub to: FieldPath,
    pub condition: AliasCondition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasCondition {
    /// Fire whenever the source path is present, regardless of value type.
    Always,
    WhenString,
    WhenObject,
    WhenArray,
    /// Special-case decomposer: expands an OpenAI-shape
    /// `messages: [...]` array into `text.prompt.user`,
    /// `text.prompt.system`, and `text.prompt.history`. See
    /// [`crate::services::contextualizer`] for the implementation.
    MessagesDecomposer,
}

/// Cross-cutting namespaces a primitive opts into. When listed, the
/// input validator accepts keys in that namespace as known, and the
/// output validator treats them as documented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SharedNamespace {
    Usage,
    Timing,
    Meta,
    Job,
    Stream,
}

impl SharedNamespace {
    pub const fn as_str(self) -> &'static str {
        match self {
            SharedNamespace::Usage => "usage",
            SharedNamespace::Timing => "timing",
            SharedNamespace::Meta => "meta",
            SharedNamespace::Job => "job",
            SharedNamespace::Stream => "stream",
        }
    }
}

// ── Registry ──────────────────────────────────────────────────

/// O(1) primitive → [`Vocabulary`] lookup.
#[derive(Debug, Clone)]
pub struct VocabularyRegistry {
    vocabularies: Arc<HashMap<Primitive, Vocabulary>>,
}

impl VocabularyRegistry {
    /// Build the registry with every primitive's vocabulary. Called
    /// once at startup.
    pub fn build() -> Self {
        let mut map = HashMap::new();
        map.insert(Primitive::TextChat, text_chat::vocabulary());
        map.insert(Primitive::TextTranslate, text_translate::vocabulary());
        map.insert(Primitive::TextEmbed, text_embed::vocabulary());
        map.insert(Primitive::TextRerank, text_rerank::vocabulary());
        map.insert(Primitive::ImageGenerate, image_generate::vocabulary());
        map.insert(Primitive::ImageEdit, image_edit::vocabulary());
        map.insert(Primitive::ImageUpscale, image_upscale::vocabulary());
        map.insert(Primitive::ImageAnalyze, image_analyze::vocabulary());
        map.insert(Primitive::AudioGenerate, audio_generate::vocabulary());
        map.insert(Primitive::AudioTranscribe, audio_transcribe::vocabulary());
        debug_assert_eq!(map.len(), Primitive::ALL.len());
        Self {
            vocabularies: Arc::new(map),
        }
    }

    pub fn get(&self, primitive: Primitive) -> &Vocabulary {
        self.vocabularies
            .get(&primitive)
            .expect("every primitive has a vocabulary (enforced by build())")
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Primitive, &Vocabulary)> {
        self.vocabularies.iter()
    }

    pub fn len(&self) -> usize {
        self.vocabularies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vocabularies.is_empty()
    }
}

impl Default for VocabularyRegistry {
    fn default() -> Self {
        Self::build()
    }
}

// ── Serialization helpers for the catalog builder ─────────────

/// Serialized form of a [`Vocabulary`] for the `/v1/catalog` endpoint.
#[derive(Debug, Serialize)]
pub struct VocabularyView<'a> {
    pub primitive: &'static str,
    pub summary: &'static str,
    pub input: IoSchemaView<'a>,
    pub output: IoSchemaView<'a>,
    pub examples: ExamplesView<'a>,
}

#[derive(Debug, Serialize)]
pub struct IoSchemaView<'a> {
    pub required: Vec<FieldSpecView<'a>>,
    pub optional: Vec<FieldSpecView<'a>>,
    pub aliases: Vec<AliasView<'a>>,
    pub shared: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct FieldSpecView<'a> {
    pub path: &'a str,
    #[serde(rename = "type")]
    pub field_type: &'static str,
    pub description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct AliasView<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub when: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ExamplesView<'a> {
    pub minimal: &'a Value,
    pub full: &'a Value,
}

impl Vocabulary {
    /// Render a serializable view of this vocabulary for the catalog.
    pub fn view(&self) -> VocabularyView<'_> {
        VocabularyView {
            primitive: self.primitive.dotted(),
            summary: self.summary,
            input: schema_view(&self.input),
            output: schema_view(&self.output),
            examples: ExamplesView {
                minimal: &self.example_minimal,
                full: &self.example_full,
            },
        }
    }
}

fn schema_view(schema: &IoSchema) -> IoSchemaView<'_> {
    IoSchemaView {
        required: schema.required.iter().map(field_view).collect(),
        optional: schema.optional.iter().map(field_view).collect(),
        aliases: schema.aliases.iter().map(alias_view).collect(),
        shared: schema
            .shared_namespaces
            .iter()
            .map(|ns| ns.as_str())
            .collect(),
    }
}

fn field_view(spec: &FieldSpec) -> FieldSpecView<'_> {
    let (type_name, min, max) = match &spec.field_type {
        FieldType::String => ("string", None, None),
        FieldType::Integer { min, max } => (
            "integer",
            min.map(serde_json::Value::from),
            max.map(serde_json::Value::from),
        ),
        FieldType::Number { min, max } => (
            "number",
            min.map(serde_json::Value::from),
            max.map(serde_json::Value::from),
        ),
        FieldType::Boolean => ("boolean", None, None),
        FieldType::Array => ("array", None, None),
        FieldType::Object => ("object", None, None),
        FieldType::MediaRef => ("media_ref", None, None),
        FieldType::Dialogue => ("dialogue", None, None),
    };
    FieldSpecView {
        path: spec.path.as_str(),
        field_type: type_name,
        description: spec.description,
        min,
        max,
    }
}

fn alias_view(alias: &Alias) -> AliasView<'_> {
    let when = match alias.condition {
        AliasCondition::Always => "always",
        AliasCondition::WhenString => "when_string",
        AliasCondition::WhenObject => "when_object",
        AliasCondition::WhenArray => "when_array",
        AliasCondition::MessagesDecomposer => "messages_decomposer",
    };
    AliasView {
        from: alias.from.as_str(),
        to: alias.to.as_str(),
        when,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_every_primitive() {
        let reg = VocabularyRegistry::build();
        assert_eq!(reg.len(), 10);
        for p in Primitive::ALL {
            let vocab = reg.get(*p);
            assert_eq!(vocab.primitive, *p);
        }
    }

    #[test]
    fn every_vocabulary_has_a_summary() {
        let reg = VocabularyRegistry::build();
        for p in Primitive::ALL {
            let vocab = reg.get(*p);
            assert!(!vocab.summary.is_empty(), "{} missing summary", p.dotted());
        }
    }

    #[test]
    fn every_vocabulary_has_examples() {
        let reg = VocabularyRegistry::build();
        for p in Primitive::ALL {
            let vocab = reg.get(*p);
            assert!(
                vocab.example_minimal.is_object(),
                "{} minimal example not an object",
                p.dotted()
            );
            assert!(
                vocab.example_full.is_object(),
                "{} full example not an object",
                p.dotted()
            );
        }
    }

    #[test]
    fn no_field_path_collisions_between_required_and_optional() {
        let reg = VocabularyRegistry::build();
        for (p, vocab) in reg.iter() {
            let mut seen = std::collections::HashSet::new();
            for spec in vocab.input.required.iter().chain(vocab.input.optional.iter()) {
                assert!(
                    seen.insert(spec.path.as_str()),
                    "{}: duplicate input field {}",
                    p.dotted(),
                    spec.path
                );
            }
        }
    }
}
