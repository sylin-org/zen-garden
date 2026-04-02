//! Skill import — extract workflows from various sources (ORCH-0023).
//!
//! Handles: CivitAI image URLs, direct PNG URLs, raw PNG bytes, raw JSON.
//! Extracts the ComfyUI API-format workflow, analyzes it, resolves models,
//! and creates a draft skill on disk.

pub mod png_extract;
pub mod civitai;
pub mod model_resolve;
pub mod analyze;
