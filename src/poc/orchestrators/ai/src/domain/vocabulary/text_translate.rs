//! `text.translate` vocabulary.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::text;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::TextTranslate,
        summary: Primitive::TextTranslate.summary(),
        input: IoSchema {
            required: vec![
                FieldSpec {
                    path: text::BODY,
                    field_type: FieldType::String,
                    description: "The text to translate.",
                },
                FieldSpec {
                    path: text::LANGUAGE_TARGET,
                    field_type: FieldType::String,
                    description: "Target language code (IETF BCP 47 or ISO 639).",
                },
            ],
            optional: vec![FieldSpec {
                path: text::LANGUAGE_SOURCE,
                field_type: FieldType::String,
                description: "Source language; when absent, providers auto-detect.",
            }],
            aliases: vec![
                Alias {
                    from: FieldPath::new("body"),
                    to: text::BODY,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("source"),
                    to: text::LANGUAGE_SOURCE,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("target"),
                    to: text::LANGUAGE_TARGET,
                    condition: AliasCondition::WhenString,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: text::TRANSLATED,
                    field_type: FieldType::String,
                    description: "The translated text.",
                },
                FieldSpec {
                    path: text::DETECTED_LANGUAGE,
                    field_type: FieldType::String,
                    description: "Source language detected when not supplied.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        example_minimal: json!({"body": "Hello", "target": "ja"}),
        example_full: json!({
            "text": {
                "body": "Monsieur!",
                "language": {"source": "fr", "target": "en-US"}
            }
        }),
    }
}
