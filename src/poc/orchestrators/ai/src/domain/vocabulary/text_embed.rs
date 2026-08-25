//! `text.embed` vocabulary.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::text;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::TextEmbed,
        summary: Primitive::TextEmbed.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: text::INPUT,
                field_type: FieldType::Array,
                description: "Input text or array of texts to embed.",
            }],
            optional: vec![FieldSpec {
                path: text::DIMENSIONS,
                field_type: FieldType::Integer {
                    min: Some(1),
                    max: Some(8192),
                },
                description: "Desired embedding dimensionality (provider-dependent).",
            }],
            aliases: vec![
                Alias {
                    from: FieldPath::new("input"),
                    to: text::INPUT,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("dimensions"),
                    to: text::DIMENSIONS,
                    condition: AliasCondition::Always,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![FieldSpec {
                path: text::EMBEDDINGS,
                field_type: FieldType::Array,
                description: "Array of float arrays, one per input.",
            }],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        example_minimal: json!({"input": ["first passage", "second passage"]}),
        example_full: json!({
            "text": {
                "input": ["first passage", "second passage"],
                "dimensions": 1024
            }
        }),
    }
}
