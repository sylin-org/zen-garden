//! Canonical field paths for the text modality.

use crate::domain::field_path::FieldPath;

// ── Input fields ──────────────────────────────────────────────

/// User message or current conversation turn.
pub const PROMPT_USER: FieldPath = FieldPath::new("text.prompt.user");
/// System prompt (persona, instructions).
pub const PROMPT_SYSTEM: FieldPath = FieldPath::new("text.prompt.system");
/// Conversation history as an array of `{user, assistant}` dialogue turns.
pub const PROMPT_HISTORY: FieldPath = FieldPath::new("text.prompt.history");

/// Maximum output length in tokens.
pub const TOKENS_MAX: FieldPath = FieldPath::new("text.tokens.max");

/// Sampling temperature.
pub const SAMPLING_TEMPERATURE: FieldPath = FieldPath::new("text.sampling.temperature");
/// Nucleus sampling cutoff.
pub const SAMPLING_TOP_P: FieldPath = FieldPath::new("text.sampling.top_p");
/// Top-K sampling.
pub const SAMPLING_TOP_K: FieldPath = FieldPath::new("text.sampling.top_k");
/// Sampling seed for deterministic generation.
pub const SAMPLING_SEED: FieldPath = FieldPath::new("text.sampling.seed");

/// Stop sequences (array).
pub const STOP_SEQUENCES: FieldPath = FieldPath::new("text.stop.sequences");

/// Tool/function definitions (array).
pub const TOOLS_DEFINITIONS: FieldPath = FieldPath::new("text.tools.definitions");
/// Tool choice strategy (`auto`, `required`, or a tool name).
pub const TOOLS_CHOICE: FieldPath = FieldPath::new("text.tools.choice");

/// Response format hint (`text`, `json`).
pub const FORMAT_RESPONSE: FieldPath = FieldPath::new("text.format.response");

/// Stream delivery request.
pub const STREAM: FieldPath = FieldPath::new("text.stream");

// ── text.translate ─────────────────────────────────────────────

/// Body to translate.
pub const BODY: FieldPath = FieldPath::new("text.body");
/// Source language code (optional; providers may auto-detect).
pub const LANGUAGE_SOURCE: FieldPath = FieldPath::new("text.language.source");
/// Target language code (required).
pub const LANGUAGE_TARGET: FieldPath = FieldPath::new("text.language.target");

// ── text.embed ────────────────────────────────────────────────

/// Embedding input — a string or an array of strings.
pub const INPUT: FieldPath = FieldPath::new("text.input");
/// Desired embedding dimensionality (provider-dependent).
pub const DIMENSIONS: FieldPath = FieldPath::new("text.dimensions");

// ── text.rerank ───────────────────────────────────────────────

/// Query for reranking.
pub const QUERY: FieldPath = FieldPath::new("text.query");
/// Documents to rerank.
pub const DOCUMENTS: FieldPath = FieldPath::new("text.documents");
/// Number of top results to keep.
pub const RESULTS_TOP_K: FieldPath = FieldPath::new("text.results.top_k");
/// Minimum score threshold.
pub const RESULTS_MIN_SCORE: FieldPath = FieldPath::new("text.results.min_score");

// ── Output fields ─────────────────────────────────────────────

/// Primary text response.
pub const RESPONSE: FieldPath = FieldPath::new("text.response");
/// Reasoning-model chain-of-thought, emitted separately from the
/// final response. Populated only when the caller asked for it
/// (via [`REASONING_THINK`]) and the provider supports it.
pub const REASONING: FieldPath = FieldPath::new("text.reasoning.content");
/// Input flag asking a reasoning-capable model to emit its
/// chain-of-thought in a separate `text.reasoning.content` output
/// field. Providers without a "thinking" capability silently ignore
/// this flag.
pub const REASONING_THINK: FieldPath = FieldPath::new("text.reasoning.think");
/// Why generation stopped.
pub const FINISH_REASON: FieldPath = FieldPath::new("text.finish_reason");
/// Tool calls the model wants to make.
pub const TOOL_CALLS: FieldPath = FieldPath::new("text.tool_calls");
/// Translated body.
pub const TRANSLATED: FieldPath = FieldPath::new("text.translated");
/// Detected source language (populated when auto-detection is used).
pub const DETECTED_LANGUAGE: FieldPath = FieldPath::new("text.detected_language");
/// Embedding vectors (array of arrays).
pub const EMBEDDINGS: FieldPath = FieldPath::new("text.embeddings");
/// Rerank segments (array of `{index, score, document}` objects).
pub const SEGMENTS: FieldPath = FieldPath::new("text.segments");
/// Primary language field (for transcription output).
pub const LANGUAGE: FieldPath = FieldPath::new("text.language");
/// Media ID for the full response (streaming/archive mode).
pub const MEDIA_ID: FieldPath = FieldPath::new("text.media_id");

pub mod values {
    //! Enumerated string values for text-output fields.
    pub const FINISH_REASON_STOP: &str = "stop";
    pub const FINISH_REASON_LENGTH: &str = "length";
    pub const FINISH_REASON_TOOL_CALLS: &str = "tool_calls";
    pub const FINISH_REASON_CONTENT_FILTER: &str = "content_filter";
    pub const FORMAT_RESPONSE_TEXT: &str = "text";
    pub const FORMAT_RESPONSE_JSON: &str = "json";
    pub const TOOLS_CHOICE_AUTO: &str = "auto";
    pub const TOOLS_CHOICE_REQUIRED: &str = "required";
}
