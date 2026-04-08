//! `text.chat` vocabulary — conversational text completion.

use serde_json::json;

use crate::domain::field_path::FieldPath;
use crate::domain::keys::{text, usage};
use crate::domain::primitive::Primitive;
use crate::domain::vocabulary::{
    Alias, AliasCondition, FieldSpec, FieldType, IoSchema, SharedNamespace, Vocabulary,
};

pub fn vocabulary() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::TextChat,
        summary: Primitive::TextChat.summary(),
        input: IoSchema {
            required: vec![FieldSpec {
                path: text::PROMPT_USER,
                field_type: FieldType::String,
                description: "The user's current turn in the conversation.",
            }],
            optional: vec![
                FieldSpec {
                    path: text::PROMPT_SYSTEM,
                    field_type: FieldType::String,
                    description: "System prompt providing persona or instructions.",
                },
                FieldSpec {
                    path: text::PROMPT_PREVIOUS,
                    field_type: FieldType::MessageHistory,
                    description: "Prior turns as an array of {user, assistant} pairs.",
                },
                FieldSpec {
                    path: text::TOKENS_MAX,
                    field_type: FieldType::Integer {
                        min: Some(1),
                        max: Some(200_000),
                    },
                    description: "Maximum output length in tokens.",
                },
                FieldSpec {
                    path: text::SAMPLING_TEMPERATURE,
                    field_type: FieldType::Number {
                        min: Some(0.0),
                        max: Some(2.0),
                    },
                    description: "Sampling temperature controlling randomness.",
                },
                FieldSpec {
                    path: text::SAMPLING_TOP_P,
                    field_type: FieldType::Number {
                        min: Some(0.0),
                        max: Some(1.0),
                    },
                    description: "Nucleus sampling probability threshold.",
                },
                FieldSpec {
                    path: text::SAMPLING_TOP_K,
                    field_type: FieldType::Integer {
                        min: Some(1),
                        max: None,
                    },
                    description: "Top-K sampling — keep the K highest-probability tokens.",
                },
                FieldSpec {
                    path: text::SAMPLING_SEED,
                    field_type: FieldType::Integer {
                        min: None,
                        max: None,
                    },
                    description: "Random seed for deterministic sampling.",
                },
                FieldSpec {
                    path: text::STOP_SEQUENCES,
                    field_type: FieldType::Array,
                    description: "Array of strings that end generation when seen.",
                },
                FieldSpec {
                    path: text::TOOLS_DEFINITIONS,
                    field_type: FieldType::Array,
                    description: "Tool/function definitions for function calling.",
                },
                FieldSpec {
                    path: text::TOOLS_CHOICE,
                    field_type: FieldType::String,
                    description: "Tool choice strategy: 'auto', 'required', or a tool name.",
                },
                FieldSpec {
                    path: text::FORMAT_RESPONSE,
                    field_type: FieldType::String,
                    description: "Response format hint: 'text' or 'json'.",
                },
                FieldSpec {
                    path: text::STREAM,
                    field_type: FieldType::Boolean,
                    description: "Request streaming delivery of tokens.",
                },
            ],
            aliases: vec![
                Alias {
                    from: FieldPath::new("prompt"),
                    to: text::PROMPT_USER,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("system"),
                    to: text::PROMPT_SYSTEM,
                    condition: AliasCondition::WhenString,
                },
                Alias {
                    from: FieldPath::new("temperature"),
                    to: text::SAMPLING_TEMPERATURE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("max_tokens"),
                    to: text::TOKENS_MAX,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("top_p"),
                    to: text::SAMPLING_TOP_P,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("top_k"),
                    to: text::SAMPLING_TOP_K,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("seed"),
                    to: text::SAMPLING_SEED,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("stop"),
                    to: text::STOP_SEQUENCES,
                    condition: AliasCondition::WhenArray,
                },
                Alias {
                    from: FieldPath::new("tools"),
                    to: text::TOOLS_DEFINITIONS,
                    condition: AliasCondition::WhenArray,
                },
                Alias {
                    from: FieldPath::new("stream"),
                    to: text::STREAM,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("messages"),
                    to: text::PROMPT_USER,
                    condition: AliasCondition::MessagesDecomposer,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing, SharedNamespace::Job, SharedNamespace::Stream],
        },
        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: text::RESPONSE,
                    field_type: FieldType::String,
                    description: "The assistant's reply text.",
                },
                FieldSpec {
                    path: text::FINISH_REASON,
                    field_type: FieldType::String,
                    description: "Why generation stopped: 'stop', 'length', 'tool_calls', 'content_filter'.",
                },
                FieldSpec {
                    path: text::TOOL_CALLS,
                    field_type: FieldType::Array,
                    description: "Tool calls the model wants to make.",
                },
                FieldSpec {
                    path: text::MEDIA_ID,
                    field_type: FieldType::String,
                    description: "Media ID for the full response (streaming/archive mode).",
                },
                FieldSpec {
                    path: usage::TOKENS_INPUT,
                    field_type: FieldType::Integer { min: Some(0), max: None },
                    description: "Input token count.",
                },
                FieldSpec {
                    path: usage::TOKENS_OUTPUT,
                    field_type: FieldType::Integer { min: Some(0), max: None },
                    description: "Output token count.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Meta, SharedNamespace::Usage, SharedNamespace::Timing],
        },
        example_minimal: json!({"prompt": "Hi!"}),
        example_full: json!({
            "text": {
                "prompt": {
                    "user": "What color is the sky in the evening?",
                    "system": "You are a helpful assistant.",
                    "previous": [
                        {"user": "hi", "assistant": "Hello! How can I help?"}
                    ]
                },
                "sampling": {"temperature": 0.7, "top_p": 0.95, "seed": 42},
                "tokens": {"max": 500},
                "stream": false
            }
        }),
    }
}
