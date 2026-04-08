//! `image.edit` vocabulary — inpainting / outpainting with an optional mask.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::image;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::ImageEdit,
        summary: Primitive::ImageEdit.summary(),
        input: IoSchema {
            required: vec![
                FieldSpec {
                    path: image::SOURCE,
                    field_type: FieldType::MediaRef,
                    description: "Source image to edit.",
                },
                FieldSpec {
                    path: image::PROMPT_POSITIVE,
                    field_type: FieldType::String,
                    description: "Prompt describing the desired edit.",
                },
            ],
            optional: vec![
                FieldSpec {
                    path: image::MASK,
                    field_type: FieldType::MediaRef,
                    description: "Optional mask indicating the region to edit.",
                },
                FieldSpec {
                    path: image::PROMPT_NEGATIVE,
                    field_type: FieldType::String,
                    description: "Negative prompt.",
                },
                FieldSpec {
                    path: image::SAMPLING_STEPS,
                    field_type: FieldType::Integer {
                        min: Some(1),
                        max: Some(150),
                    },
                    description: "Number of sampling steps.",
                },
                FieldSpec {
                    path: image::SAMPLING_SEED,
                    field_type: FieldType::Integer {
                        min: None,
                        max: None,
                    },
                    description: "Seed for reproducible edits.",
                },
                FieldSpec {
                    path: image::SAMPLING_GUIDANCE,
                    field_type: FieldType::Number {
                        min: Some(0.0),
                        max: Some(30.0),
                    },
                    description: "CFG scale.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("prompt"),
                    to: image::PROMPT_POSITIVE,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("source"),
                    to: image::SOURCE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("mask"),
                    to: image::MASK,
                    condition: AliasCondition::Always,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing, SharedNamespace::Job],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: image::MEDIA_ID,
                    field_type: FieldType::String,
                    description: "Media ID for the edited image.",
                },
                FieldSpec {
                    path: image::WIDTH,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Output image width.",
                },
                FieldSpec {
                    path: image::HEIGHT,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Output image height.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing],
        },
        example_minimal: json!({
            "image": {
                "source": {"media_id": "01JA7X-example"},
                "prompt": {"positive": "a small cat"}
            }
        }),
        example_full: json!({
            "image": {
                "source": {"media_id": "01JA7X-example"},
                "mask": {"media_id": "01JA7Y-example"},
                "prompt": {"positive": "a small cat", "negative": "dog"},
                "sampling": {"steps": 20, "guidance": 7.0}
            }
        }),
    }
}
