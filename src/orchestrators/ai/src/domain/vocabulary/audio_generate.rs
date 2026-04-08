//! `audio.generate` vocabulary — text-to-speech.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::audio;
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::AudioGenerate,
        summary: Primitive::AudioGenerate.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: audio::TEXT,
                field_type: FieldType::String,
                description: "Text to synthesize.",
            }],
            optional: vec![
                FieldSpec {
                    path: audio::VOICE_ID,
                    field_type: FieldType::String,
                    description: "Voice identifier (provider-specific).",
                },
                FieldSpec {
                    path: audio::VOICE_STYLE,
                    field_type: FieldType::String,
                    description: "Voice style hint (e.g., 'friendly', 'dramatic').",
                },
                FieldSpec {
                    path: audio::VOICE_SPEED,
                    field_type: FieldType::Number {
                        min: Some(0.25),
                        max: Some(4.0),
                    },
                    description: "Playback speed multiplier (1.0 = normal).",
                },
                FieldSpec {
                    path: audio::FORMAT_CODEC,
                    field_type: FieldType::String,
                    description: "Output codec (mp3, wav, opus, flac).",
                },
                FieldSpec {
                    path: audio::FORMAT_SAMPLE_RATE,
                    field_type: FieldType::Integer {
                        min: Some(8_000),
                        max: Some(48_000),
                    },
                    description: "Sample rate in Hz.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("text"),
                    to: audio::TEXT,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("voice"),
                    to: audio::VOICE_ID,
                    condition: AliasCondition::WhenString,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing, SharedNamespace::Job, SharedNamespace::Stream],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: audio::MEDIA_ID,
                    field_type: FieldType::String,
                    description: "Media ID of the generated audio.",
                },
                FieldSpec {
                    path: audio::DURATION_MS,
                    field_type: FieldType::Integer { min: Some(0), max: None },
                    description: "Duration of the generated audio in milliseconds.",
                },
                FieldSpec {
                    path: audio::FORMAT,
                    field_type: FieldType::String,
                    description: "Actual output codec.",
                },
                FieldSpec {
                    path: audio::SAMPLE_RATE,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Actual output sample rate.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Timing],
        },
        example_minimal: json!({"audio": {"text": "Hello, world!"}}),
        example_full: json!({
            "audio": {
                "text": "Hello, world!",
                "voice": {"id": "en-us-female-1", "speed": 1.0},
                "format": {"codec": "mp3", "sample_rate": 24000}
            }
        }),
    }
}
