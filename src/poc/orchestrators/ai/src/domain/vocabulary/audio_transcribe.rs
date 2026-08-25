//! `audio.transcribe` vocabulary — speech-to-text.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::{audio, text};
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::AudioTranscribe,
        summary: Primitive::AudioTranscribe.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: audio::SOURCE,
                field_type: FieldType::MediaRef,
                description: "Audio to transcribe.",
            }],
            optional: vec![
                FieldSpec {
                    path: audio::LANGUAGE_SOURCE,
                    field_type: FieldType::String,
                    description: "Source language code (auto-detect if absent).",
                },
                FieldSpec {
                    path: text::FORMAT_RESPONSE,
                    field_type: FieldType::String,
                    description: "Response format hint.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("source"),
                    to: audio::SOURCE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("language"),
                    to: audio::LANGUAGE_SOURCE,
                    condition: AliasCondition::WhenString,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing, SharedNamespace::Job],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: text::RESPONSE,
                    field_type: FieldType::String,
                    description: "Transcribed text.",
                },
                FieldSpec {
                    path: text::LANGUAGE,
                    field_type: FieldType::String,
                    description: "Detected or declared language.",
                },
                FieldSpec {
                    path: text::SEGMENTS,
                    field_type: FieldType::Array,
                    description: "Array of timestamped segments, if the provider emits them.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing],
        },
        example_minimal: json!({"audio": {"source": {"media_id": "01JA7Z-example"}}}),
        example_full: json!({
            "audio": {
                "source": {"media_id": "01JA7Z-example"},
                "language": {"source": "en"}
            }
        }),
    }
}
