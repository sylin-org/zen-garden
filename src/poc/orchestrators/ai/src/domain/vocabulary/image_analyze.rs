//! `image.analyze` vocabulary — describe an image or answer a question about it.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::{image, text};
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::ImageAnalyze,
        summary: Primitive::ImageAnalyze.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: image::SOURCE,
                field_type: FieldType::MediaRef,
                description: "Source image to analyze.",
            }],
            optional: vec![
                FieldSpec {
                    path: text::PROMPT_USER,
                    field_type: FieldType::String,
                    description: "Question or instruction for analysis.",
                },
                FieldSpec {
                    path: text::FORMAT_RESPONSE,
                    field_type: FieldType::String,
                    description: "Desired response format: 'text' or 'json'.",
                },
                FieldSpec {
                    path: text::TOKENS_MAX,
                    field_type: FieldType::Integer {
                        min: Some(1),
                        max: Some(16_000),
                    },
                    description: "Maximum length of the response in tokens.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("source"),
                    to: image::SOURCE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("prompt"),
                    to: text::PROMPT_USER,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("max_tokens"),
                    to: text::TOKENS_MAX,
                    condition: AliasCondition::Always,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![FieldSpec {
                path: text::RESPONSE,
                field_type: FieldType::String,
                description: "Textual analysis of the image.",
            }],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        example_minimal: json!({
            "image": {"source": {"media_id": "01JA7X-example"}},
            "prompt": "What's in this image?"
        }),
        example_full: json!({
            "image": {"source": {"media_id": "01JA7X-example"}},
            "text": {
                "prompt": {"user": "Describe the scene in detail"},
                "format": {"response": "text"},
                "tokens": {"max": 500}
            }
        }),
    }
}
