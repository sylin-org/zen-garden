//! `image.generate` vocabulary.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::image;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::ImageGenerate,
        summary: Primitive::ImageGenerate.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: image::PROMPT_POSITIVE,
                field_type: FieldType::String,
                description: "Positive prompt describing the desired image.",
            }],
            optional: vec![
                FieldSpec {
                    path: image::PROMPT_NEGATIVE,
                    field_type: FieldType::String,
                    description: "Negative prompt describing what to avoid.",
                },
                FieldSpec {
                    path: image::DIMENSIONS_WIDTH,
                    field_type: FieldType::Integer {
                        min: Some(64),
                        max: Some(8192),
                    },
                    description: "Output image width in pixels.",
                },
                FieldSpec {
                    path: image::DIMENSIONS_HEIGHT,
                    field_type: FieldType::Integer {
                        min: Some(64),
                        max: Some(8192),
                    },
                    description: "Output image height in pixels.",
                },
                FieldSpec {
                    path: image::DIMENSIONS_ASPECT,
                    field_type: FieldType::String,
                    description: "Aspect ratio hint (e.g., '16:9', '1:1').",
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
                    description: "Seed for deterministic generation.",
                },
                FieldSpec {
                    path: image::SAMPLING_GUIDANCE,
                    field_type: FieldType::Number {
                        min: Some(0.0),
                        max: Some(30.0),
                    },
                    description: "CFG / guidance scale.",
                },
                FieldSpec {
                    path: image::STYLE_PRESET,
                    field_type: FieldType::String,
                    description: "Style preset name (e.g., 'photographic', 'anime').",
                },
                FieldSpec {
                    path: image::STYLE_QUALITY,
                    field_type: FieldType::String,
                    description: "Quality target ('low', 'medium', 'high').",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("prompt"),
                    to: image::PROMPT_POSITIVE,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("negative"),
                    to: image::PROMPT_NEGATIVE,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("width"),
                    to: image::DIMENSIONS_WIDTH,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("height"),
                    to: image::DIMENSIONS_HEIGHT,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("steps"),
                    to: image::SAMPLING_STEPS,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("seed"),
                    to: image::SAMPLING_SEED,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("guidance"),
                    to: image::SAMPLING_GUIDANCE,
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
                    description: "Media ID for the generated image.",
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
                FieldSpec {
                    path: image::SEED,
                    field_type: FieldType::Integer { min: None, max: None },
                    description: "Seed that was used (for reproduction).",
                },
                FieldSpec {
                    path: image::MODEL,
                    field_type: FieldType::String,
                    description: "Underlying model used for generation.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing],
        },
        example_minimal: json!({"prompt": "a serene mountain landscape at sunrise"}),
        example_full: json!({
            "image": {
                "prompt": {
                    "positive": "a serene mountain landscape at sunrise",
                    "negative": "blurry, lowres"
                },
                "dimensions": {"width": 1024, "height": 768},
                "sampling": {"steps": 30, "seed": 12345, "guidance": 7.5},
                "style": {"quality": "high"}
            }
        }),
    }
}
