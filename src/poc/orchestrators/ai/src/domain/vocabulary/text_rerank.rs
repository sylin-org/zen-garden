//! `text.rerank` vocabulary.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::text;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::TextRerank,
        summary: Primitive::TextRerank.summary(),
        input: IoSchema {
            required: vec![
                FieldSpec {
                    path: text::QUERY,
                    field_type: FieldType::String,
                    description: "Query to score documents against.",
                },
                FieldSpec {
                    path: text::DOCUMENTS,
                    field_type: FieldType::Array,
                    description: "Array of candidate documents (strings).",
                },
            ],
            optional: vec![
                FieldSpec {
                    path: text::RESULTS_TOP_K,
                    field_type: FieldType::Integer {
                        min: Some(1),
                        max: None,
                    },
                    description: "Return only the K highest-scoring results.",
                },
                FieldSpec {
                    path: text::RESULTS_MIN_SCORE,
                    field_type: FieldType::Number {
                        min: Some(0.0),
                        max: Some(1.0),
                    },
                    description: "Drop results below this score threshold.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("query"),
                    to: text::QUERY,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("documents"),
                    to: text::DOCUMENTS,
                    condition: AliasCondition::WhenArray,
                },
                Alias {
                    from: FieldPath::new("top_k"),
                    to: text::RESULTS_TOP_K,
                    condition: AliasCondition::Always,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![FieldSpec {
                path: text::SEGMENTS,
                field_type: FieldType::Array,
                description: "Array of {index, score, document} entries, sorted by score.",
            }],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        example_minimal: json!({
            "query": "cities in Europe",
            "documents": ["Paris", "Tokyo", "Berlin", "Sydney"]
        }),
        example_full: json!({
            "text": {
                "query": "cities in Europe",
                "documents": ["Paris", "Tokyo", "Berlin", "Sydney"],
                "results": {"top_k": 2, "min_score": 0.3}
            }
        }),
    }
}
