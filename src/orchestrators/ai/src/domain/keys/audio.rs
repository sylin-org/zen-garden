//! Canonical field paths for the audio modality.

use crate::domain::field_path::FieldPath;

// ── Input ─────────────────────────────────────────────────────

/// Source audio (media reference) for transcription.
pub const SOURCE: FieldPath = FieldPath::new("audio.source");
/// Text to synthesize (for TTS).
pub const TEXT: FieldPath = FieldPath::new("audio.text");

/// Voice selector.
pub const VOICE_ID: FieldPath = FieldPath::new("audio.voice.id");
pub const VOICE_STYLE: FieldPath = FieldPath::new("audio.voice.style");
pub const VOICE_SPEED: FieldPath = FieldPath::new("audio.voice.speed");

/// Source language for transcription (auto-detect if absent).
pub const LANGUAGE_SOURCE: FieldPath = FieldPath::new("audio.language.source");

/// Output codec (mp3, wav, opus, ...).
pub const FORMAT_CODEC: FieldPath = FieldPath::new("audio.format.codec");
pub const FORMAT_SAMPLE_RATE: FieldPath = FieldPath::new("audio.format.sample_rate");

// ── Output ────────────────────────────────────────────────────

/// Generated audio as a media reference.
pub const MEDIA_ID: FieldPath = FieldPath::new("audio.media_id");
pub const DURATION_MS: FieldPath = FieldPath::new("audio.duration_ms");
pub const FORMAT: FieldPath = FieldPath::new("audio.format");
pub const SAMPLE_RATE: FieldPath = FieldPath::new("audio.sample_rate");
