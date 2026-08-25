//! `image.upscale` vocabulary.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::image;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::ImageUpscale,
        summary: Primitive::ImageUpscale.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: image::SOURCE,
                field_type: FieldType::MediaRef,
                description: "Source image to upscale.",
            }],
            optional: vec![FieldSpec {
                path: image::SCALE,
                field_type: FieldType::Integer {
                    min: Some(2),
                    max: Some(8),
                },
                description: "Scale multiplier (2x, 4x, 8x).",
            }],
            aliases: vec![
                Alias {
                    from: FieldPath::new("source"),
                    to: image::SOURCE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("scale"),
                    to: image::SCALE,
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
                    description: "Media ID of the upscaled image.",
                },
                FieldSpec {
                    path: image::WIDTH,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Output width.",
                },
                FieldSpec {
                    path: image::HEIGHT,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Output height.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing],
        },
        example_minimal: json!({
            "image": {"source": {"media_id": "01JA7X-example"}, "scale": 4}
        }),
        example_full: json!({
            "image": {"source": {"media_id": "01JA7X-example"}, "scale": 4}
        }),
    }
}
