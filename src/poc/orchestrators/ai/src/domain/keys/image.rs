//! Canonical field paths for the image modality.

use crate::domain::field_path::FieldPath;

// ── Input: sources ────────────────────────────────────────────

/// Primary source image (media reference).
pub const SOURCE: FieldPath = FieldPath::new("image.source");
/// Mask image for inpainting (media reference).
pub const MASK: FieldPath = FieldPath::new("image.mask");

// ── Input: prompts ────────────────────────────────────────────

/// Positive prompt describing the desired output.
pub const PROMPT_POSITIVE: FieldPath = FieldPath::new("image.prompt.positive");
/// Negative prompt describing what to avoid.
pub const PROMPT_NEGATIVE: FieldPath = FieldPath::new("image.prompt.negative");

// ── Input: dimensions ─────────────────────────────────────────

pub const DIMENSIONS_WIDTH: FieldPath = FieldPath::new("image.dimensions.width");
pub const DIMENSIONS_HEIGHT: FieldPath = FieldPath::new("image.dimensions.height");
pub const DIMENSIONS_ASPECT: FieldPath = FieldPath::new("image.dimensions.aspect");

// ── Input: sampling ───────────────────────────────────────────

pub const SAMPLING_STEPS: FieldPath = FieldPath::new("image.sampling.steps");
pub const SAMPLING_SEED: FieldPath = FieldPath::new("image.sampling.seed");
pub const SAMPLING_GUIDANCE: FieldPath = FieldPath::new("image.sampling.guidance");

// ── Input: style ──────────────────────────────────────────────

pub const STYLE_PRESET: FieldPath = FieldPath::new("image.style.preset");
pub const STYLE_QUALITY: FieldPath = FieldPath::new("image.style.quality");

// ── Input: upscale ────────────────────────────────────────────

/// Scale multiplier for upscaling.
pub const SCALE: FieldPath = FieldPath::new("image.scale");

// ── Output fields ─────────────────────────────────────────────

/// Generated or edited image as a media reference.
pub const MEDIA_ID: FieldPath = FieldPath::new("image.media_id");
pub const WIDTH: FieldPath = FieldPath::new("image.width");
pub const HEIGHT: FieldPath = FieldPath::new("image.height");
/// Seed that produced this image (for reproduction).
pub const SEED: FieldPath = FieldPath::new("image.seed");
/// Name of the underlying model used.
pub const MODEL: FieldPath = FieldPath::new("image.model");
